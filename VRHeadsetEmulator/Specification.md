
This document provides the complete structural design, communication protocols, architecture, and code scaffolding for developing a Virtual VR Headset Driver and an external 6DOF Mouse/Keyboard Controller using Rust.

---

## 1. Project Overview & Objectives

### 1.1 Context & Problem Statement
When developing VR applications, games, or testing OpenXR/SteamVR plugins, developers frequently face the friction of putting on and taking off physical hardware. Existing simulation tools are either closed-source, heavily restricted, or tightly coupled to specific runtime frameworks. 

### 1.2 Core Objectives
* **Hardwareless Execution**: Allow SteamVR to initialize and run normally on a Windows machine without any physical VR headset attached.
* **Full 6DOF Simulation**: Provide full six-degrees-of-freedom tracking via traditional keyboard and mouse inputs.
* **Ultra-Low Latency IPC**: Establish a performant data bridge syncing the controller state to the driver at $\ge$ 90Hz frequency with sub-5ms latency.

---

## 2. System Architecture & Topology

The system uses a decoupled **Dual-Component Architecture** consisting of a **Driver DLL** (running inside the privileged SteamVR service process) and a standalone **Controller Executable** (running in user space). This prevents security contexts or blocking input loops from crashing the OpenVR runtime.

```text
+-----------------------------------------------------------------------+
|                              Windows OS                               |
|                                                                       |
|   +---------------------------------+                                 |
|   |      Controller Application     |                                 |
|   |       (controller_app.exe)      |                                 |
|   +---------------------------------+                                 |
|                    |                                                  |
|                    | 6DOF Pose Data (Binary Struct)                   |
|                    v [Windows Named Pipe IPC]                         |
|   +---------------------------------------------------------------+   |
|   |   SteamVR Server Process (vrserver.exe)                       |   |
|   |                                                               |   |
|   |   +-------------------------------------------------------+   |   |
|   |   |              Virtual HMD Driver Plugin                |   |   |
|   |   |               (driver_virtual_hmd.dll)                |   |   |
|   |   |                                                       |   |   |
|   |   |  * Exports: HmdDriverFactory                           |   |   |
|   |   |  * Implements: ITrackedDeviceServerDriver             |   |   |
|   |   +-------------------------------------------------------+   |   |
|   +---------------------------------------------------------------+   |
+-----------------------------------------------------------------------+

```

### 2.1 Communication Protocol (IPC)

* **Mechanism**: Windows Named Pipes.
* **Pipe Name**: `\\\\.\\pipe\\SteamVRVirtualHmdPipe`
* **Mode**: Unidirectional / Message-based stream.
* **Serialization**: Raw C-Packed representation to eliminate serialization overhead.

#### Shared Frame Layout

```rust
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct HmdPoseData {
    /// 3D Position vector in meters [X, Y, Z]
    pub position: [f32; 3],
    /// Orientation Quaternion [X, Y, Z, W]
    pub orientation: [f32; 4],
}

```

---

## 3. Driver Module Specification (driver_plugin)

The driver is a dynamic link library (`.dll`) which interfaces with Valve's OpenVR C++ ABI. Rust compiles to native code conforming to this contract using `cdylib`.

### 3.1 Exposed Interfaces

1. **`HmdDriverFactory`**: The singular entrypoint exported by the DLL. It returns requested pointers to interface implementations matching specific version strings (e.g., `IServerTrackedDeviceProvider_004`).
2. **`IServerTrackedDeviceProvider`**: Controls the life cycle of the driver inside SteamVR.
* `Init`: Spawns a background thread creating the Named Pipe server and registers the virtual device.
* `Cleanup`: Closes active pipe handles and safely tears down memory resources.


3. **`ITrackedDeviceServerDriver`**: Represents the virtual HMD instance.
* `Activate`: Called when SteamVR wakes the device. Sets critical device properties (Resolution, Refresh Rate, IPD).
* `GetPose`: Queried continuously by SteamVR. Must convert the incoming IPC `HmdPoseData` into OpenVR's internal `DriverPose_t` tracking format.



### 3.2 Target HMD Device Properties

The driver must announce the following hardware properties to the runtime during activation:

* `Prop_UserIpdMeters_Float` = `0.063` (Standard 63mm IPD)
* `Prop_DisplayFrequency_Float` = `90.0` (90Hz refresh rate target)
* `Prop_SecondsFromVmtosPhotons_Float` = `0.011` (Simulated display latency)
* `Prop_ContainsProximitySensor_Bool` = `true` (Tricking runtime into thinking a face is inside the mask)

---

## 4. Controller Module Specification (controller_app)

An independent binary application responsible for translating low-level hardware polling into smooth spatial kinematics.

### 4.1 Kinematics & Math Model

The application tracks position ($\vec{P}$) and orientation ($Q_{current}$) iteratively through raw input deltas:

1. **Rotational Orientation (Mouse Engine)**:
* Poll relative screen movement differentials ($\Delta x$, $\Delta y$).
* Update internal Euler angles:

$$\text{yaw} = \text{yaw} - (\Delta x \times \text{Sensitivity})$$


$$\text{pitch} = \text{pitch} - (\Delta y \times \text{Sensitivity})$$


* Clamping: $\text{pitch}$ must be explicitly restricted to $[-89.0^\circ, +89.0^\circ]$ to prevent gimbal lock or inverted camera states.
* Construct orientation quaternion: $Q_{current} = \mathcal{Q}(\text{yaw}, \text{pitch}, 0)$


2. **Translational Movement (Keyboard Engine)**:
* Poll keystates for `W`/`S` (Forward/Backward), `A`/`D` (Left/Right), `Space`/`LeftCtrl` (Up/Down).
* Derive directional unit vectors directly from $Q_{current}$:

$$\vec{F} = Q_{current} \times [0, 0, -1]$$


$$\vec{R} = Q_{current} \times [1, 0, 0]$$


$$\vec{U} = [0, 1, 0] \quad (\text{Locked to absolute global Up})$$


* Compute absolute update step:

$$\vec{P}_{new} = \vec{P}_{old} + (\vec{F} \cdot w\_axis) + (\vec{R} \cdot d\_axis) + (\vec{U} \cdot space\_axis)$$




3. **Transmission Pipeline**:
The logic executes on a strict fixed update loop timer set to roughly $11.1\text{ms}$ ($90\text{Hz}$) to smoothly match the OpenVR frame pipeline.

---

## 5. Engineering Blueprint & Scaffolding

### 5.1 Cargo Workspace Tree

```text
vr_virtual_hmd_project/
├── Cargo.toml                  # Workspace Manifest
├── driver_plugin/              # Driver Crate (Compiles into a DLL)
│   ├── Cargo.toml
│   └── src
│       ├── lib.rs              # FFI Interfaces and Lifecycle hooks
│       └── openvr_abi.rs       # Reconstructed OpenVR struct representations
└── controller_app/             # Controller Crate (Compiles into an EXE)
    ├── Cargo.toml
    └── src
        └── main.rs             # Input processing, math engine, & IPC Client

```

### 5.2 Manifest Specifications

#### Workspace Configuration (`/Cargo.toml`)

```toml
[workspace]
members = [
    "driver_plugin",
    "controller_app"
]

```

#### Driver Crate Configuration (`/driver_plugin/Cargo.toml`)

```toml
[package]
name = "driver_virtual_hmd"
version = "0.1.0"
edition = "2021"

[lib]
name = "driver_virtual_hmd"
crate-type = ["cdylib"] # Must compile down to standard Windows DLL

[dependencies]
windows-sys = { version = "0.52", features = ["Win32_Foundation", "Win32_System_Pipes"] }
nalgebra = "0.32"       # For quaternion and matrix translations

```

#### Controller Crate Configuration (`/controller_app/Cargo.toml`)

```toml
[package]
name = "controller_app"
version = "0.1.0"
edition = "2021"

[dependencies]
windows-sys = { version = "0.52", features = ["Win32_Foundation", "Win32_Storage_FileSystem", "Win32_System_Pipes"] }
device_query = "1.1"    # Simple platform-agnostic keyboard state poller

```

---

## 6. Target Code Base Layout

### 6.1 Driver Plugin Implementation Scaffold (`/driver_plugin/src/lib.rs`)

```rust
use std::ffi::{c_char, c_void, CStr};
use std::ptr;

#[repr(C)]
pub struct DriverPose_t {
    pub poseIsValid: bool,
    pub deviceIsConnected: bool,
    pub qWorldFromDriverRotation: [f64; 4],
    pub qDriverFromHeadRotation: [f64; 4],
    pub vecPosition: [f64; 3],
    pub result: i32,
}

/// Entrypoint expected by SteamVR runtime loader.
/// The function signature must remain unmangled and match standard 'C' ABI.
#[no_mangle]
pub unsafe extern "C" fn HmdDriverFactory(
    pInterfaceName: *const c_char,
    pReturnCode: *mut i32
) -> *mut c_void {
    if pInterfaceName.is_null() {
        if !pReturnCode.is_null() {
            *pReturnCode = 1; // HmdError_Init_InterfaceNotFound
        }
        return ptr::null_mut();
    }

    let interface_str = CStr::from_ptr(pInterfaceName).to_str().unwrap_or("");
    
    // Check version strings against implemented OpenVR boundaries
    if interface_str.starts_with("IServerTrackedDeviceProvider_") {
        if !pReturnCode.is_null() {
            *pReturnCode = 0; // HmdError_None
        }
        // In full production, instantiate your struct instance implementing VTable mappings here.
        println!("[VirtualHMD] SteamVR runtime matching interface: {}", interface_str);
    }
    
    ptr::null_mut()
}

```

### 6.2 Controller Client Implementation Scaffold (`/controller_app/src/main.rs`)

```rust
use std::fs::OpenOptions;
use std::io::Write;
use std::thread;
use std::time::Duration;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct HmdPoseData {
    position: [f32; 3],
    orientation: [f32; 4],
}

fn main() {
    println!("=== SteamVR Virtual HMD Control Terminal ===");
    let pipe_path = "\\\\.\\pipe\\SteamVRVirtualHmdPipe";
    
    println!("Connecting to active driver pipeline at: {}...", pipe_path);
    
    // Ensure SteamVR process has loaded your virtual driver and generated the pipe endpoint
    match OpenOptions::new().write(true).open(pipe_path) {
        Ok(mut stream) => {
            println!("IPC Channel online. Dispatching kinematic frames to OpenVR runtime.");
            let mut mock_x = 0.0;
            
            loop {
                mock_x += 0.005; // Simulate horizontal tracking shift
                
                let pose = HmdPoseData {
                    position: [mock_x, 1.75, -0.5], // Centered roughly at stand height
                    orientation: [0.0, 0.0, 0.0, 1.0], // Native unit Identity Quaternion
                };
                
                let bytes: &[u8] = unsafe {
                    std::slice::from_raw_parts(
                        &pose as *const HmdPoseData as *const u8,
                        std::mem::size_of::<HmdPoseData>(),
                    )
                };
                
                if let Err(e) = stream.write_all(bytes) {
                    eprintln!("Pipeline closed or dropped by SteamVR context: {}", e);
                    break;
                }
                
                thread::sleep(Duration::from_millis(11)); // Fast match tracking rate (approx ~90Hz)
            }
        }
        Err(e) => {
            eprintln!("Connection failed! Verify that SteamVR is running with your driver deployed: {}", e);
        }
    }
}

```

---

## 7. Runtime Deployment & Registration Guide

### 7.1 Required Directory Topography

SteamVR scans targeted structural patterns to dynamically mount third-party drivers. Construct the following layout locally:

```text
C:\Driver_VirtualHMD\
├── driver.vrresources              # Manifest telling SteamVR how to hook the module
└── bin\
    └── win64\
        └── driver_virtual_hmd.dll   # Output of target cargo cdylib build

```

#### Manifest Manifest Construction (`/driver.vrresources`)

```json
{
  "drivername" : "virtual_hmd",
  "resources" : "resources",
  "webextensions" : "webextensions",
  "legacy_bindings" : true
}

```

### 7.2 Registering the Directory via SteamVR Environment Paths

Invoke SteamVR’s built-in registration application via an elevated PowerShell session to map the driver into runtime configuration state:

```powershell
# Navigate to your native SteamVR runtime binaries
cd "C:\\Program Files (x86)\\Steam\\steamapps\\common\\SteamVR\\bin\\win64"

# Bind the custom layout into the active driver engine
./vrpathreg.exe adddriver "C:\Driver_VirtualHMD"

```

### 7.3 Testing Execution Tree

1. **Initialize Engine**: Launch SteamVR directly via the Steam Client. The runtime configuration environment will read the mapped pointer, load your Rust DLL, and show an inactive standalone HMD profile status icon.
2. **Launch Tracking Client**: Spin up `controller_app.exe`. It hooks into the opened named pipe.
3. **Validate Tracking Loop**: Launch the SteamVR "Display Mirror" tool. Move keys or slide inputs on the controller console app; observe smooth view camera reactions inside the workspace layer.

```

```