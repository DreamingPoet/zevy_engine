# zevy_engine

`zevy_engine` is a custom VR rendering engine project built with Rust, Bevy, OpenXR, and Vulkan.

The goal of this project is to support the production of real-time, first-person, interactive VR experiences with modern rendering features such as:

- PBR materials
- Multiple light sources
- OpenXR support
- Vulkan-based rendering
- Support platforms: Android XR device (PICO 4 Ultra), Windows (editor, debug)

## Development Environment

This project uses the stable Rust toolchain.

Set the local toolchain to stable:

```powershell
rustup override set stable
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

The project keeps platform, XR, and scene code separated:

- `src/lib.rs`: crate/native entry point.
- `src/app.rs`: launch mode selection, plugin assembly, global app startup.
- `src/platform.rs`: Android NativeActivity bridge, Android lifecycle polling, display refresh-rate setup.
- `src/xr.rs`: OpenXR plugin setup, XR actions, locomotion, hand/controller anchor visuals, XR logs.
- `src/scene.rs`: Level management, default Level, `OpenLevel` event.
- `src/scene/levels.rs`: current prototype Level content.
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
