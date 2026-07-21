# Zevy Level Exporter

UE 5.5 Editor plugin for exporting the current Unreal Level to zevy_engine.

## Editor workflow

Open a Level and choose:

    Tools > Zevy > Export Current Level to Zevy...

The default output location in this workspace is:

    ../zevy_engine/assets/levels/<LevelName>/

Each export produces:

    <LevelName>.glb
    <LevelName>.zevy-level.json

The GLB contains Static Mesh geometry, material slots and overrides, baked PBR
textures, Actor/Component attachment hierarchy, local transforms, and
Directional/Point/Spot lights. The JSON file records schema version, source
map, content counts, export settings, and diagnostics.

## Commandlet workflow

The plugin also provides a non-interactive commandlet for CI and repeatable
exports:

```powershell
& "F:\Program Files\Epic Games\UE_5.5\Engine\Binaries\Win64\UnrealEditor-Cmd.exe" `
  "G:\zevy_engine\ue_project\zevy_ue.uproject" `
  -run=ZevyLevelExport `
  -Map="/Game/Level_ZevyDemo" `
  -Output="G:\zevy_engine\zevy_engine\assets\levels\Level_ZevyDemo\Level_ZevyDemo.glb" `
  -unattended -nop4 -nosplash -NoSound -AllowCommandletRendering -DDC-ForceMemoryCache
```

Add `-GenerateFixture` only for exporter validation. It temporarily spawns
three attached Engine basic-shape meshes, two materials with baked textures,
and Directional/Point/Spot lights without saving them back into the source Map.

## Current limitations

- Complex UE-only shaders are baked or degraded to glTF PBR.
- Non-uniform parent scale combined with child rotation can differ after TRS
  decomposition; the exporter records a warning in the manifest.
- Rect Light, Sky Light, IES, Light Function, advanced shadow settings,
  Stationary mixed-lighting semantics, Lumen, collision, Blueprint gameplay,
  Niagara, and World Partition streaming need later Zevy-specific metadata or
  runtime systems. PointLight `static` mobility is exported; Map_S03B applies
  its level calibration once, then prevents candle visuals, animation and
  periodic projection invalidation for that light.
