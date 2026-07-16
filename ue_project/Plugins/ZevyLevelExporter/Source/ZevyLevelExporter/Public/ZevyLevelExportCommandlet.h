#pragma once

#include "Commandlets/Commandlet.h"
#include "ZevyLevelExportCommandlet.generated.h"

UCLASS()
class ZEVYLEVELEXPORTER_API UZevyLevelExportCommandlet final : public UCommandlet
{
    GENERATED_BODY()

public:
    UZevyLevelExportCommandlet();

    virtual int32 Main(const FString& Params) override;
};
