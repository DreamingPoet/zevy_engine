# Zevy Render Debug HUD

The render debug HUD is compiled by the `render_debug` Cargo feature. It is enabled by default for development builds and is rendered separately for the desktop camera and both OpenXR eye cameras.

## Controls

- `F3`: show or hide the HUD.
- `F4`: cycle Overview, GPU/Render Passes, and Materials/Lights pages.
- Right controller `A`: show or hide the HUD in XR.
- Right controller `B`: cycle the HUD page in XR.
- `--no-debug-hud`: start with the HUD hidden.
- `--debug-hud-page=passes`: start on the Passes page.
- `--debug-hud-page=materials`: start on the Materials page.

## Metrics

- FPS, CPU frame time, process CPU and memory usage.
- Visible mesh instances, unique meshes and visible triangles per eye.
- Estimated main-view draw calls after basic opaque instance grouping.
- GPU timestamp or CPU command-recording time for Bevy-instrumented render passes.
- Pipeline primitive and fragment invocation counts when the GPU supports pipeline statistics queries.
- Visible StandardMaterial texture slots and high-cost material flags.
- Light and estimated shadow-view counts.

Draw-call counts marked `est` are estimates. Bevy 0.16 does not expose an exact public draw-command counter, especially when GPU multi-draw and indirect rendering are active.

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
