#pragma once

#include "Modules/ModuleManager.h"

class UWorld;

namespace ZevyLevelExporter
{
bool ExportWorld(UWorld* World, const FString& RequestedGlbPath, FString& OutManifestPath);
bool ExportWorldSplit(UWorld* World, const FString& RequestedManifestPath, FString& OutManifestPath);
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
