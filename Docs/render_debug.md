# Zevy Render Debug HUD

The render debug HUD is compiled by the `render_debug` Cargo feature. It is enabled by default for development builds and is rendered separately for the desktop camera and both OpenXR eye cameras.

## Controls

- `F3`: show or hide the HUD.
- `F4`: cycle Overview, Full-frame Workload, GPU/Render Passes, and Materials/Lights pages.
- Right controller `A`: show or hide the HUD in XR.
- Right controller `B`: cycle the HUD page in XR.
- `--no-debug-hud`: start with the HUD hidden.
- `--debug-hud-page=workload`: start on the aggregated Workload page.
- `--debug-hud-page=passes`: start on the Passes page.
- `--debug-hud-page=materials`: start on the Materials page.

Android NativeActivity does not provide ordinary desktop argv. Profiling APKs
therefore accept a debug-only Android system property before launch:

```powershell
adb shell setprop debug.zevy.hud_page workload
```

Accepted values are `overview`, `workload`, `passes`, and `materials`. Clear the
override with `adb shell setprop debug.zevy.hud_page ''`. The property is read
only at startup and is absent from builds made with `--no-default-features`.

The same profiling-only path supports fixed renderer A/B overrides, also read
once before Bevy plugins and shaders are installed:

```powershell
adb shell setprop debug.zevy.point_direct 0
adb shell setprop debug.zevy.point_shadows 1
adb shell setprop debug.zevy.dynamic_overlay 1
adb shell setprop debug.zevy.shadow_updates 2
adb shell setprop debug.zevy.shadow_hz 8
adb shell setprop debug.zevy.hero_samples 2
adb shell setprop debug.zevy.tail_samples 2
```

Boolean values accept `0/1`, `false/true`, `off/on`, or `no/yes`. Empty or
invalid values retain `RenderQualityConfig::default()`. Force-stop and relaunch
the application after changing a property. These overrides are compiled only
for Android builds that include `render_debug`; Shipping ignores them.

## Metrics

- FPS, CPU frame time, process CPU and memory usage.
- Visible mesh instances, unique meshes and visible triangles per eye.
- Estimated main-view draw calls after basic opaque instance grouping.
- GPU timestamp or CPU command-recording time for Bevy-instrumented render passes.
- Pipeline primitive and fragment invocation counts when the GPU supports pipeline statistics queries.
- Visible StandardMaterial texture slots and high-cost material flags.
- Light and estimated shadow-view counts.

Draw-call counts marked `est` are estimates. Bevy 0.16 does not expose an exact public draw-command counter, especially when GPU multi-draw and indirect rendering are active.

On Android Vulkan, `elapsed_gpu` can cover only the Bevy diagnostic spans that
were instrumented. It is not guaranteed to include tile resolve/store work,
runtime composition, uninstrumented render-graph nodes, or the whole XR frame.
The HUD therefore labels these values `GPU spans (partial)` and does not infer a
CPU bottleneck merely because their sum is far below frame time. Use PICO
`PxrMetric`, Android GPU Inspector, or the device vendor profiler for total GPU
frame time; use the HUD spans only for relative A/B of the same instrumented
work.

`Shadow-enabled` is the number of scene PointLights whose shadow flag is on.
`Cache faces R/D/U` reports actual resident/drawn/reused cubemap faces for the
current frame. These are intentionally separate: an enabled light count must
not be mislabeled as actual cache residency.

## Shipping build

Build without the default crate features to remove the HUD systems, UI dependency, render query collection, controller debug actions, and debug-only GPU feature requests:

```powershell
cargo build --release --no-default-features
```

For Android/cargo-apk, pass the same `--no-default-features` flag to the build command used by the packaging pipeline.

The project helper script now produces a HUD-free Shipping-style APK by default:

```powershell
.\scripts\build_android_pico.ps1 -Profile release
```

Build a release-optimized profiling APK with the HUD enabled using:

```powershell
.\scripts\build_android_pico.ps1 -Profile release -RenderDebug
```
