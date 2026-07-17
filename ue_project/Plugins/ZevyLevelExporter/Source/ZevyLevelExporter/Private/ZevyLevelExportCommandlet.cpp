#include "ZevyLevelExportCommandlet.h"

#include "Components/DirectionalLightComponent.h"
#include "Components/PointLightComponent.h"
#include "Components/SpotLightComponent.h"
#include "Engine/DirectionalLight.h"
#include "Engine/PointLight.h"
#include "Engine/SpotLight.h"
#include "Engine/StaticMesh.h"
#include "Engine/StaticMeshActor.h"
#include "FileHelpers.h"
#include "Materials/MaterialInterface.h"
#include "Misc/Parse.h"
#include "ZevyLevelExporterModule.h"

namespace
{
bool GenerateValidationFixture(UWorld* World)
{
    UStaticMesh* CubeMesh = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Cube.Cube"));
    UStaticMesh* SphereMesh = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Sphere.Sphere"));
    UStaticMesh* CylinderMesh = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Cylinder.Cylinder"));
    UMaterialInterface* GridMaterial = LoadObject<UMaterialInterface>(
        nullptr,
        TEXT("/Engine/EngineMaterials/WorldGridMaterial.WorldGridMaterial"));
    UMaterialInterface* BasicShapeMaterial = LoadObject<UMaterialInterface>(
        nullptr,
        TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));

    if (CubeMesh == nullptr || SphereMesh == nullptr || CylinderMesh == nullptr)
    {
        UE_LOG(LogTemp, Error, TEXT("Failed to load one or more Engine basic-shape meshes"));
        return false;
    }

    AStaticMeshActor* Parent = World->SpawnActor<AStaticMeshActor>();
    AStaticMeshActor* Child = World->SpawnActor<AStaticMeshActor>();
    AStaticMeshActor* Grandchild = World->SpawnActor<AStaticMeshActor>();
    if (Parent == nullptr || Child == nullptr || Grandchild == nullptr)
    {
        UE_LOG(LogTemp, Error, TEXT("Failed to spawn Static Mesh validation fixture"));
        return false;
    }

    Parent->SetActorLabel(TEXT("ZevyFixtureParentCube"));
    Parent->GetStaticMeshComponent()->SetMobility(EComponentMobility::Movable);
    Parent->GetStaticMeshComponent()->SetStaticMesh(CubeMesh);
    Parent->GetStaticMeshComponent()->SetMaterial(0, GridMaterial);
    Parent->SetActorLocation(FVector(100.0, -75.0, 100.0));
    Parent->SetActorRotation(FRotator(10.0, 25.0, -5.0));
    Parent->SetActorScale3D(FVector(1.5, 0.75, 0.5));

    Child->SetActorLabel(TEXT("ZevyFixtureChildSphere"));
    Child->GetStaticMeshComponent()->SetMobility(EComponentMobility::Movable);
    Child->GetStaticMeshComponent()->SetStaticMesh(SphereMesh);
    Child->GetStaticMeshComponent()->SetMaterial(0, BasicShapeMaterial);
    Child->AttachToActor(Parent, FAttachmentTransformRules::KeepRelativeTransform);
    Child->SetActorRelativeLocation(FVector(125.0, 20.0, 140.0));
    Child->SetActorRelativeRotation(FRotator(15.0, 40.0, 20.0));
    Child->SetActorRelativeScale3D(FVector(0.5, 1.25, 0.75));

    Grandchild->SetActorLabel(TEXT("ZevyFixtureGrandchildCylinder"));
    Grandchild->GetStaticMeshComponent()->SetMobility(EComponentMobility::Movable);
    Grandchild->GetStaticMeshComponent()->SetStaticMesh(CylinderMesh);
    Grandchild->GetStaticMeshComponent()->SetMaterial(0, GridMaterial);
    Grandchild->AttachToActor(Child, FAttachmentTransformRules::KeepRelativeTransform);
    Grandchild->SetActorRelativeLocation(FVector(-60.0, 80.0, 75.0));
    Grandchild->SetActorRelativeRotation(FRotator(-30.0, 5.0, 55.0));
    Grandchild->SetActorRelativeScale3D(FVector(0.35, 0.6, 1.4));

    ADirectionalLight* DirectionalLight = World->SpawnActor<ADirectionalLight>();
    APointLight* PointLight = World->SpawnActor<APointLight>();
    ASpotLight* SpotLight = World->SpawnActor<ASpotLight>();
    if (DirectionalLight == nullptr || PointLight == nullptr || SpotLight == nullptr)
    {
        UE_LOG(LogTemp, Error, TEXT("Failed to spawn light validation fixture"));
        return false;
    }

    DirectionalLight->SetActorLabel(TEXT("ZevyFixtureDirectionalLight"));
    DirectionalLight->SetActorRotation(FRotator(-45.0, 30.0, 0.0));
    UDirectionalLightComponent* DirectionalComponent =
        CastChecked<UDirectionalLightComponent>(DirectionalLight->GetLightComponent());
    DirectionalComponent->SetMobility(EComponentMobility::Movable);
    DirectionalComponent->SetIntensity(5.0f);
    DirectionalComponent->SetLightColor(FLinearColor(1.0f, 0.88f, 0.72f));

    PointLight->SetActorLabel(TEXT("ZevyFixtureAttachedPointLight"));
    PointLight->AttachToActor(Parent, FAttachmentTransformRules::KeepRelativeTransform);
    PointLight->SetActorRelativeLocation(FVector(0.0, -180.0, 220.0));
    UPointLightComponent* PointComponent =
        CastChecked<UPointLightComponent>(PointLight->GetLightComponent());
    PointComponent->SetMobility(EComponentMobility::Movable);
    PointComponent->SetIntensity(2500.0f);
    PointComponent->SetAttenuationRadius(700.0f);
    PointComponent->SetLightColor(FLinearColor(0.3f, 0.55f, 1.0f));

    SpotLight->SetActorLabel(TEXT("ZevyFixtureSpotLight"));
    SpotLight->SetActorLocation(FVector(-250.0, 150.0, 400.0));
    SpotLight->SetActorRotation(FRotator(-55.0, -20.0, 0.0));
    USpotLightComponent* SpotComponent =
        CastChecked<USpotLightComponent>(SpotLight->GetLightComponent());
    SpotComponent->SetMobility(EComponentMobility::Movable);
    SpotComponent->SetIntensity(4000.0f);
    SpotComponent->SetAttenuationRadius(900.0f);
    SpotComponent->SetInnerConeAngle(18.0f);
    SpotComponent->SetOuterConeAngle(34.0f);
    SpotComponent->SetLightColor(FLinearColor(1.0f, 0.25f, 0.12f));

    return true;
}
} // namespace

UZevyLevelExportCommandlet::UZevyLevelExportCommandlet()
{
    IsClient = false;
    IsServer = false;
    IsEditor = true;
    LogToConsole = true;
    ShowErrorCount = true;
}

int32 UZevyLevelExportCommandlet::Main(const FString& Params)
{
    FString MapPath;
    if (!FParse::Value(*Params, TEXT("Map="), MapPath) || MapPath.IsEmpty())
    {
        UE_LOG(
            LogTemp,
            Error,
            TEXT("Missing -Map=<map package or .umap filename> for ZevyLevelExport commandlet"));
        return 1;
    }

    FString OutputPath;
    if (!FParse::Value(*Params, TEXT("Output="), OutputPath) || OutputPath.IsEmpty())
    {
        UE_LOG(
            LogTemp,
            Error,
            TEXT("Missing -Output=<absolute .zevy-level.json or .glb filename> for ZevyLevelExport commandlet"));
        return 1;
    }

    UWorld* World = UEditorLoadingAndSavingUtils::LoadMap(MapPath);
    if (World == nullptr)
    {
        UE_LOG(LogTemp, Error, TEXT("Failed to load map for export: %s"), *MapPath);
        return 1;
    }

    if (FParse::Param(*Params, TEXT("GenerateFixture")) && !GenerateValidationFixture(World))
    {
        return 1;
    }

    FString ManifestPath;
    const bool bMonolithicGlb = OutputPath.EndsWith(TEXT(".glb"), ESearchCase::IgnoreCase);
    ZevyLevelExporter::FTextureMipExportOptions TextureMipOptions;
    TextureMipOptions.bDebugMipNumbers = FParse::Param(*Params, TEXT("DebugMipNumbers"));
    TextureMipOptions.bGenerateMipmaps =
        TextureMipOptions.bDebugMipNumbers || !FParse::Param(*Params, TEXT("NoTextureMipmaps"));
    const bool bExported = bMonolithicGlb
        ? ZevyLevelExporter::ExportWorld(World, OutputPath, ManifestPath)
        : ZevyLevelExporter::ExportWorldSplit(
              World,
              OutputPath,
              ManifestPath,
              TextureMipOptions);
    if (!bExported)
    {
        return 1;
    }

    UE_LOG(LogTemp, Display, TEXT("Zevy Level manifest: %s"), *ManifestPath);
    return 0;
}
