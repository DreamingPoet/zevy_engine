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

The profiling build can also select the deterministic Shadow Motion Lab before
startup. This is a renderer pressure fixture independent of Map_S03B:

```powershell
adb shell setprop debug.zevy.level shadow-motion-16
adb shell setprop debug.zevy.level shadow-motion-32
adb shell setprop debug.zevy.level shadow-motion-64
```

Force-stop and relaunch after changing profiles. Clear it with
`adb shell setprop debug.zevy.level ''` to restore the normal startup Level.
Shipping builds do not read this property.

The same profiling-only path supports fixed renderer A/B overrides, also read
once before Bevy plugins and shaders are installed:

```powershell
adb shell setprop debug.zevy.point_direct 0
adb shell setprop debug.zevy.point_shadows 1
adb shell setprop debug.zevy.world_reservoir 1
adb shell setprop debug.zevy.cluster_preselection 0
adb shell setprop debug.zevy.dynamic_overlay 1
adb shell setprop debug.zevy.shadow_updates 2
adb shell setprop debug.zevy.shadow_hz 8
adb shell setprop debug.zevy.hero_samples 2
adb shell setprop debug.zevy.tail_samples 2
adb shell setprop debug.zevy.exact_lights 8
adb shell setprop debug.zevy.local_lighting forward
```

`debug.zevy.local_lighting` accepts `forward` or `deferred`. `deferred` is the
full-resolution G-buffer reference, not the reduced-rate product path. It
forces effective MSAA to 1x and is intended for fixed A/B captures. The HUD
prints the active lighting pipeline so screenshots cannot silently mix them.

PointLight selection A/B uses two independent switches. The product default is
`world_reservoir=1, cluster_preselection=0` (one real-cluster scan with a
world-anchored reservoir). Set both to `0` for the scalar two-scan reference.
Set `world_reservoir=0, cluster_preselection=1` only to reproduce the rejected
aggressive 2x2 screen-supercluster path and its maximum-performance bound.
When both are `1`, the world-space path takes precedence.

`debug.zevy.exact_lights` sets the compile-time exact local-list threshold for
the next app start. The source default is currently `18`; the previously
VR-validated Map_S03B visual baseline used the explicit value `8`. Clusters at
or below the selected threshold sum every shadowed BRDF exactly, while denser
overflow enters the experimental reservoir. `4` reproduces the raw reservoir
cost bound; `6` still showed shadow blotches; `8` removed both the screen-space
turning blocks and world-space shadow blotches in that map. Use the complete
profile light count (`16`, `32`, or `64`) as the named all-exact quality
reference for a Shadow Motion Lab A/B; this is a correctness reference, not a
performance default.

Boolean values accept `0/1`, `false/true`, `off/on`, or `no/yes`. Empty or
invalid values retain `RenderQualityConfig::default()`. Force-stop and relaunch
the application after changing a property. These overrides are compiled only
for Android builds that include `render_debug`; Shipping ignores them.

The repeatable Shadow Motion Lab profiler wraps these startup properties, cold
launches the app, waits for warmup, parses PICO `PxrMetric`, and reports average
plus P95 values:

```powershell
.\scripts\profile_shadow_motion_lab.ps1 `
  -Serial PA9410MGJ9260457G `
  -LightCount 16 `
  -Mode full `
  -LightingPipeline forward `
  -WarmupSeconds 30 `
  -SampleCount 20 `
  -GpuFrequencyMHz 599
```

`Mode` is one of `geometry`, `direct`, `shadow`, or `full`. Set
`GpuFrequencyMHz 0` only when deliberately accepting all non-zero DVFS samples;
the result then reports the observed min/average/max frequency. The script does
not install an APK and does not make a thermal-soak result from a short sample.

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

`Slow shadow xfade A/S wait W copy C` reports the sparse SlowMoving shadow
reconstruction path. `A` is the number of active old/new snapshot blends, `S`
is the effective transition-slot capacity after the RenderDevice texture-array
limit is applied, `W` is the number of due lights waiting for a slot, and `C`
is the number of whole cubemaps copied on this frame. A stable scene normally
shows `0/S`; that does not mean the feature is disabled. The Materials/Lights
page also reports transition starts and maximum world-space snapshot staleness.
These values are shared by both XR eyes: per-eye disagreement is a correctness
failure, not expected sampling noise.

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
