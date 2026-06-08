# VRHeadsetEmulator Implementation

`VRHeadsetEmulator` is an independent Windows-only SteamVR fake HMD project under `G:\zevy_engine\VRHeadsetEmulator`.

The current implementation uses:

- **C++ SteamVR driver DLL** for the HMD device, using Valve's `openvr_driver.h` C++ interfaces.
- **Rust controller app** for keyboard/mouse 6DOF pose input.
- **Windows Named Pipe IPC** between controller and driver.

The previous Rust `cdylib` driver is kept in `driver_plugin/` as an experiment, but it is no longer the packaged driver. SteamVR driver ABI is a C++ vtable ABI, so the stable path is to compile a native C++ driver with MSVC.

## Current Layout

```text
VRHeadsetEmulator/
├── controller_app/          # Rust EXE: keyboard/mouse 6DOF controller
├── hmd_protocol/            # Rust protocol tests/docs
├── driver_plugin_cpp/       # C++ SteamVR driver DLL
├── driver_plugin/           # Old Rust ABI experiment, not packaged by default
├── third_party/openvr/      # Valve openvr_driver.h
├── scripts/
├── driver.vrdrivermanifest
└── driver.vrresources
```

## C++ Driver

The C++ driver builds `driver_virtual_hmd.dll` and implements:

- `HmdDriverFactory`
- `vr::IServerTrackedDeviceProvider`
- `vr::ITrackedDeviceServerDriver`
- `vr::IVRDisplayComponent`

Key source files:

- `driver_plugin_cpp/src/driver_factory.cpp`
- `driver_plugin_cpp/src/device_provider.cpp`
- `driver_plugin_cpp/src/virtual_hmd_device.cpp`
- `driver_plugin_cpp/src/pose_pipe.cpp`

The provider calls:

```cpp
VR_INIT_SERVER_DRIVER_CONTEXT(driver_context);
vr::VRServerDriverHost()->TrackedDeviceAdded(serial, vr::TrackedDeviceClass_HMD, hmd);
```

The HMD calls:

```cpp
vr::VRServerDriverHost()->TrackedDevicePoseUpdated(object_id, pose, sizeof(vr::DriverPose_t));
```

## IPC Frame

Pipe name:

```text
\\.\pipe\SteamVRVirtualHmdPipe
```

C++ and Rust share this 32-byte layout:

```cpp
struct HmdPoseData {
    float position[3];
    float orientation[4]; // x, y, z, w
    uint32_t connected;
};
```

The driver converts quaternion `[x, y, z, w]` into OpenVR's `{w, x, y, z}` `HmdQuaternion_t`.

## Display Component

The fake HMD uses a simple debug desktop display:

- `IsDisplayOnDesktop() = true`
- `IsDisplayRealDisplay() = false`
- Display size: `2160 x 1200`
- Per-eye viewport: `1080 x 1200`
- Recommended render target: `1512 x 1680`
- Projection: symmetric raw projection `[-1, 1]`
- Distortion: identity mapping
- Inverse distortion: identity mapping, returns `true`

This mirrors Valve's `simplehmd` style and avoids hand-written Rust vtable layout issues.

## Controller

`controller_app` sends pose frames at roughly 90Hz.

Controls:

- `W` / `S`: forward / backward
- `A` / `D`: strafe left / right
- `Space` / `Left Ctrl`: up / down
- Mouse movement: yaw / pitch
- `R`: reset pose
- `C`: toggle virtual HMD connected / disconnected

## Build

Build the Rust controller:

```powershell
cd G:\zevy_engine\VRHeadsetEmulator
cargo build
```

Build and package the C++ SteamVR driver:

```powershell
.\scripts\package-driver.ps1 -Profile debug -Driver cpp
```

Release:

```powershell
.\scripts\package-driver.ps1 -Profile release -Driver cpp
```

Generated SteamVR driver layout:

```text
dist\virtual_hmd\
├── driver.vrdrivermanifest
├── driver.vrresources
└── bin\
    └── win64\
        └── driver_virtual_hmd.dll
```

## Register

```powershell
cd "C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64"
.\vrpathreg.exe removedriverswithname virtual_hmd
.\vrpathreg.exe adddriver "G:\zevy_engine\VRHeadsetEmulator\dist\virtual_hmd"
.\vrpathreg.exe show
```

Restart SteamVR after registration.

## Run

Start SteamVR, then run:

```powershell
cd G:\zevy_engine\VRHeadsetEmulator
.\target\debug\controller_app.exe
```

## Logs

C++ driver log:

```powershell
Get-Content "$env:TEMP\VRHeadsetEmulator_driver_cpp.log" -Tail 120
```

SteamVR logs:

```powershell
Get-Content "C:\Users\idesi\AppData\Local\openvr\logs\vrserver.txt" -Tail 160
Get-Content "C:\Users\idesi\AppData\Local\openvr\logs\vrcompositor.txt" -Tail 160
```

Before a clean retry:

```powershell
Stop-Process -Name vrserver,vrmonitor,vrcompositor,vrwebhelper -Force -ErrorAction SilentlyContinue
Remove-Item "$env:TEMP\VRHeadsetEmulator_driver_cpp.log" -Force -ErrorAction SilentlyContinue
.\scripts\package-driver.ps1 -Profile debug -Driver cpp
```

## References

- Valve OpenVR driver header: `third_party/openvr/openvr_driver.h`
- Valve `simplehmd` driver sample: https://chromium.googlesource.com/external/github.com/ValveSoftware/openvr/+/master/samples/drivers/drivers/simplehmd/
- OpenVR Driver API documentation: https://github.com/ValveSoftware/openvr/blob/master/docs/Driver_API_Documentation.md

