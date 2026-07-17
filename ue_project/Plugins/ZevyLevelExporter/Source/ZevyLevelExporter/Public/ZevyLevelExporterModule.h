#pragma once

#include "Modules/ModuleManager.h"

class UWorld;

namespace ZevyLevelExporter
{
struct FTextureMipExportOptions
{
    bool bGenerateMipmaps = true;
    bool bDebugMipNumbers = false;
};

bool ExportWorld(UWorld* World, const FString& RequestedGlbPath, FString& OutManifestPath);
bool ExportWorldSplit(UWorld* World, const FString& RequestedManifestPath, FString& OutManifestPath);
bool ExportWorldSplit(
    UWorld* World,
    const FString& RequestedManifestPath,
    FString& OutManifestPath,
    const FTextureMipExportOptions& TextureMipOptions);
}

class FZevyLevelExporterModule final : public IModuleInterface
{
public:
    virtual void StartupModule() override;
    virtual void ShutdownModule() override;

private:
    void RegisterMenus();
    void ExportCurrentLevel();
};
