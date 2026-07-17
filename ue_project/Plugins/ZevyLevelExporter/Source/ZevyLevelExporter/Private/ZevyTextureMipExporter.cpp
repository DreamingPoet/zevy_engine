#include "ZevyTextureMipExporter.h"

#include "HAL/FileManager.h"
#include "ImageCore.h"
#include "ImageUtils.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "ZevyLevelExporterModule.h"

DEFINE_LOG_CATEGORY_STATIC(LogZevyTextureMipExporter, Log, All);

namespace ZevyLevelExporter
{
namespace
{
constexpr uint32 ZevyMipVersion = 1;
constexpr uint32 ZevyMipFlagDebugNumbers = 1;

const uint8 DigitRows[10][7] = {
    {0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E}, // 0
    {0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E}, // 1
    {0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F}, // 2
    {0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E}, // 3
    {0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02}, // 4
    {0x1F, 0x10, 0x10, 0x1E, 0x01, 0x01, 0x1E}, // 5
    {0x0E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x0E}, // 6
    {0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08}, // 7
    {0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E}, // 8
    {0x0E, 0x11, 0x11, 0x0F, 0x01, 0x01, 0x0E}, // 9
};

const FColor DebugPalette[] = {
    FColor(235, 64, 52),
    FColor(255, 159, 26),
    FColor(250, 204, 21),
    FColor(34, 197, 94),
    FColor(20, 184, 166),
    FColor(14, 165, 233),
    FColor(59, 130, 246),
    FColor(139, 92, 246),
    FColor(217, 70, 239),
    FColor(236, 72, 153),
};

void AppendU32LittleEndian(TArray64<uint8>& Bytes, uint32 Value)
{
    Bytes.Add(static_cast<uint8>(Value & 0xFF));
    Bytes.Add(static_cast<uint8>((Value >> 8) & 0xFF));
    Bytes.Add(static_cast<uint8>((Value >> 16) & 0xFF));
    Bytes.Add(static_cast<uint8>((Value >> 24) & 0xFF));
}

uint8 BlendChannel(uint8 Source, uint8 Overlay, uint8 Alpha)
{
    return static_cast<uint8>(
        (static_cast<uint32>(Source) * (255 - Alpha) +
         static_cast<uint32>(Overlay) * Alpha + 127) /
        255);
}

void BlendPixel(FImage& Image, int32 X, int32 Y, const FColor& Color, uint8 Alpha)
{
    if (X < 0 || Y < 0 || X >= Image.SizeX || Y >= Image.SizeY)
    {
        return;
    }

    FColor* Pixels = reinterpret_cast<FColor*>(Image.RawData.GetData());
    FColor& Pixel = Pixels[static_cast<int64>(Y) * Image.SizeX + X];
    Pixel.R = BlendChannel(Pixel.R, Color.R, Alpha);
    Pixel.G = BlendChannel(Pixel.G, Color.G, Alpha);
    Pixel.B = BlendChannel(Pixel.B, Color.B, Alpha);
}

void FillRect(
    FImage& Image,
    int32 MinX,
    int32 MinY,
    int32 MaxX,
    int32 MaxY,
    const FColor& Color,
    uint8 Alpha)
{
    MinX = FMath::Clamp(MinX, 0, Image.SizeX);
    MinY = FMath::Clamp(MinY, 0, Image.SizeY);
    MaxX = FMath::Clamp(MaxX, 0, Image.SizeX);
    MaxY = FMath::Clamp(MaxY, 0, Image.SizeY);
    for (int32 Y = MinY; Y < MaxY; ++Y)
    {
        for (int32 X = MinX; X < MaxX; ++X)
        {
            BlendPixel(Image, X, Y, Color, Alpha);
        }
    }
}

void DrawDigit(
    FImage& Image,
    int32 Digit,
    int32 OriginX,
    int32 OriginY,
    int32 Scale,
    const FColor& Color)
{
    check(Digit >= 0 && Digit <= 9);
    for (int32 Row = 0; Row < 7; ++Row)
    {
        for (int32 Column = 0; Column < 5; ++Column)
        {
            if ((DigitRows[Digit][Row] & (1 << (4 - Column))) == 0)
            {
                continue;
            }
            FillRect(
                Image,
                OriginX + Column * Scale,
                OriginY + Row * Scale,
                OriginX + (Column + 1) * Scale,
                OriginY + (Row + 1) * Scale,
                Color,
                255);
        }
    }
}

void OverlayDebugMipNumber(FImage& Image, int32 MipLevel)
{
    check(Image.Format == ERawImageFormat::BGRA8);
    const FColor Background = DebugPalette[MipLevel % UE_ARRAY_COUNT(DebugPalette)];
    const FString Label = FString::FromInt(MipLevel);

    const int32 MinDimension = FMath::Min(Image.SizeX, Image.SizeY);
    if (MinDimension < 9)
    {
        FillRect(Image, 0, 0, Image.SizeX, Image.SizeY, Background, 190);
        return;
    }

    const int32 Scale = FMath::Clamp(MinDimension / 192, 1, 10);
    const int32 LabelWidth = Label.Len() * 6 * Scale - Scale;
    const int32 LabelHeight = 7 * Scale;
    const int32 Padding = 3 * Scale;
    const int32 CellWidth = FMath::Max(LabelWidth + Padding * 2, 24 * Scale);
    const int32 CellHeight = FMath::Max(LabelHeight + Padding * 2, 18 * Scale);

    for (int32 CellY = 0; CellY < Image.SizeY; CellY += CellHeight)
    {
        for (int32 CellX = 0; CellX < Image.SizeX; CellX += CellWidth)
        {
            FillRect(
                Image,
                CellX,
                CellY,
                FMath::Min(CellX + CellWidth, Image.SizeX),
                FMath::Min(CellY + CellHeight, Image.SizeY),
                Background,
                105);

            const int32 TextX = CellX + FMath::Max((CellWidth - LabelWidth) / 2, 0);
            const int32 TextY = CellY + FMath::Max((CellHeight - LabelHeight) / 2, 0);
            for (int32 Index = 0; Index < Label.Len(); ++Index)
            {
                const int32 Digit = Label[Index] - TEXT('0');
                DrawDigit(
                    Image,
                    Digit,
                    TextX + Index * 6 * Scale + Scale,
                    TextY + Scale,
                    Scale,
                    FColor::Black);
                DrawDigit(
                    Image,
                    Digit,
                    TextX + Index * 6 * Scale,
                    TextY,
                    Scale,
                    FColor::White);
            }
        }
    }
}

int32 CalculateMipLevelCount(int32 Width, int32 Height)
{
    int32 LevelCount = 1;
    while (Width > 1 || Height > 1)
    {
        Width = FMath::Max(Width / 2, 1);
        Height = FMath::Max(Height / 2, 1);
        ++LevelCount;
    }
    return LevelCount;
}

bool WriteTextureMipSidecar(const FString& PngPath, bool bDebugMipNumbers)
{
    FImage SourceImage;
    if (!FImageUtils::LoadImage(*PngPath, SourceImage) || SourceImage.SizeX <= 0 || SourceImage.SizeY <= 0)
    {
        UE_LOG(LogZevyTextureMipExporter, Error, TEXT("Failed to load exported PNG: %s"), *PngPath);
        return false;
    }

    FImage CurrentImage;
    SourceImage.CopyTo(CurrentImage, ERawImageFormat::BGRA8, SourceImage.GammaSpace);
    const int32 MipLevelCount = CalculateMipLevelCount(CurrentImage.SizeX, CurrentImage.SizeY);

    TArray64<uint8> SidecarBytes;
    static const uint8 Magic[8] = {'Z', 'E', 'V', 'Y', 'M', 'I', 'P', 0};
    SidecarBytes.Append(Magic, UE_ARRAY_COUNT(Magic));
    AppendU32LittleEndian(SidecarBytes, ZevyMipVersion);
    AppendU32LittleEndian(SidecarBytes, bDebugMipNumbers ? ZevyMipFlagDebugNumbers : 0);
    AppendU32LittleEndian(SidecarBytes, static_cast<uint32>(CurrentImage.SizeX));
    AppendU32LittleEndian(SidecarBytes, static_cast<uint32>(CurrentImage.SizeY));
    AppendU32LittleEndian(SidecarBytes, static_cast<uint32>(MipLevelCount));
    AppendU32LittleEndian(SidecarBytes, 0);

    for (int32 MipLevel = 0; MipLevel < MipLevelCount; ++MipLevel)
    {
        FImage OutputImage;
        CurrentImage.CopyTo(OutputImage);
        if (bDebugMipNumbers)
        {
            OverlayDebugMipNumber(OutputImage, MipLevel);
        }

        TArray64<uint8> CompressedPng;
        if (!FImageUtils::CompressImage(CompressedPng, TEXT(".png"), OutputImage, 0) ||
            CompressedPng.Num() > MAX_uint32)
        {
            UE_LOG(
                LogZevyTextureMipExporter,
                Error,
                TEXT("Failed to encode mip %d for %s"),
                MipLevel,
                *PngPath);
            return false;
        }

        AppendU32LittleEndian(SidecarBytes, static_cast<uint32>(OutputImage.SizeX));
        AppendU32LittleEndian(SidecarBytes, static_cast<uint32>(OutputImage.SizeY));
        AppendU32LittleEndian(SidecarBytes, static_cast<uint32>(CompressedPng.Num()));
        SidecarBytes.Append(CompressedPng);

        if (MipLevel + 1 < MipLevelCount)
        {
            FImage NextImage;
            FImageCore::ResizeImageAllocDest(
                CurrentImage,
                NextImage,
                FMath::Max(CurrentImage.SizeX / 2, 1),
                FMath::Max(CurrentImage.SizeY / 2, 1),
                FImageCore::EResizeImageFilter::Box);
            CurrentImage.Swap(NextImage);
        }
    }

    const FString SidecarPath = FPaths::ChangeExtension(PngPath, TEXT("zevy-mips"));
    if (!FFileHelper::SaveArrayToFile(SidecarBytes, *SidecarPath))
    {
        UE_LOG(LogZevyTextureMipExporter, Error, TEXT("Failed to save mip sidecar: %s"), *SidecarPath);
        return false;
    }

    UE_LOG(
        LogZevyTextureMipExporter,
        Display,
        TEXT("Exported %d mip levels for %s%s"),
        MipLevelCount,
        *PngPath,
        bDebugMipNumbers ? TEXT(" with debug numbers") : TEXT(""));
    return true;
}

void DeleteExistingMipSidecars(const FString& ExportDirectory)
{
    TArray<FString> SidecarPaths;
    IFileManager::Get().FindFilesRecursive(
        SidecarPaths,
        *FPaths::Combine(ExportDirectory, TEXT("assets")),
        TEXT("*.zevy-mips"),
        true,
        false,
        false);
    for (const FString& SidecarPath : SidecarPaths)
    {
        IFileManager::Get().Delete(*SidecarPath, false, true, true);
    }
}
} // namespace

bool GenerateTextureMipSidecars(
    const FString& ExportDirectory,
    const FTextureMipExportOptions& Options,
    int32& OutTextureCount)
{
    OutTextureCount = 0;
    if (!Options.bGenerateMipmaps)
    {
        DeleteExistingMipSidecars(ExportDirectory);
        UE_LOG(LogZevyTextureMipExporter, Display, TEXT("Texture mipmap export is disabled"));
        return true;
    }

    TArray<FString> PngPaths;
    IFileManager::Get().FindFilesRecursive(
        PngPaths,
        *FPaths::Combine(ExportDirectory, TEXT("assets")),
        TEXT("*.png"),
        true,
        false,
        false);
    PngPaths.Sort();

    for (const FString& PngPath : PngPaths)
    {
        if (!WriteTextureMipSidecar(PngPath, Options.bDebugMipNumbers))
        {
            return false;
        }
        ++OutTextureCount;
    }

    UE_LOG(
        LogZevyTextureMipExporter,
        Display,
        TEXT("Exported mip sidecars for %d PNG textures (debug numbers: %s)"),
        OutTextureCount,
        Options.bDebugMipNumbers ? TEXT("true") : TEXT("false"));
    return true;
}
} // namespace ZevyLevelExporter
