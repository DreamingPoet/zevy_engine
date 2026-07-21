# zevy_engine

`zevy_engine` is a custom VR rendering engine project built with Rust, Bevy, OpenXR, and Vulkan.

The goal of this project is to support the production of real-time, first-person, interactive VR experiences with modern rendering features such as:

- PBR materials
- Multiple light sources
- OpenXR support
- Vulkan-based rendering
- Support platforms: Android XR device (PICO 4 Ultra), Windows (editor, debug)

## Development Environment

The reproducible baseline is Rust 1.95.0, Cargo 1.95.0, and cargo-apk 0.10.0.
The complete clean-machine setup, Android SDK/NDK/JDK matrix, verification
commands, signing steps, and troubleshooting guide are documented in
[`Docs/Rust_Android_Environment_Setup.md`](../Docs/Rust_Android_Environment_Setup.md).

Set the repository-local toolchain to the exact validated version:

```powershell
rustup override set 1.95.0-x86_64-pc-windows-msvc
```

Check the installed Cargo version:

```powershell
cargo --version
```

Inspect the currently installed Rust toolchains and check for updates:

```powershell
rustup show
rustup check
```

Install the Android Rust target if it is not already installed:

```powershell
rustup target add aarch64-linux-android
```

## Project Structure

The project keeps platform, XR, input, and scene code separated:

- `src/lib.rs`: crate/native entry point.
- `src/app.rs`: launch mode selection, plugin assembly, global app startup.
- `src/platform.rs`: Android NativeActivity bridge, Android lifecycle polling, display refresh-rate setup.
- `src/xr.rs`: OpenXR plugin setup, hand/controller anchor visuals, mirror camera sync, XR logs.
- `src/input.rs`: keyboard, mouse, and OpenXR/PICO controller input abstraction.
- `src/scene.rs`: Level management, default Level, `OpenLevel` event.
- `src/scene/levels.rs`: current prototype Level content.
- `InputSpec.md`: input module goal, design, implementation progress, and next steps.
- `scripts/build_android_pico.ps1`: Android APK build script.
- `scripts/deploy_pico.ps1`: install and launch script for PICO 4 Ultra.

## Windows Build and Run

Check the desktop build:

```powershell
cargo check
```

Run the desktop prototype:

```powershell
cargo run
```

Request XR mode on a desktop OpenXR runtime:

```powershell
cargo run -- --xr
```

## Import an Unreal Engine Level

The UE 5.5 test project includes the `ZevyLevelExporter` Editor plugin. In
Unreal Editor, use:

    Tools > Zevy > Export Current Level to Zevy...

The plugin writes an editable schema-v2 Zevy Level manifest plus independent
glTF assets under `assets/levels/<LevelName>/` by default:

```text
<LevelName>.zevy-level.json
assets/<AssetName>_<Hash>/<AssetName>_<Hash>.gltf
assets/<AssetName>_<Hash>/<AssetName>_<Hash>.bin
assets/<AssetName>_<Hash>/*.png
```

The manifest owns Actor IDs, parent relationships, visibility, and editable
local translation/rotation/scale. Reused model/material combinations share an
asset while keeping independent Actor transforms. The loader still supports
the older schema-v1 single-GLB format.

Supported directional, point, and spot lights also keep reusable parameters in
each entity's `lights` array. Zevy applies the exported Bevy color, intensity,
range, source radius, shadow flag, and spot cone angles after glTF scene
instantiation. The public `ImportedZevyLight` component retains both those Bevy
values and the original Unreal units, temperature, attenuation mode/falloff
exponent, attenuation radius, source dimensions, mobility, and shadow biases.
Custom Unreal falloff exponents are retained as metadata and currently render
with Bevy's standard inverse-square cut-off approximation.

Load an exported Level on desktop:

```powershell
cargo run -- --level=levels/<LevelName>/<LevelName>.zevy-level.json
```

Desktop Level roaming uses an Unreal DefaultPawn/editor-style free-flight
controller. It is attached only in `--desktop` mode; XR keeps using the
OpenXR camera and tracking root.

- `W/A/S/D` or arrow keys: move relative to the view.
- Hold right mouse button: capture the cursor and look around.
- `Q` / `E`: move down / up.
- Hold either `Shift`: sprint.
- Mouse wheel: decrease / increase the base movement speed.
- `Esc`: release the captured cursor until the right mouse button is released.

The current controller intentionally uses no-clip movement because imported
collision data is not part of the Level schema yet.

Run the headless end-to-end validator against the included fixture:

```powershell
cargo run --offline --bin validate_zevy_level -- levels/ZevyExporterFixture/ZevyExporterFixture.zevy-level.json
```

Validate the exported `Map_S03B` Level:

```powershell
cargo run --offline --bin validate_zevy_level -- levels/Map_S03B/Map_S03B.zevy-level.json
```

Render an automatically framed 1600x900 preview and exit:

```powershell
cargo run --offline -- --desktop `
    --level=levels/Map_S03B/Map_S03B.zevy-level.json `
    --screenshot=assets/levels/Map_S03B/Map_S03B_preview.png
```

The validator loads all recursive glTF dependencies, spawns every composed
`SceneRoot`, and checks Actor IDs, hierarchy, editable local transforms,
meshes, materials, supported lights, and the reusable light parameter overrides.

## Android / PICO 4 Ultra Environment

The PowerShell scripts currently assume this local Android environment:

```text
Android SDK: F:\AndriodSDK\AndriodSDK
JDK:         F:\AndriodSDK\AndriodSDK\JAVA\jdk-17.0.10
NDK:         F:\AndriodSDK\AndriodSDK\ndk\25.1.8937393
ADB:         F:\AndriodSDK\AndriodSDK\platform-tools\adb.exe
```

For manual commands, set these environment variables in the current PowerShell session:

```powershell
$env:ANDROID_HOME = "F:\AndriodSDK\AndriodSDK"
$env:ANDROID_SDK_ROOT = "F:\AndriodSDK\AndriodSDK"
$env:JAVA_HOME = "F:\AndriodSDK\AndriodSDK\JAVA\jdk-17.0.10"
$env:PATH = "$env:JAVA_HOME\bin;$env:ANDROID_HOME\platform-tools;$env:ANDROID_HOME\cmdline-tools\latest\bin;$env:PATH"
```

## Android Compilation Check

Use this before a full APK build when changing Rust code:

```powershell
cargo check --target aarch64-linux-android
```

## Build Android APK

Release is the default and recommended profile for headset testing:

```powershell
.\scripts\build_android_pico.ps1
```

This is equivalent to:

```powershell
.\scripts\build_android_pico.ps1 -Profile release
```

The release helper excludes the render debug HUD by default. Build a release-optimized profiling APK with the HUD enabled using:

```powershell
.\scripts\build_android_pico.ps1 -Profile release -RenderDebug
```

The release APK is written to:

```text
target\release\apk\zevy_engine.apk
```

Build a debug APK only when you specifically need a slower diagnostic build:

```powershell
.\scripts\build_android_pico.ps1 -Profile debug
```

The debug APK is written to:

```text
target\debug\apk\zevy_engine.apk
```

Notes:

- The release profile uses optimized Rust settings from `Cargo.toml`.
- The release APK is signed with the local Android debug keystore for device iteration.
- This signing setup is for development only, not store distribution.

## Deploy to PICO 4 Ultra

Connect the PICO 4 Ultra over ADB, enable USB debugging, then verify the device:

```powershell
F:\AndriodSDK\AndriodSDK\platform-tools\adb.exe devices -l
```

Deploy and launch the release APK:

```powershell
.\scripts\deploy_pico.ps1
```

This is equivalent to:

```powershell
.\scripts\deploy_pico.ps1 -Profile release
```

Deploy and launch the debug APK:

```powershell
.\scripts\deploy_pico.ps1 -Profile debug
```

The deploy script:

- checks that `target\<profile>\apk\zevy_engine.apk` exists,
- checks that an Android device is connected over ADB,
- installs the APK with `adb install -r`,
- sets `debug.xr.graphicsPlugin Vulkan`,
- force-stops `com.zevy.engine`,
- launches `android.app.NativeActivity` with the PICO VR category.

## Common PICO Iteration Commands

Clear logcat before a run:

```powershell
F:\AndriodSDK\AndriodSDK\platform-tools\adb.exe logcat -c
```

Capture useful runtime logs after launch:

```powershell
F:\AndriodSDK\AndriodSDK\platform-tools\adb.exe logcat -d -v time |
    Select-String -Pattern "Pkg=com.zevy.engine|PXRSDK_PM ENGINE FPS|PxrMetric|FATAL|Failed|ERROR|panic"
```

Build, deploy, and inspect logs:

```powershell
.\scripts\build_android_pico.ps1 -Profile release
F:\AndriodSDK\AndriodSDK\platform-tools\adb.exe logcat -c
.\scripts\deploy_pico.ps1 -Profile release
Start-Sleep -Seconds 15
F:\AndriodSDK\AndriodSDK\platform-tools\adb.exe logcat -d -v time |
    Select-String -Pattern "Pkg=com.zevy.engine|PXRSDK_PM ENGINE FPS|PxrMetric|FATAL|Failed|ERROR|panic"
```

## Development Goals

The planned work is divided into two tracks:

1. Engine development
   - Complete the Bevy + OpenXR integration.
   - Continue expanding the rendering and VR feature set.
2. Content development
   - Build interactive demo content on top of the engine.
   - Keep content and engine/platform code separated through the Level system.
