#pragma once

#include "CoreMinimal.h"

namespace ZevyLevelExporter
{
struct FTextureMipExportOptions;

bool GenerateTextureMipSidecars(
    const FString& ExportDirectory,
    const FTextureMipExportOptions& Options,
    int32& OutTextureCount);
}
