using UnrealBuildTool;

public class ZevyLevelExporter : ModuleRules
{
    public ZevyLevelExporter(ReadOnlyTargetRules Target) : base(Target)
    {
        PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;

        PrivateDependencyModuleNames.AddRange(
            new string[]
            {
                "Core",
                "CoreUObject",
                "Engine",
                "UnrealEd",
                "Slate",
                "SlateCore",
                "ToolMenus",
                "DesktopPlatform",
                "ImageCore",
                "Json",
                "GLTFExporter"
            }
        );
    }
}
