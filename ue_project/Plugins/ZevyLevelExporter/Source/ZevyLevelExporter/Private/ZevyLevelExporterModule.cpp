#include "ZevyLevelExporterModule.h"

#include "Components/DirectionalLightComponent.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Components/LightComponent.h"
#include "Components/LocalLightComponent.h"
#include "Components/PointLightComponent.h"
#include "Components/SceneComponent.h"
#include "Components/SplineMeshComponent.h"
#include "Components/SpotLightComponent.h"
#include "Components/StaticMeshComponent.h"
#include "DesktopPlatformModule.h"
#include "Editor.h"
#include "Engine/Engine.h"
#include "Engine/Level.h"
#include "Engine/Scene.h"
#include "Engine/World.h"
#include "EngineUtils.h"
#include "Exporters/GLTFExporter.h"
#include "Framework/Application/SlateApplication.h"
#include "Framework/Notifications/NotificationManager.h"
#include "HAL/FileManager.h"
#include "IDesktopPlatform.h"
#include "LevelInstance/LevelInstanceActor.h"
#include "Materials/MaterialInterface.h"
#include "Misc/App.h"
#include "Misc/DateTime.h"
#include "Misc/EngineVersion.h"
#include "Misc/FileHelper.h"
#include "Misc/MessageDialog.h"
#include "Misc/PackageName.h"
#include "Misc/Paths.h"
#include "Misc/SecureHash.h"
#include "Options/GLTFExportOptions.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "ToolMenus.h"
#include "UObject/StrongObjectPtr.h"
#include "Widgets/Notifications/SNotificationList.h"

#define LOCTEXT_NAMESPACE "FZevyLevelExporterModule"

DEFINE_LOG_CATEGORY_STATIC(LogZevyLevelExporter, Log, All);

namespace ZevyLevelExporter
{
constexpr int32 MonolithicManifestSchemaVersion = 1;
constexpr int32 SplitManifestSchemaVersion = 2;

struct FContentSummary
{
    int32 StaticMeshActors = 0;
    int32 StaticMeshComponents = 0;
    int32 UniqueMaterials = 0;
    int32 DirectionalLights = 0;
    int32 PointLights = 0;
    int32 SpotLights = 0;
    int32 UnsupportedLights = 0;
};

struct FSplitAssetRecord
{
    FString Id;
    FString Name;
    FString FolderName;
    FString RelativeScenePath;
    AActor* RepresentativeActor = nullptr;
};

FString GetLevelName(const UWorld* World)
{
    if (World == nullptr)
    {
        return TEXT("UntitledLevel");
    }

    const FString PackageName = World->GetOutermost()->GetName();
    return FPaths::MakeValidFileName(FPackageName::GetShortName(PackageName));
}

FString GetDefaultExportDirectory(const UWorld* World)
{
    const FString LevelName = GetLevelName(World);
    const FString SiblingEngineRoot = FPaths::ConvertRelativePathToFull(
        FPaths::Combine(FPaths::ProjectDir(), TEXT("../zevy_engine")));

    FString ExportDirectory;
    if (IFileManager::Get().FileExists(*FPaths::Combine(SiblingEngineRoot, TEXT("Cargo.toml"))))
    {
        ExportDirectory = FPaths::Combine(
            SiblingEngineRoot,
            TEXT("assets/levels"),
            LevelName);
    }
    else
    {
        ExportDirectory = FPaths::Combine(
            FPaths::ProjectSavedDir(),
            TEXT("ZevyExports"),
            LevelName);
    }

    IFileManager::Get().MakeDirectory(*ExportDirectory, true);
    return ExportDirectory;
}

FString GetActorDisplayName(const AActor* Actor)
{
    if (!IsValid(Actor))
    {
        return TEXT("Actor");
    }

    FString Name = Actor->GetActorLabel();
    if (Name.IsEmpty())
    {
        Name = Actor->GetName();
    }
    return Name;
}

FString GetStableActorId(const AActor* Actor)
{
    if (IsValid(Actor))
    {
        const FGuid ActorGuid = Actor->GetActorInstanceGuid();
        if (ActorGuid.IsValid())
        {
            return FString(TEXT("actor_")) +
                ActorGuid.ToString(EGuidFormats::Digits).ToLower();
        }
    }

    return FString(TEXT("actor_")) +
        FMD5::HashAnsiString(IsValid(Actor) ? *Actor->GetPathName() : TEXT("invalid"));
}

bool IsMainLevelActor(const AActor* Actor)
{
    if (!IsValid(Actor) || Actor->IsEditorOnly())
    {
        return false;
    }

    const ULevel* Level = Actor->GetLevel();
    return Level != nullptr && (!Level->IsInstancedLevel() || Level->IsCurrentLevel());
}

bool HasSupportedLight(const AActor* Actor)
{
    TArray<ULightComponent*> LightComponents;
    Actor->GetComponents<ULightComponent>(LightComponents);
    for (const ULightComponent* Component : LightComponents)
    {
        if (!IsValid(Component) || Component->IsEditorOnly())
        {
            continue;
        }

        if (Component->IsA<UDirectionalLightComponent>() ||
            Component->IsA<UPointLightComponent>() ||
            Component->IsA<USpotLightComponent>())
        {
            return true;
        }
    }
    return false;
}

bool HasExportableContent(const AActor* Actor)
{
    if (!IsMainLevelActor(Actor))
    {
        return false;
    }

    if (const ALevelInstance* LevelInstance = Cast<ALevelInstance>(Actor))
    {
        return LevelInstance->GetLoadedLevel() != nullptr;
    }

    TArray<UStaticMeshComponent*> StaticMeshComponents;
    Actor->GetComponents<UStaticMeshComponent>(StaticMeshComponents);
    for (const UStaticMeshComponent* Component : StaticMeshComponents)
    {
        if (IsValid(Component) && !Component->IsEditorOnly() &&
            Component->GetStaticMesh() != nullptr)
        {
            return true;
        }
    }

    return HasSupportedLight(Actor);
}

FString TransformSignature(const FTransform& Transform)
{
    const FVector Translation = Transform.GetTranslation();
    const FQuat Rotation = Transform.GetRotation().GetNormalized();
    const FVector Scale = Transform.GetScale3D();
    return FString::Printf(
        TEXT("T%.9g,%.9g,%.9g|R%.9g,%.9g,%.9g,%.9g|S%.9g,%.9g,%.9g"),
        Translation.X,
        Translation.Y,
        Translation.Z,
        Rotation.X,
        Rotation.Y,
        Rotation.Z,
        Rotation.W,
        Scale.X,
        Scale.Y,
        Scale.Z);
}

FString BuildActorContentSignature(const AActor* Actor)
{
    FString Signature = Actor->GetClass()->GetPathName();
    bool bRequiresUniqueAsset = Actor->IsA<ALevelInstance>() || HasSupportedLight(Actor);

    if (const ALevelInstance* LevelInstance = Cast<ALevelInstance>(Actor))
    {
        Signature += TEXT("|level_instance:");
        Signature += LevelInstance->GetWorldAsset().ToSoftObjectPath().ToString();
    }

    TArray<UStaticMeshComponent*> Components;
    Actor->GetComponents<UStaticMeshComponent>(Components);
    Components.Sort([](const UStaticMeshComponent& Left, const UStaticMeshComponent& Right)
    {
        return Left.GetName() < Right.GetName();
    });

    for (const UStaticMeshComponent* Component : Components)
    {
        if (!IsValid(Component) || Component->IsEditorOnly() ||
            Component->GetStaticMesh() == nullptr)
        {
            continue;
        }

        Signature += TEXT("|component:");
        Signature += Component->GetClass()->GetPathName();
        Signature += TEXT(":");
        Signature += Component->GetName();
        Signature += TEXT(":mesh=");
        Signature += Component->GetStaticMesh()->GetPathName();

        if (Component != Actor->GetRootComponent())
        {
            Signature += TEXT(":local=");
            Signature += TransformSignature(Component->GetRelativeTransform());
            if (const USceneComponent* AttachParent = Component->GetAttachParent())
            {
                Signature += TEXT(":parent_component=");
                Signature += AttachParent->GetName();
            }
        }

        for (int32 MaterialIndex = 0; MaterialIndex < Component->GetNumMaterials(); ++MaterialIndex)
        {
            Signature += FString::Printf(TEXT(":material%d="), MaterialIndex);
            if (const UMaterialInterface* Material = Component->GetMaterial(MaterialIndex))
            {
                Signature += Material->GetPathName();
            }
            else
            {
                Signature += TEXT("null");
            }
        }

        if (Component->IsA<UInstancedStaticMeshComponent>() ||
            Component->IsA<USplineMeshComponent>())
        {
            bRequiresUniqueAsset = true;
        }
    }

    if (bRequiresUniqueAsset)
    {
        Signature += TEXT("|unique=");
        Signature += GetStableActorId(Actor);
    }

    return Signature;
}

void AddLevelInstanceActorsRecursive(AActor* Actor, TSet<AActor*>& ActorsToExport)
{
    if (!IsValid(Actor) || ActorsToExport.Contains(Actor))
    {
        return;
    }

    ActorsToExport.Add(Actor);
    const ALevelInstance* LevelInstance = Cast<ALevelInstance>(Actor);
    if (LevelInstance == nullptr)
    {
        return;
    }

    const ULevel* LoadedLevel = LevelInstance->GetLoadedLevel();
    if (LoadedLevel == nullptr)
    {
        return;
    }

    for (AActor* LevelActor : LoadedLevel->Actors)
    {
        if (IsValid(LevelActor) && !LevelActor->IsEditorOnly())
        {
            AddLevelInstanceActorsRecursive(LevelActor, ActorsToExport);
        }
    }
}

void ConfigureExportOptions(UGLTFExportOptions* Options)
{
    check(Options != nullptr);
    Options->ResetToDefault();
    Options->ExportUniformScale = 0.01f;
    Options->bExportPreviewMesh = false;
    Options->bSkipNearDefaultValues = false;
    Options->bExportHiddenInGame = false;
    Options->bExportCameras = false;
    Options->bExportLights = true;
    Options->bExportLevelSequences = false;
    Options->bExportAnimationSequences = false;
    Options->bExportVertexSkinWeights = false;
    Options->bUseMeshQuantization = false;
    Options->TextureImageFormat = EGLTFTextureImageFormat::PNG;
    Options->bExportTextureTransforms = true;
    Options->bAdjustNormalmaps = true;
    Options->BakeMaterialInputs = EGLTFMaterialBakeMode::UseMeshData;
    Options->ExportMaterialVariants = EGLTFMaterialVariantMode::None;
}

TArray<TSharedPtr<FJsonValue>> ToJsonVector3(const FVector& Value, const double Scale)
{
    TArray<TSharedPtr<FJsonValue>> Values;
    Values.Add(MakeShared<FJsonValueNumber>(Value.X * Scale));
    Values.Add(MakeShared<FJsonValueNumber>(Value.Z * Scale));
    Values.Add(MakeShared<FJsonValueNumber>(Value.Y * Scale));
    return Values;
}

TArray<TSharedPtr<FJsonValue>> ToJsonRotation(const FQuat& Value)
{
    const FQuat Normalized = Value.GetNormalized();
    TArray<TSharedPtr<FJsonValue>> Values;
    Values.Add(MakeShared<FJsonValueNumber>(-Normalized.X));
    Values.Add(MakeShared<FJsonValueNumber>(-Normalized.Z));
    Values.Add(MakeShared<FJsonValueNumber>(-Normalized.Y));
    Values.Add(MakeShared<FJsonValueNumber>(Normalized.W));
    return Values;
}

TSharedRef<FJsonObject> ToJsonTransform(const FTransform& Transform)
{
    TSharedRef<FJsonObject> Json = MakeShared<FJsonObject>();
    Json->SetArrayField(TEXT("translation"), ToJsonVector3(Transform.GetTranslation(), 0.01));
    Json->SetArrayField(TEXT("rotation"), ToJsonRotation(Transform.GetRotation()));
    Json->SetArrayField(TEXT("scale"), ToJsonVector3(Transform.GetScale3D(), 1.0));
    return Json;
}

TArray<TSharedPtr<FJsonValue>> ToJsonColor3(const FLinearColor& Color)
{
    TArray<TSharedPtr<FJsonValue>> Values;
    Values.Add(MakeShared<FJsonValueNumber>(FMath::Clamp(Color.R, 0.0f, 1.0f)));
    Values.Add(MakeShared<FJsonValueNumber>(FMath::Clamp(Color.G, 0.0f, 1.0f)));
    Values.Add(MakeShared<FJsonValueNumber>(FMath::Clamp(Color.B, 0.0f, 1.0f)));
    return Values;
}

FString LightUnitsToString(const ELightUnits Units)
{
    switch (Units)
    {
    case ELightUnits::Candelas:
        return TEXT("candelas");
    case ELightUnits::Lumens:
        return TEXT("lumens");
    case ELightUnits::EV:
        return TEXT("ev100");
    case ELightUnits::Unitless:
    default:
        return TEXT("unitless");
    }
}

FString MobilityToString(const EComponentMobility::Type Mobility)
{
    switch (Mobility)
    {
    case EComponentMobility::Static:
        return TEXT("static");
    case EComponentMobility::Stationary:
        return TEXT("stationary");
    case EComponentMobility::Movable:
    default:
        return TEXT("movable");
    }
}

FString GetGltfLightName(const ULightComponent* LightComponent)
{
    const AActor* Owner = IsValid(LightComponent) ? LightComponent->GetOwner() : nullptr;
    if (IsValid(Owner) && LightComponent == Owner->GetRootComponent())
    {
        return GetActorDisplayName(Owner);
    }
    return IsValid(LightComponent) ? LightComponent->GetName() : TEXT("Light");
}

TSharedRef<FJsonObject> ToJsonLightDefinition(const ULightComponent* LightComponent)
{
    const UPointLightComponent* PointLight = Cast<UPointLightComponent>(LightComponent);
    const USpotLightComponent* SpotLight = Cast<USpotLightComponent>(LightComponent);
    const UDirectionalLightComponent* DirectionalLight =
        Cast<UDirectionalLightComponent>(LightComponent);
    const ULocalLightComponent* LocalLight = Cast<ULocalLightComponent>(LightComponent);

    FString Kind = TEXT("unsupported");
    if (SpotLight != nullptr)
    {
        Kind = TEXT("spot");
    }
    else if (PointLight != nullptr)
    {
        Kind = TEXT("point");
    }
    else if (DirectionalLight != nullptr)
    {
        Kind = TEXT("directional");
    }

    const bool bDirectional = DirectionalLight != nullptr;
    const float ConversionScale = bDirectional ? 1.0f : 0.0001f;
    const FLinearColor ColorBrightness =
        LightComponent->GetColoredLightBrightness() * ConversionScale;
    const float GltfIntensity = FMath::Max(
        FMath::Max(ColorBrightness.R, FMath::Max(ColorBrightness.G, ColorBrightness.B)),
        1.0f);
    const FLinearColor GltfColor = ColorBrightness / GltfIntensity;
    const float BevyIntensity = bDirectional
        ? GltfIntensity
        : GltfIntensity * 4.0f * UE_PI;

    TSharedRef<FJsonObject> Root = MakeShared<FJsonObject>();
    Root->SetStringField(TEXT("component_name"), LightComponent->GetName());
    Root->SetStringField(TEXT("gltf_name"), GetGltfLightName(LightComponent));
    Root->SetStringField(TEXT("kind"), Kind);

    TSharedRef<FJsonObject> Bevy = MakeShared<FJsonObject>();
    Bevy->SetArrayField(TEXT("color_srgb"), ToJsonColor3(GltfColor));
    Bevy->SetNumberField(TEXT("intensity"), BevyIntensity);
    Bevy->SetStringField(TEXT("intensity_unit"), bDirectional ? TEXT("lux") : TEXT("lumens"));
    Bevy->SetStringField(TEXT("attenuation_model"), TEXT("inverse_square_cutoff"));
    Bevy->SetBoolField(TEXT("enabled"), LightComponent->bAffectsWorld);
    Bevy->SetBoolField(TEXT("shadows_enabled"), LightComponent->CastShadows);

    if (PointLight != nullptr)
    {
        Bevy->SetNumberField(TEXT("range_m"), PointLight->AttenuationRadius * 0.01f);
        Bevy->SetNumberField(TEXT("radius_m"), PointLight->SourceRadius * 0.01f);
    }
    if (SpotLight != nullptr)
    {
        Bevy->SetNumberField(
            TEXT("inner_angle_radians"),
            FMath::DegreesToRadians(SpotLight->InnerConeAngle));
        Bevy->SetNumberField(
            TEXT("outer_angle_radians"),
            FMath::DegreesToRadians(SpotLight->OuterConeAngle));
    }
    Root->SetObjectField(TEXT("bevy"), Bevy);

    const FColor SourceColor = LightComponent->LightColor;
    const FLinearColor SourceSrgb(
        SourceColor.R / 255.0f,
        SourceColor.G / 255.0f,
        SourceColor.B / 255.0f,
        1.0f);
    TSharedRef<FJsonObject> Unreal = MakeShared<FJsonObject>();
    Unreal->SetNumberField(TEXT("intensity"), LightComponent->Intensity);
    Unreal->SetStringField(
        TEXT("intensity_units"),
        LocalLight != nullptr
            ? LightUnitsToString(LocalLight->IntensityUnits)
            : TEXT("lux"));
    Unreal->SetArrayField(TEXT("light_color_srgb"), ToJsonColor3(SourceSrgb));
    Unreal->SetBoolField(TEXT("use_temperature"), LightComponent->bUseTemperature);
    Unreal->SetNumberField(TEXT("temperature_kelvin"), LightComponent->Temperature);
    Unreal->SetBoolField(TEXT("affects_world"), LightComponent->bAffectsWorld);
    Unreal->SetBoolField(TEXT("casts_shadows"), LightComponent->CastShadows);
    Unreal->SetStringField(TEXT("mobility"), MobilityToString(LightComponent->Mobility));
    Unreal->SetNumberField(TEXT("shadow_bias"), LightComponent->ShadowBias);
    Unreal->SetNumberField(TEXT("shadow_slope_bias"), LightComponent->ShadowSlopeBias);

    if (LocalLight != nullptr)
    {
        Unreal->SetNumberField(TEXT("attenuation_radius_m"), LocalLight->AttenuationRadius * 0.01f);
        Unreal->SetNumberField(TEXT("inverse_exposure_blend"), LocalLight->InverseExposureBlend);
    }
    if (PointLight != nullptr)
    {
        Unreal->SetStringField(
            TEXT("attenuation_model"),
            PointLight->bUseInverseSquaredFalloff
                ? TEXT("inverse_square")
                : TEXT("custom_exponent"));
        Unreal->SetNumberField(TEXT("falloff_exponent"), PointLight->LightFalloffExponent);
        Unreal->SetNumberField(TEXT("source_radius_m"), PointLight->SourceRadius * 0.01f);
        Unreal->SetNumberField(
            TEXT("soft_source_radius_m"),
            PointLight->SoftSourceRadius * 0.01f);
        Unreal->SetNumberField(TEXT("source_length_m"), PointLight->SourceLength * 0.01f);
    }
    if (SpotLight != nullptr)
    {
        Unreal->SetNumberField(TEXT("inner_cone_angle_degrees"), SpotLight->InnerConeAngle);
        Unreal->SetNumberField(TEXT("outer_cone_angle_degrees"), SpotLight->OuterConeAngle);
    }
    Root->SetObjectField(TEXT("unreal"), Unreal);
    return Root;
}

void AppendExportMessages(
    FGLTFExportMessages& Combined,
    const FGLTFExportMessages& Messages,
    const FString& AssetName)
{
    for (const FString& Message : Messages.Suggestions)
    {
        Combined.Suggestions.Add(FString::Printf(TEXT("[%s] %s"), *AssetName, *Message));
    }
    for (const FString& Message : Messages.Warnings)
    {
        Combined.Warnings.Add(FString::Printf(TEXT("[%s] %s"), *AssetName, *Message));
    }
    for (const FString& Message : Messages.Errors)
    {
        Combined.Errors.Add(FString::Printf(TEXT("[%s] %s"), *AssetName, *Message));
    }
}

FContentSummary CollectContentSummary(UWorld* World)
{
    FContentSummary Summary;
    TSet<const UMaterialInterface*> UniqueMaterials;

    for (TActorIterator<AActor> ActorIterator(World); ActorIterator; ++ActorIterator)
    {
        AActor* Actor = *ActorIterator;
        if (!IsValid(Actor) || Actor->IsEditorOnly())
        {
            continue;
        }

        TArray<UStaticMeshComponent*> StaticMeshComponents;
        Actor->GetComponents<UStaticMeshComponent>(StaticMeshComponents);

        bool bHasExportableStaticMesh = false;
        for (UStaticMeshComponent* Component : StaticMeshComponents)
        {
            if (!IsValid(Component) || Component->IsEditorOnly() || Component->GetStaticMesh() == nullptr)
            {
                continue;
            }

            bHasExportableStaticMesh = true;
            ++Summary.StaticMeshComponents;

            for (int32 MaterialIndex = 0; MaterialIndex < Component->GetNumMaterials(); ++MaterialIndex)
            {
                if (const UMaterialInterface* Material = Component->GetMaterial(MaterialIndex))
                {
                    UniqueMaterials.Add(Material);
                }
            }
        }

        if (bHasExportableStaticMesh)
        {
            ++Summary.StaticMeshActors;
        }

        TArray<ULightComponent*> LightComponents;
        Actor->GetComponents<ULightComponent>(LightComponents);
        for (ULightComponent* LightComponent : LightComponents)
        {
            if (!IsValid(LightComponent) || LightComponent->IsEditorOnly())
            {
                continue;
            }

            if (LightComponent->IsA<UDirectionalLightComponent>())
            {
                ++Summary.DirectionalLights;
            }
            else if (LightComponent->IsA<USpotLightComponent>())
            {
                ++Summary.SpotLights;
            }
            else if (LightComponent->IsA<UPointLightComponent>())
            {
                ++Summary.PointLights;
            }
            else
            {
                ++Summary.UnsupportedLights;
            }
        }
    }

    Summary.UniqueMaterials = UniqueMaterials.Num();
    return Summary;
}

TArray<TSharedPtr<FJsonValue>> ToJsonStringArray(const TArray<FString>& Messages)
{
    TArray<TSharedPtr<FJsonValue>> Values;
    Values.Reserve(Messages.Num());
    for (const FString& Message : Messages)
    {
        Values.Add(MakeShared<FJsonValueString>(Message));
    }
    return Values;
}

void LogExportMessages(const FGLTFExportMessages& Messages)
{
    for (const FString& Message : Messages.Suggestions)
    {
        UE_LOG(LogZevyLevelExporter, Display, TEXT("glTF suggestion: %s"), *Message);
    }
    for (const FString& Message : Messages.Warnings)
    {
        UE_LOG(LogZevyLevelExporter, Warning, TEXT("glTF warning: %s"), *Message);
    }
    for (const FString& Message : Messages.Errors)
    {
        UE_LOG(LogZevyLevelExporter, Error, TEXT("glTF error: %s"), *Message);
    }
}

bool WriteManifest(
    UWorld* World,
    const FString& GlbPath,
    const FContentSummary& Summary,
    const FGLTFExportMessages& Messages,
    FString& OutManifestPath)
{
    OutManifestPath = FPaths::ChangeExtension(GlbPath, TEXT("zevy-level.json"));
    const FString ExportedLevelName = FPaths::GetBaseFilename(GlbPath);

    TSharedRef<FJsonObject> Root = MakeShared<FJsonObject>();
    Root->SetNumberField(TEXT("schema_version"), MonolithicManifestSchemaVersion);
    Root->SetStringField(TEXT("level_name"), ExportedLevelName);
    Root->SetStringField(TEXT("scene"), FPaths::GetCleanFilename(GlbPath));
    Root->SetNumberField(TEXT("scene_index"), 0);

    TSharedRef<FJsonObject> Source = MakeShared<FJsonObject>();
    Source->SetStringField(TEXT("unreal_engine_version"), FEngineVersion::Current().ToString());
    Source->SetStringField(TEXT("project_name"), FApp::GetProjectName());
    Source->SetStringField(TEXT("map_package"), World->GetOutermost()->GetName());
    Source->SetStringField(TEXT("exported_at_utc"), FDateTime::UtcNow().ToIso8601());
    Root->SetObjectField(TEXT("source"), Source);

    TSharedRef<FJsonObject> Content = MakeShared<FJsonObject>();
    Content->SetNumberField(TEXT("static_mesh_actors"), Summary.StaticMeshActors);
    Content->SetNumberField(TEXT("static_mesh_components"), Summary.StaticMeshComponents);
    Content->SetNumberField(TEXT("unique_materials"), Summary.UniqueMaterials);
    Content->SetNumberField(TEXT("directional_lights"), Summary.DirectionalLights);
    Content->SetNumberField(TEXT("point_lights"), Summary.PointLights);
    Content->SetNumberField(TEXT("spot_lights"), Summary.SpotLights);
    Content->SetNumberField(TEXT("unsupported_lights"), Summary.UnsupportedLights);
    Root->SetObjectField(TEXT("content"), Content);

    TSharedRef<FJsonObject> Export = MakeShared<FJsonObject>();
    Export->SetStringField(TEXT("format"), TEXT("glb"));
    Export->SetStringField(TEXT("coordinate_system"), TEXT("right-handed-y-up"));
    Export->SetNumberField(TEXT("unit_scale"), 0.01);
    Export->SetStringField(TEXT("texture_format"), TEXT("png"));
    Export->SetStringField(TEXT("material_bake_mode"), TEXT("use_mesh_data"));
    Export->SetBoolField(TEXT("normal_maps_adjusted"), true);
    Export->SetBoolField(TEXT("lights_exported"), true);
    Root->SetObjectField(TEXT("export"), Export);

    TSharedRef<FJsonObject> Diagnostics = MakeShared<FJsonObject>();
    Diagnostics->SetArrayField(TEXT("suggestions"), ToJsonStringArray(Messages.Suggestions));
    Diagnostics->SetArrayField(TEXT("warnings"), ToJsonStringArray(Messages.Warnings));
    Diagnostics->SetArrayField(TEXT("errors"), ToJsonStringArray(Messages.Errors));
    Root->SetObjectField(TEXT("diagnostics"), Diagnostics);

    FString JsonText;
    const TSharedRef<TJsonWriter<TCHAR, TPrettyJsonPrintPolicy<TCHAR>>> Writer =
        TJsonWriterFactory<TCHAR, TPrettyJsonPrintPolicy<TCHAR>>::Create(&JsonText);
    if (!FJsonSerializer::Serialize(Root, Writer))
    {
        UE_LOG(LogZevyLevelExporter, Error, TEXT("Failed to serialize manifest JSON"));
        return false;
    }

    if (!FFileHelper::SaveStringToFile(
            JsonText,
            *OutManifestPath,
            FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM))
    {
        UE_LOG(
            LogZevyLevelExporter,
            Error,
            TEXT("Failed to write Zevy Level manifest: %s"),
            *OutManifestPath);
        return false;
    }

    return true;
}

bool ExportWorld(UWorld* World, const FString& RequestedGlbPath, FString& OutManifestPath)
{
    if (!IsValid(World))
    {
        UE_LOG(LogZevyLevelExporter, Error, TEXT("No valid editor World is available to export"));
        return false;
    }

    FString GlbPath = RequestedGlbPath;
    if (!GlbPath.EndsWith(TEXT(".glb"), ESearchCase::IgnoreCase))
    {
        GlbPath += TEXT(".glb");
    }
    GlbPath = FPaths::ConvertRelativePathToFull(GlbPath);

    if (!IFileManager::Get().MakeDirectory(*FPaths::GetPath(GlbPath), true))
    {
        UE_LOG(LogZevyLevelExporter, Error, TEXT("Failed to create export directory for %s"), *GlbPath);
        return false;
    }

    TStrongObjectPtr<UGLTFExportOptions> Options(NewObject<UGLTFExportOptions>());
    ConfigureExportOptions(Options.Get());

    const TSet<AActor*> ActorsToExport;
    FGLTFExportMessages Messages;
    const bool bExported = UGLTFExporter::ExportToGLTF(
        World,
        GlbPath,
        Options.Get(),
        ActorsToExport,
        Messages);
    LogExportMessages(Messages);

    if (!bExported)
    {
        UE_LOG(LogZevyLevelExporter, Error, TEXT("Failed to export World to %s"), *GlbPath);
        return false;
    }

    const FContentSummary Summary = CollectContentSummary(World);
    if (!WriteManifest(World, GlbPath, Summary, Messages, OutManifestPath))
    {
        return false;
    }

    UE_LOG(
        LogZevyLevelExporter,
        Display,
        TEXT("Exported Zevy Level '%s': GLB=%s manifest=%s"),
        *FPaths::GetBaseFilename(GlbPath),
        *GlbPath,
        *OutManifestPath);
    return true;
}

bool ExportActorAsset(
    UWorld* World,
    const FSplitAssetRecord& Asset,
    const FString& ExportDirectory,
    UGLTFExportOptions* Options,
    FGLTFExportMessages& OutCombinedMessages)
{
    AActor* Actor = Asset.RepresentativeActor;
    USceneComponent* RootComponent = IsValid(Actor) ? Actor->GetRootComponent() : nullptr;
    if (!IsValid(Actor) || RootComponent == nullptr)
    {
        UE_LOG(
            LogZevyLevelExporter,
            Error,
            TEXT("Cannot export asset '%s': representative Actor has no root component"),
            *Asset.Name);
        return false;
    }

    FString RelativeScenePath = Asset.RelativeScenePath;
    FPaths::MakePlatformFilename(RelativeScenePath);
    const FString ScenePath = FPaths::Combine(ExportDirectory, RelativeScenePath);
    if (!IFileManager::Get().MakeDirectory(*FPaths::GetPath(ScenePath), true))
    {
        UE_LOG(
            LogZevyLevelExporter,
            Error,
            TEXT("Failed to create asset directory for %s"),
            *ScenePath);
        return false;
    }

    TSet<AActor*> ActorsToExport;
    AddLevelInstanceActorsRecursive(Actor, ActorsToExport);

    const FTransform OriginalWorldTransform = Actor->GetActorTransform();
    const FTransform OriginalRelativeTransform = RootComponent->GetRelativeTransform();
    const TWeakObjectPtr<USceneComponent> OriginalAttachParent = RootComponent->GetAttachParent();
    const FName OriginalAttachSocket = RootComponent->GetAttachSocketName();

    Actor->DetachFromActor(FDetachmentTransformRules::KeepWorldTransform);
    Actor->SetActorTransform(FTransform::Identity, false, nullptr, ETeleportType::TeleportPhysics);

    FGLTFExportMessages Messages;
    const bool bExported = UGLTFExporter::ExportToGLTF(
        World,
        ScenePath,
        Options,
        ActorsToExport,
        Messages);

    Actor->SetActorTransform(
        OriginalWorldTransform,
        false,
        nullptr,
        ETeleportType::TeleportPhysics);
    if (OriginalAttachParent.IsValid())
    {
        Actor->AttachToComponent(
            OriginalAttachParent.Get(),
            FAttachmentTransformRules::KeepWorldTransform,
            OriginalAttachSocket);
        RootComponent->SetRelativeTransform(
            OriginalRelativeTransform,
            false,
            nullptr,
            ETeleportType::TeleportPhysics);
    }

    LogExportMessages(Messages);
    AppendExportMessages(OutCombinedMessages, Messages, Asset.Name);
    if (!bExported)
    {
        UE_LOG(
            LogZevyLevelExporter,
            Error,
            TEXT("Failed to export Actor asset '%s' to %s"),
            *Asset.Name,
            *ScenePath);
        return false;
    }

    UE_LOG(
        LogZevyLevelExporter,
        Display,
        TEXT("Exported asset %s (%s) from Actor %s"),
        *Asset.Id,
        *Asset.RelativeScenePath,
        *Actor->GetPathName());
    return true;
}

bool WriteSplitManifest(
    UWorld* World,
    const FString& ManifestPath,
    const FString& LevelName,
    const FContentSummary& Summary,
    const TArray<FSplitAssetRecord>& Assets,
    const TArray<AActor*>& RelevantActors,
    const TMap<const AActor*, FString>& ActorIds,
    const TMap<const AActor*, FString>& ActorAssetIds,
    const FGLTFExportMessages& Messages)
{
    TSharedRef<FJsonObject> Root = MakeShared<FJsonObject>();
    Root->SetNumberField(TEXT("schema_version"), SplitManifestSchemaVersion);
    Root->SetStringField(TEXT("level_name"), LevelName);

    TSharedRef<FJsonObject> Source = MakeShared<FJsonObject>();
    Source->SetStringField(TEXT("unreal_engine_version"), FEngineVersion::Current().ToString());
    Source->SetStringField(TEXT("project_name"), FApp::GetProjectName());
    Source->SetStringField(TEXT("map_package"), World->GetOutermost()->GetName());
    Source->SetStringField(TEXT("exported_at_utc"), FDateTime::UtcNow().ToIso8601());
    Root->SetObjectField(TEXT("source"), Source);

    TSharedRef<FJsonObject> Content = MakeShared<FJsonObject>();
    Content->SetNumberField(TEXT("static_mesh_actors"), Summary.StaticMeshActors);
    Content->SetNumberField(TEXT("static_mesh_components"), Summary.StaticMeshComponents);
    Content->SetNumberField(TEXT("unique_materials"), Summary.UniqueMaterials);
    Content->SetNumberField(TEXT("directional_lights"), Summary.DirectionalLights);
    Content->SetNumberField(TEXT("point_lights"), Summary.PointLights);
    Content->SetNumberField(TEXT("spot_lights"), Summary.SpotLights);
    Content->SetNumberField(TEXT("unsupported_lights"), Summary.UnsupportedLights);
    Content->SetNumberField(TEXT("exported_assets"), Assets.Num());
    Content->SetNumberField(TEXT("exported_entities"), RelevantActors.Num());
    Root->SetObjectField(TEXT("content"), Content);

    TSharedRef<FJsonObject> Export = MakeShared<FJsonObject>();
    Export->SetStringField(TEXT("format"), TEXT("gltf-per-asset"));
    Export->SetStringField(TEXT("asset_layout"), TEXT("assets/<asset-name>/<asset-name>.gltf"));
    Export->SetStringField(TEXT("coordinate_system"), TEXT("right-handed-y-up"));
    Export->SetNumberField(TEXT("unit_scale"), 0.01);
    Export->SetStringField(TEXT("texture_format"), TEXT("png"));
    Export->SetStringField(TEXT("material_bake_mode"), TEXT("use_mesh_data"));
    Export->SetBoolField(TEXT("normal_maps_adjusted"), true);
    Export->SetBoolField(TEXT("lights_exported"), true);
    Export->SetBoolField(TEXT("actor_hierarchy_editable"), true);
    Export->SetBoolField(TEXT("local_transforms_editable"), true);
    Root->SetObjectField(TEXT("export"), Export);

    TArray<TSharedPtr<FJsonValue>> AssetValues;
    AssetValues.Reserve(Assets.Num());
    for (const FSplitAssetRecord& Asset : Assets)
    {
        TSharedRef<FJsonObject> AssetJson = MakeShared<FJsonObject>();
        AssetJson->SetStringField(TEXT("id"), Asset.Id);
        AssetJson->SetStringField(TEXT("name"), Asset.Name);
        AssetJson->SetStringField(TEXT("scene"), Asset.RelativeScenePath);
        AssetJson->SetNumberField(TEXT("scene_index"), 0);
        if (IsValid(Asset.RepresentativeActor))
        {
            AssetJson->SetStringField(
                TEXT("source_actor"),
                Asset.RepresentativeActor->GetPathName());
            AssetJson->SetStringField(
                TEXT("source_class"),
                Asset.RepresentativeActor->GetClass()->GetPathName());
        }
        AssetValues.Add(MakeShared<FJsonValueObject>(AssetJson));
    }
    Root->SetArrayField(TEXT("assets"), AssetValues);

    TArray<TSharedPtr<FJsonValue>> EntityValues;
    EntityValues.Reserve(RelevantActors.Num());
    for (const AActor* Actor : RelevantActors)
    {
        const FString* ActorId = ActorIds.Find(Actor);
        if (ActorId == nullptr)
        {
            continue;
        }

        const AActor* ParentActor = Actor->GetAttachParentActor();
        const FString* ParentId = ParentActor != nullptr ? ActorIds.Find(ParentActor) : nullptr;
        const FTransform LocalTransform = ParentId != nullptr
            ? Actor->GetActorTransform().GetRelativeTransform(ParentActor->GetActorTransform())
            : Actor->GetActorTransform();

        TSharedRef<FJsonObject> EntityJson = MakeShared<FJsonObject>();
        EntityJson->SetStringField(TEXT("id"), *ActorId);
        EntityJson->SetStringField(TEXT("name"), GetActorDisplayName(Actor));
        if (ParentId != nullptr)
        {
            EntityJson->SetStringField(TEXT("parent"), *ParentId);
        }
        else
        {
            EntityJson->SetField(TEXT("parent"), MakeShared<FJsonValueNull>());
        }

        if (const FString* AssetId = ActorAssetIds.Find(Actor))
        {
            EntityJson->SetStringField(TEXT("asset"), *AssetId);
        }
        else
        {
            EntityJson->SetField(TEXT("asset"), MakeShared<FJsonValueNull>());
        }

        EntityJson->SetObjectField(TEXT("transform"), ToJsonTransform(LocalTransform));
        EntityJson->SetBoolField(TEXT("visible"), !Actor->IsHidden() && !Actor->IsHiddenEd());
        EntityJson->SetStringField(TEXT("source_actor"), Actor->GetPathName());
        EntityJson->SetStringField(TEXT("source_class"), Actor->GetClass()->GetPathName());

        TArray<ULightComponent*> LightComponents;
        Actor->GetComponents<ULightComponent>(LightComponents);
        TArray<TSharedPtr<FJsonValue>> LightValues;
        for (const ULightComponent* LightComponent : LightComponents)
        {
            if (!IsValid(LightComponent) || LightComponent->IsEditorOnly())
            {
                continue;
            }
            if (!LightComponent->IsA<UDirectionalLightComponent>() &&
                !LightComponent->IsA<UPointLightComponent>() &&
                !LightComponent->IsA<USpotLightComponent>())
            {
                continue;
            }
            LightValues.Add(MakeShared<FJsonValueObject>(ToJsonLightDefinition(LightComponent)));
        }
        if (!LightValues.IsEmpty())
        {
            EntityJson->SetArrayField(TEXT("lights"), LightValues);
        }

        if (const USceneComponent* RootComponent = Actor->GetRootComponent())
        {
            if (const USceneComponent* AttachParent = RootComponent->GetAttachParent())
            {
                TSharedRef<FJsonObject> Attachment = MakeShared<FJsonObject>();
                Attachment->SetStringField(TEXT("parent_component"), AttachParent->GetName());
                Attachment->SetStringField(
                    TEXT("socket"),
                    RootComponent->GetAttachSocketName().ToString());
                EntityJson->SetObjectField(TEXT("source_attachment"), Attachment);
            }
        }

        EntityValues.Add(MakeShared<FJsonValueObject>(EntityJson));
    }
    Root->SetArrayField(TEXT("entities"), EntityValues);

    TSharedRef<FJsonObject> Diagnostics = MakeShared<FJsonObject>();
    Diagnostics->SetArrayField(TEXT("suggestions"), ToJsonStringArray(Messages.Suggestions));
    Diagnostics->SetArrayField(TEXT("warnings"), ToJsonStringArray(Messages.Warnings));
    Diagnostics->SetArrayField(TEXT("errors"), ToJsonStringArray(Messages.Errors));
    Root->SetObjectField(TEXT("diagnostics"), Diagnostics);

    FString JsonText;
    const TSharedRef<TJsonWriter<TCHAR, TPrettyJsonPrintPolicy<TCHAR>>> Writer =
        TJsonWriterFactory<TCHAR, TPrettyJsonPrintPolicy<TCHAR>>::Create(&JsonText);
    if (!FJsonSerializer::Serialize(Root, Writer))
    {
        UE_LOG(LogZevyLevelExporter, Error, TEXT("Failed to serialize split Level manifest"));
        return false;
    }

    if (!FFileHelper::SaveStringToFile(
            JsonText,
            *ManifestPath,
            FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM))
    {
        UE_LOG(
            LogZevyLevelExporter,
            Error,
            TEXT("Failed to write split Zevy Level manifest: %s"),
            *ManifestPath);
        return false;
    }

    return true;
}

bool ExportWorldSplit(
    UWorld* World,
    const FString& RequestedManifestPath,
    FString& OutManifestPath)
{
    if (!IsValid(World))
    {
        UE_LOG(LogZevyLevelExporter, Error, TEXT("No valid editor World is available to export"));
        return false;
    }

    OutManifestPath = RequestedManifestPath;
    if (!OutManifestPath.EndsWith(TEXT(".zevy-level.json"), ESearchCase::IgnoreCase))
    {
        OutManifestPath += TEXT(".zevy-level.json");
    }
    OutManifestPath = FPaths::ConvertRelativePathToFull(OutManifestPath);

    const FString ExportDirectory = FPaths::GetPath(OutManifestPath);
    if (!IFileManager::Get().MakeDirectory(*FPaths::Combine(ExportDirectory, TEXT("assets")), true))
    {
        UE_LOG(
            LogZevyLevelExporter,
            Error,
            TEXT("Failed to create split export directory: %s"),
            *ExportDirectory);
        return false;
    }

    FString LevelName = FPaths::GetCleanFilename(OutManifestPath);
    LevelName.RemoveFromEnd(TEXT(".zevy-level.json"), ESearchCase::IgnoreCase);
    if (LevelName.IsEmpty())
    {
        LevelName = GetLevelName(World);
    }

    TArray<AActor*> ExportableActors;
    for (TActorIterator<AActor> ActorIterator(World); ActorIterator; ++ActorIterator)
    {
        AActor* Actor = *ActorIterator;
        if (HasExportableContent(Actor))
        {
            ExportableActors.Add(Actor);
        }
    }
    ExportableActors.Sort([](const AActor& Left, const AActor& Right)
    {
        return Left.GetPathName() < Right.GetPathName();
    });

    TSet<AActor*> RelevantActorSet;
    for (AActor* Actor : ExportableActors)
    {
        RelevantActorSet.Add(Actor);
        for (AActor* Parent = Actor->GetAttachParentActor();
             IsMainLevelActor(Parent);
             Parent = Parent->GetAttachParentActor())
        {
            RelevantActorSet.Add(Parent);
        }
    }

    TArray<AActor*> RelevantActors = RelevantActorSet.Array();
    RelevantActors.Sort([](const AActor& Left, const AActor& Right)
    {
        return Left.GetPathName() < Right.GetPathName();
    });

    TMap<const AActor*, FString> ActorIds;
    TSet<FString> UsedActorIds;
    for (const AActor* Actor : RelevantActors)
    {
        FString ActorId = GetStableActorId(Actor);
        if (UsedActorIds.Contains(ActorId))
        {
            ActorId += TEXT("_") + FMD5::HashAnsiString(*Actor->GetPathName()).Left(8);
        }
        UsedActorIds.Add(ActorId);
        ActorIds.Add(Actor, MoveTemp(ActorId));
    }

    TArray<FSplitAssetRecord> Assets;
    TMap<FString, int32> SignatureToAssetIndex;
    TMap<const AActor*, FString> ActorAssetIds;
    for (AActor* Actor : ExportableActors)
    {
        const FString Signature = BuildActorContentSignature(Actor);
        if (const int32* ExistingIndex = SignatureToAssetIndex.Find(Signature))
        {
            ActorAssetIds.Add(Actor, Assets[*ExistingIndex].Id);
            continue;
        }

        const FString Hash = FMD5::HashAnsiString(*Signature).ToLower();
        FString SafeName = FPaths::MakeValidFileName(GetActorDisplayName(Actor));
        if (SafeName.IsEmpty())
        {
            SafeName = TEXT("Asset");
        }
        SafeName = SafeName.Left(48);

        FSplitAssetRecord Asset;
        Asset.Id = TEXT("asset_") + Hash;
        Asset.Name = GetActorDisplayName(Actor);
        Asset.FolderName = SafeName + TEXT("_") + Hash.Left(8);
        Asset.RelativeScenePath = FString::Printf(
            TEXT("assets/%s/%s.gltf"),
            *Asset.FolderName,
            *Asset.FolderName);
        Asset.RepresentativeActor = Actor;

        const int32 AssetIndex = Assets.Add(MoveTemp(Asset));
        SignatureToAssetIndex.Add(Signature, AssetIndex);
        ActorAssetIds.Add(Actor, Assets[AssetIndex].Id);
    }

    UE_LOG(
        LogZevyLevelExporter,
        Display,
        TEXT("Split export '%s': %d exportable Actors, %d hierarchy entities, %d unique assets"),
        *LevelName,
        ExportableActors.Num(),
        RelevantActors.Num(),
        Assets.Num());

    TStrongObjectPtr<UGLTFExportOptions> Options(NewObject<UGLTFExportOptions>());
    ConfigureExportOptions(Options.Get());

    FGLTFExportMessages CombinedMessages;
    for (const FSplitAssetRecord& Asset : Assets)
    {
        if (!ExportActorAsset(
                World,
                Asset,
                ExportDirectory,
                Options.Get(),
                CombinedMessages))
        {
            return false;
        }
    }

    const FContentSummary Summary = CollectContentSummary(World);
    if (!WriteSplitManifest(
            World,
            OutManifestPath,
            LevelName,
            Summary,
            Assets,
            RelevantActors,
            ActorIds,
            ActorAssetIds,
            CombinedMessages))
    {
        return false;
    }

    UE_LOG(
        LogZevyLevelExporter,
        Display,
        TEXT("Exported editable Zevy Level '%s': manifest=%s assets=%d entities=%d"),
        *LevelName,
        *OutManifestPath,
        Assets.Num(),
        RelevantActors.Num());
    return true;
}

void ShowNotification(const FString& Message, SNotificationItem::ECompletionState State)
{
    FNotificationInfo Info(FText::FromString(Message));
    Info.bFireAndForget = true;
    Info.ExpireDuration = 8.0f;

    if (const TSharedPtr<SNotificationItem> Notification =
            FSlateNotificationManager::Get().AddNotification(Info))
    {
        Notification->SetCompletionState(State);
    }
}
} // namespace ZevyLevelExporter

void FZevyLevelExporterModule::StartupModule()
{
    UToolMenus::RegisterStartupCallback(
        FSimpleMulticastDelegate::FDelegate::CreateRaw(
            this,
            &FZevyLevelExporterModule::RegisterMenus));
}

void FZevyLevelExporterModule::ShutdownModule()
{
    UToolMenus::UnRegisterStartupCallback(this);
    UToolMenus::UnregisterOwner(this);
}

void FZevyLevelExporterModule::RegisterMenus()
{
    FToolMenuOwnerScoped OwnerScoped(this);

    if (UToolMenu* Menu = UToolMenus::Get()->ExtendMenu(TEXT("LevelEditor.MainMenu.Tools")))
    {
        FToolMenuSection& Section = Menu->AddSection(
            TEXT("ZevyLevelExporter"),
            LOCTEXT("ZevyLevelExporterSection", "Zevy"));
        Section.AddMenuEntry(
            TEXT("ZevyExportCurrentLevel"),
            LOCTEXT("ExportCurrentLevel", "Export Current Level to Zevy..."),
            LOCTEXT(
                "ExportCurrentLevelTooltip",
                "Export Static Meshes, PBR materials/textures, attachment hierarchy, transforms, and supported lights to a Zevy Level."),
            FSlateIcon(),
            FUIAction(FExecuteAction::CreateRaw(
                this,
                &FZevyLevelExporterModule::ExportCurrentLevel)));
    }
}

void FZevyLevelExporterModule::ExportCurrentLevel()
{
    UWorld* World = GEditor != nullptr ? GEditor->GetEditorWorldContext().World() : nullptr;
    if (!IsValid(World))
    {
        FMessageDialog::Open(
            EAppMsgType::Ok,
            LOCTEXT("NoWorld", "No valid editor Level is open."));
        return;
    }

    IDesktopPlatform* DesktopPlatform = FDesktopPlatformModule::Get();
    if (DesktopPlatform == nullptr)
    {
        FMessageDialog::Open(
            EAppMsgType::Ok,
            LOCTEXT("NoDesktopPlatform", "The desktop file dialog service is unavailable."));
        return;
    }

    const FString LevelName = ZevyLevelExporter::GetLevelName(World);
    TArray<FString> OutputFiles;
    const bool bSelected = DesktopPlatform->SaveFileDialog(
        FSlateApplication::Get().FindBestParentWindowHandleForDialogs(nullptr),
        LOCTEXT("SaveDialogTitle", "Export Current Level to Zevy").ToString(),
        ZevyLevelExporter::GetDefaultExportDirectory(World),
        LevelName + TEXT(".zevy-level.json"),
        TEXT("Editable Zevy Level (*.zevy-level.json)|*.zevy-level.json"),
        EFileDialogFlags::None,
        OutputFiles);

    if (!bSelected || OutputFiles.IsEmpty())
    {
        return;
    }

    FString ManifestPath;
    if (ZevyLevelExporter::ExportWorldSplit(World, OutputFiles[0], ManifestPath))
    {
        ZevyLevelExporter::ShowNotification(
            FString::Printf(
                TEXT("Zevy Level exported successfully: %s"),
                *ManifestPath),
            SNotificationItem::CS_Success);
    }
    else
    {
        ZevyLevelExporter::ShowNotification(
            TEXT("Zevy Level export failed. See Output Log for details."),
            SNotificationItem::CS_Fail);
    }
}

IMPLEMENT_MODULE(FZevyLevelExporterModule, ZevyLevelExporter)

#undef LOCTEXT_NAMESPACE
