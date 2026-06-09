# zevy_engine Spec

## Project Goal

`zevy_engine` is a custom VR engine for producing real-time, first-person, interactive VR experiences.

The engine must support:

- PBR materials.
- Multiple light sources.
- OpenXR-based XR runtime integration.
- Vulkan-based rendering.
- Android XR devices, with PICO 4 Ultra as the first target device.
- Windows as the editor, debug, and desktop validation platform.

This project is not a one-off demo. It should evolve into a reusable engine foundation that can support production content.

## Non-Negotiable Direction

- OpenXR is the primary XR abstraction.
- Vulkan is the primary rendering backend.
- PICO-specific work must be layered on top of the OpenXR/Vulkan path where possible.
- Windows support exists to make iteration, debugging, tools, and editor workflows practical.
- Android/PICO support exists to validate the real production runtime path.
- Engine code and content/demo code must remain clearly separated.
- The project should move in small, verifiable iterations.

## Current Project Shape

- Main project: Rust, Bevy, and OpenXR.
- Current crate path: `G:\zevy_engine\zevy_engine`.
- Current entry point: `src/main.rs`.
- PICO XR UE5.5 plugin source exists at `G:\zevy_engine\PICOXR`.
- The PICO XR UE source is reference material only. It should inform Android manifest settings, PICO permissions, native runtime behavior, input mapping, and performance features, but it should not turn this project into a UE plugin port.

## Platform Strategy

### Windows

Windows is the development and debug platform.

Required uses:

- Fast `cargo check` and desktop validation.
- Debugging engine systems before deploying to device.
- Future editor/tooling work.
- Desktop XR validation when useful.

Windows must remain easy to run while Android support is added.

### Android XR / PICO 4 Ultra

PICO 4 Ultra is the first production device target.

Required uses:

- Build an installable Android arm64 APK.
- Launch as a VR app on the headset.
- Create an OpenXR instance and session through the Android runtime.
- Render through Vulkan.
- Track HMD pose.
- Support controller input.
- Validate performance and lifecycle behavior on real hardware.

Initial Android support should target `arm64-v8a` only.

## Android Environment Baseline

The local Android environment observed during planning:

- SDK path currently exposed by environment variables: `F:\AndriodSDK\AndriodSDK`.
- JDK: `F:\AndriodSDK\AndriodSDK\JAVA\jdk-17.0.10`.
- Installed NDKs:
  - `25.1.8937393`.
  - `27.0.12077973`.
- First build pass should prefer NDK `25.1.8937393`, matching the provided Unreal Android SDK screenshot.
- Installed Android platforms include `android-29`, `android-34`, and newer.

Important: the screenshot used `F:\AndroidSDK\AndroidSDK`, but the actual local environment uses `F:\AndriodSDK\AndriodSDK`. This mismatch must be handled explicitly before Android build work.

## PICO Reference Notes

The PICO XR UE5.5 source is useful for identifying PICO Android requirements.

Important reference file:

- `G:\zevy_engine\PICOXR\Source\PICOXRHMD\PICOXR_UPL.xml`.

Useful PICO reference items:

- `pvr.app.type=vr`.
- PICO controller and hand tracking manifest metadata.
- PICO hand, eye, face, body, spatial anchor, scene, and mesh permissions.
- Vulkan-first runtime behavior.
- PICO runtime version queries.
- PICO Java and native SDK integration patterns.

Initial Rust/OpenXR implementation should not copy PICO UE Java/JAR/native SDK code unless standard OpenXR Android runtime initialization proves insufficient.

## Architecture Principles

- Keep engine systems modular and testable.
- Prefer cross-platform OpenXR behavior before device-specific APIs.
- Put PICO-specific behavior behind explicit platform/device gates.
- Avoid hard-coding PICO assumptions into generic XR systems.
- Keep rendering, XR session management, input, platform packaging, and demo scene logic separable.
- Preserve the desktop path while adding Android support.
- Add logging around all platform boundaries: runtime creation, session state, swapchain setup, input binding, lifecycle events, and device capabilities.

## Iteration Roadmap

### Phase 1: Build Baseline

Goal: preserve the current working desktop baseline.

Tasks:

- Keep `cargo check` passing on Windows.
- Confirm `cargo run` desktop mode remains usable.
- Confirm `cargo run -- --xr` remains the explicit XR launch mode.
- Document any platform-specific assumptions.

Exit criteria:

- Windows `cargo check` passes.
- Existing scene still compiles.

### Phase 2: Android Toolchain

Goal: make the project capable of producing Android artifacts.

Tasks:

- Install Rust target `aarch64-linux-android`.
- Install or choose Android packaging tooling, such as `cargo-apk` or `cargo-ndk` plus Gradle.
- Standardize SDK, NDK, and JDK paths.
- Add a repeatable Android build command or PowerShell script.

Exit criteria:

- Android target compilation starts from a clean command.
- Toolchain versions are documented.

### Phase 3: Android APK Skeleton

Goal: produce an installable APK before solving all XR behavior.

Tasks:

- Add Android package metadata.
- Add Android manifest template.
- Configure app label, package id, version code, and version name.
- Package required native libraries and assets.
- Target `arm64-v8a`.

Exit criteria:

- APK builds successfully.
- APK installs on a connected Android device.

### Phase 4: PICO 4 Ultra OpenXR Launch

Goal: launch the app as a VR app on PICO 4 Ultra.

Tasks:

- Add minimum PICO VR manifest metadata.
- Verify Android OpenXR loader initialization.
- Log runtime name, runtime version, system id, view configuration, blend mode, swapchain format, and render resolution.
- Handle Android activity lifecycle events cleanly.

Exit criteria:

- App launches on PICO 4 Ultra.
- OpenXR instance and session are created.
- Session state transitions are visible in logs.

### Phase 5: Vulkan Stereo Rendering

Goal: render the current scene through OpenXR on device.

Tasks:

- Confirm Vulkan is used on Android.
- Confirm swapchain creation and frame submission.
- Validate stereo view rendering.
- Keep the scene simple while proving render correctness.

Exit criteria:

- PICO headset displays the test scene.
- No persistent black screen after session begins.
- Frame loop remains stable.

### Phase 6: PICO Controller Input

Goal: support first-person interactive control on PICO 4 Ultra.

Tasks:

- Detect or confirm PICO controller interaction profile.
- Add PICO controller bindings for locomotion and trigger input.
- Keep existing desktop and other controller bindings where useful.
- Log action sync and input state changes.

Exit criteria:

- Right thumbstick moves the XR tracking root.
- Trigger input is detected.
- Input behavior is stable after pause/resume.

### Phase 6A: Engine Input Module

Goal: provide a reusable input abstraction for gameplay and engine logic.

Input should be handled by a dedicated module, not directly inside XR runtime or scene code.

Required uses:

- Collect keyboard input for Windows editor/debug iteration.
- Collect mouse button and mouse motion input for Windows editor/debug iteration.
- Collect OpenXR controller actions for Android XR/PICO runtime input.
- Map device-specific input into semantic engine input events and state.
- Allow gameplay systems to consume input without depending on Bevy keyboard/mouse APIs or OpenXR action details.

Initial implementation:

- `InputSpec.md` records input module goals and progress.
- `src/input.rs` owns `EngineInputPlugin`, `EngineInputEvent`, `EngineInputState`, semantic buttons, semantic axes, keyboard/mouse collection, and OpenXR controller action interpretation.
- `src/xr.rs` no longer owns gameplay input interpretation.
- PICO/OpenXR input is currently represented by:
  - right thumbstick -> `InputAxis2::Move`,
  - right trigger -> `InputButton::PrimaryAction`.

Future tasks:

- Add explicit PICO interaction profile bindings after headset/runtime path confirmation.
- Add grip, A/B/X/Y, menu, thumbstick-click, left/right hand source detail, and configurable input maps.
- Add tests around keyboard axis aggregation and button source aggregation.

### Phase 7: Production Rendering Features

Goal: grow from prototype rendering toward production VR visuals.

Tasks:

- Expand PBR material coverage.
- Validate multiple dynamic and static lights.
- Add performance-aware lighting constraints for mobile VR.
- Profile render cost on PICO 4 Ultra.

Exit criteria:

- Multiple-light PBR scene renders on Windows and PICO.
- Performance costs are visible and documented.

### Phase 7A: Level and Scene Organization

Goal: keep scene/content work separate from platform, XR session, and rendering infrastructure.

The engine should borrow Unreal Engine's Level concept at the project level:

- A Level is a named loadable scene unit.
- The engine has one default Level.
- Runtime code can request opening another Level.
- Level entities should be tagged so they can be unloaded cleanly.
- Demo/test Levels must not be mixed into Android lifecycle or OpenXR session code.

Initial implementation:

- `LevelId::FogPyramid` is the current default Level.
- `LevelId::PerformanceLab` remains available as a performance stress-test Level.
- `OpenLevel(LevelId)` is the first runtime API for Level switching.
- `CurrentLevel` tracks the active Level.
- `LevelEntity` marks entities owned by the currently opened Level.
- `LevelId::Empty` exists as a reserved minimal Level for future loading and lifecycle tests.

Future tasks:

- Move each substantial Level into its own file or folder.
- Add asset-backed Level descriptions when hand-authored Rust spawning becomes too limiting.
- Add loading transition rules for XR so Level switches do not disturb the OpenXR session.
- Add editor/debug commands for setting default Level and opening Levels on Windows.

### Phase 8: Vulkan Multi-view Rendering

Goal: add a performance-oriented stereo rendering path for mobile XR.

Vulkan Multi-view is a medium-term rendering target, especially for PICO 4 Ultra and other mobile XR devices. It should be introduced after the basic OpenXR Vulkan stereo path is stable.

Tasks:

- Detect Vulkan Multi-view feature and extension support at runtime.
- Add a Multi-view stereo render path when supported by the device.
- Keep the normal two-eye stereo path as a fallback.
- Compare CPU/GPU frame cost against the non-Multi-view stereo path.
- Document device support and measured behavior on PICO 4 Ultra.

Exit criteria:

- Engine can report whether Vulkan Multi-view is supported.
- Multi-view can be enabled only when supported.
- Unsupported devices continue to render through the normal stereo path.
- PICO 4 Ultra performance comparison is recorded.

### Phase 9: PICO-Specific Enhancements

Goal: add useful PICO features without compromising the OpenXR-first architecture.

Possible tasks:

- Hand tracking.
- Passthrough / MR features.
- Spatial anchors.
- Scene mesh.
- Foveated rendering.
- Refresh rate or performance controls.

Exit criteria:

- Each PICO-specific feature is gated, documented, and optional.
- Generic OpenXR path remains intact.

## Development Checklist

Before each implementation step:

- Confirm the work supports the project goal.
- Confirm whether the change belongs to engine, platform, rendering, XR, input, or demo content.
- Check that Windows debug flow remains intact.
- Check that Android/PICO work does not leak into generic systems unnecessarily.

During implementation:

- Prefer small commits or small logical changes.
- Keep logs useful at platform boundaries.
- Avoid large unrelated refactors.
- Keep build commands repeatable.

Before considering a phase complete:

- Run the relevant local build or check.
- Record commands used.
- Record device/runtime observations if testing on PICO.
- Update this spec if the project direction changes.

## Current Immediate Plan

The next development step should be:

1. Standardize Android SDK, NDK, and JDK paths for CLI builds.
2. Add Rust Android target support.
3. Choose and configure the Android packaging path.
4. Build the first `arm64-v8a` APK skeleton.
5. Install and test on PICO 4 Ultra once the device is connected through ADB.

Do not start by porting the PICO UE plugin. Use it as reference while preserving the Rust + Bevy + OpenXR + Vulkan engine direction.

## Progress Log

Complex resolved issues are tracked in `ResolvedComplexIssues.md`. Use that document for root-cause records of hard Android/PICO/OpenXR/rendering problems; keep this spec focused on project direction, current status, and next development priorities.

### 2026-06-09: PICO 4 Ultra First Visible XR Scene

Status: major bring-up milestone reached.

Validated on headset:

- Android APK builds, installs, and launches on PICO 4 Ultra.
- PICO OpenXR runtime loads successfully.
- Vulkan OpenXR swapchain is created.
- Black loading screen blocker was fixed by moving Android XR runtime execution closer to the PICO Native SDK sample model:
  - Android XR disables the winit event loop.
  - Android XR uses a schedule runner for continuous engine ticks.
  - NativeActivity lifecycle events are polled explicitly.
  - Android headless window mode uses `WindowPlugin.exit_condition = DontExit`.
- The test scene now renders in the headset.
- XR hand tracking is detected.
- XR hand rendering is stable and correct.
- PICO runtime metrics show sustained frame submission instead of a single submitted frame.

Important implementation notes:

- Do not reintroduce a winit-driven Android XR frame loop unless there is a clear reason and a full lifecycle test.
- `primary_window = None` on Android XR is valid only with `ExitCondition::DontExit`.
- Keep PICO Native SDK samples under `G:\zevy_engine\OpenXR_Native_SDK` as the main lifecycle reference.
- `XR_FB_display_refresh_rate` is available on the target runtime and can be queried/requested, but refresh-rate request alone did not fix the earlier loading issue.
- `com.picovr.globalui.permission.GLOBAL_UI` is a signature/system permission and should not be relied on for normal APK builds.

Current headset-observed issues:

1. Left-eye scene rendering flickers about once per second.
2. Right-eye scene rendering flickers occasionally.
3. XR hand tracking/rendering is stable and does not flicker.
4. Frame rate is too low, observed around 18 FPS in headset.

Current interpretation:

- Because XR hand rendering is stable while the scene flickers, the next investigation should focus on the scene render path, stereo projection layer submission, camera/render target lifetime, or PBR/depth handling rather than basic tracking.
- Left-eye-dominant flicker suggests an eye-index, array-layer, swapchain image, view order, or per-eye camera extraction issue should be investigated before adding new features.
- Low FPS is now a Phase 5/7 blocker. Stabilize stereo rendering and reduce frame cost before expanding scene complexity.

Next immediate plan:

1. Reduce diagnostics/logging and remove frame-step spam from the Android build path.
2. Add focused render diagnostics for per-eye camera/view index, swapchain image index, layer count, and `should_render`.
3. Test a minimal unlit/mobile scene to separate PBR/lighting cost from XR frame-loop cost.
4. Check whether hand-rendering and scene-rendering use different pipelines or layers.
5. Inspect vendored `bevy_mod_openxr` stereo rendering for array-layer or left/right eye handling bugs.
6. Profile frame time on PICO and target stable 72 Hz as the first performance goal.

### 2026-06-09: Android Release Performance Baseline

Status: performance blocker largely resolved for the current demo scene.

Changes made:

- Android/PICO build and deploy scripts now default to the optimized `release` APK path.
- `scripts\build_android_pico.ps1 -Profile debug` and `scripts\deploy_pico.ps1 -Profile debug` remain available for slower diagnostic builds.
- The release APK is signed with the local Android debug keystore for device iteration only. This is not a production/store signing key.
- Android XR no longer spawns a desktop mirror camera, because the APK runs headless with `primary_window = None`.
- Android XR no longer emits per-frame winit redraw requests after disabling winit.
- The Android test scene disables dynamic shadows and reduces sphere mesh subdivision to lower mobile frame cost.
- Removed experimental per-frame JNI attach calls from the vendored OpenXR render frame release/end path.

Validation commands:

- `cargo check --target aarch64-linux-android`
- `.\scripts\build_android_pico.ps1 -Profile release`
- `.\scripts\deploy_pico.ps1 -Profile release`

PICO 4 Ultra logcat metrics from the release APK:

- `Pkg=com.zevy.engine`
- Sustained `PXRSDK_PM ENGINE FPS` around `89-90`.
- `PxrMetric` reports `FPS=88-90/90` over the sampled window.
- `FrmCpu` dropped from the debug build's approximate `16-26ms` range to about `3.3-5.6ms` after warmup.
- `FrmGpu` is about `5.6-10.7ms` in this demo scene, so the next optimization pass should watch GPU cost as scene complexity grows.
- `LayerCnt=3`, matching the expected simple projection/hand/XR composition footprint better than the previous debug run's noisy layer count.

Current interpretation:

- The observed 18 FPS issue was primarily caused by testing an unoptimized debug APK on device.
- The release APK now meets the first performance target of stable 72 Hz and is close to the device's current 90 Hz runtime rate.
- The left/right eye flicker still needs headset-side visual confirmation on this release APK. If flicker remains at 89-90 FPS, continue investigating stereo swapchain array-layer handling, eye camera extraction, projection layer view ordering, and `should_render`/session timing.

Next immediate plan:

1. Ask for headset confirmation on the release APK: left-eye flicker, right-eye flicker, hand stability, and perceived latency.
2. If flicker remains, add focused per-eye render diagnostics without increasing per-frame log pressure.
3. If flicker is resolved, move to the next production milestone: OpenXR session lifecycle cleanup and stable Android XR release build documentation.

### 2026-06-09: Stereo Scene Stability on PICO 4 Ultra

Status: resolved for current demo scene.

Validated on headset:

- Left eye remains stable.
- Right eye scene rendering is stable after disabling indirect drawing for XR cameras.
- Reaching hands into view no longer causes scene/background flicker.
- XR hand tracking remains stable in both eyes.
- Final release APK removes the temporary right-eye magenta diagnostic clear color.

Important implementation notes:

- The main OpenXR projection layer should remain opaque.
- OpenXR swapchain acquire/wait and manual texture view insertion must happen before Bevy prepares view targets.
- Current XR cameras use `NoIndirectDrawing` to avoid Bevy GPU preprocessing/indirect drawing instability in Android OpenXR stereo rendering.
- Do not remove `NoIndirectDrawing` from XR cameras without testing both eyes on PICO 4 Ultra with hand tracking visible.
- The detailed root-cause writeup is in `ResolvedComplexIssues.md`.

Validation commands:

- `cargo check --target aarch64-linux-android`
- `.\scripts\build_android_pico.ps1 -Profile release`
- `.\scripts\deploy_pico.ps1 -Profile release`

PICO 4 Ultra logcat metrics from the final release APK:

- `Pkg=com.zevy.engine`
- Sustained `PXRSDK_PM ENGINE FPS` around `89-90`.
- `PxrMetric` reports `FPS=89-90/90`.
- `FrmCpu` about `4.8-5.2ms` in the sampled window.
- `FrmGpu` about `8.9-9.5ms` in the sampled window after disabling indirect drawing.

Next immediate plan:

1. Confirm final no-diagnostic-color headset view with the user.
2. Keep Android XR release build as the baseline path.
3. Start the next production milestone: OpenXR lifecycle cleanup, input interaction profile cleanup, and mobile rendering budget tracking.

### 2026-06-09: Dynamic Lighting Performance Test Scene

Status: implemented as a stress scene and then adjusted into a more practical mobile XR lighting test.

Changes made:

- Replaced the simple demo object with multiple small PBR primitives no larger than about `0.5m`.
- Added varied materials: metallic, glossy, matte, plastic, ceramic, and rubber-like surfaces.
- Added 8 low-saturation colored dynamic point lights.
- Enabled dynamic shadows on all 8 lights.
- Added slow orbit animation for the lights around the model cluster.

First PICO 4 Ultra release APK metrics with all 8 lights casting shadows:

- `Pkg=com.zevy.engine`
- Observed `PXRSDK_PM ENGINE FPS` around `23-30` after warmup.
- `PxrMetric` reported roughly `FPS=23-30/90`.
- `FrmCpu` was about `14-16ms`.
- `FrmGpu` was about `25-38ms`.
- GPU was frequently at or near maximum clock/load.

Current interpretation:

- The current stress scene is GPU-bound.
- 8 shadow-casting point lights are too expensive for the current mobile XR baseline, especially with stereo rendering and `NoIndirectDrawing` enabled for XR camera stability.
- Keep this scene as a stress/performance test, not as the default production budget target.

Follow-up adjustment:

- The 8 dynamic point lights remain in the scene.
- Each light now has a visible small self-emissive sphere marker so its position can be observed in the headset.
- Only 2 lights, currently `OrbitLight0` and `OrbitLight4`, cast dynamic shadows.
- The other 6 lights still move and illuminate the scene, but do not render shadow maps.
- Light marker meshes are marked `NotShadowCaster` so they do not add unnecessary shadow workload.

Validation after the follow-up adjustment:

- `cargo check --target aarch64-linux-android` passed.
- `.\scripts\build_android_pico.ps1 -Profile release` built and signed the release APK.
- `.\scripts\deploy_pico.ps1 -Profile release` installed and launched the APK on PICO 4 Ultra.
- CLI logcat sampling reported `PXRSDK_PM ENGINE FPS` around `72-73` and `PxrMetric FPS=71-72/72`.
- The sampled `LayerCnt=0` and very low reported `FrmGpu` suggest this log window should be treated as a deployment/performance smoke test only until headset visual confirmation verifies that the full XR scene was visible.

Next immediate plan:

1. Use this scene to measure optimization work.
2. Add quality tiers for dynamic light count, shadow count, and shadow resolution.
3. Explore cheaper lighting strategies for production scenes: fewer shadow casters, baked/static lighting where possible, clustered light limits, and selective shadows.

### 2026-06-09: Code Organization and Initial Level System

Status: first architecture cleanup implemented.

Changes made:

- `src/lib.rs` is now only the crate/native entry point.
- `src/app.rs` owns launch mode selection, plugin assembly, and global app startup.
- `src/platform.rs` owns Android NativeActivity bridging, Android lifecycle polling, display refresh-rate setup, and Android OpenXR session begin behavior.
- `src/xr.rs` owns OpenXR plugin composition, XR anchor visuals, mirror camera sync, and XR render/state logging.
- `src/input.rs` owns keyboard/mouse/OpenXR input abstraction, XR action setup, semantic input events/state, and the first locomotion input consumer.
- `src/scene.rs` owns the Level system: default Level, current Level, `OpenLevel`, unload/load flow.
- `src/scene/levels.rs` owns concrete Level content for the current prototype Levels.
- The previous performance lighting scene is now `LevelId::PerformanceLab`.
- The default Level is configured through `DefaultLevel(LevelId::FogPyramid)`.
- Runtime Level switching starts with the `OpenLevel(LevelId)` event.

Validation commands:

- `cargo check`
- `cargo check --target aarch64-linux-android`
- `.\scripts\build_android_pico.ps1 -Profile release`

Current interpretation:

- Platform/XR code and scene code now have a cleaner boundary.
- The first Level system is intentionally simple and entity-tag based.
- Future Level work should extend `src/scene/levels.rs` or split larger Levels into dedicated files under `src/scene/`, instead of adding scene content back into `lib.rs`.

### 2026-06-09: Initial Engine Input Module

Status: implemented.

Changes made:

- Added `InputSpec.md`.
- Added `src/input.rs`.
- Added `EngineInputPlugin`.
- Added `EngineInputEvent` for semantic button, axis, and mouse-motion events.
- Added `EngineInputState` for continuously sampled semantic input state.
- Added keyboard/mouse input collection:
  - `W/A/S/D` and arrow keys -> `InputAxis2::Move`,
  - `Space` and left mouse button -> `InputButton::PrimaryAction`,
  - right mouse button -> `InputButton::SecondaryAction`.
- Moved OpenXR action creation from `src/xr.rs` to `src/input.rs`.
- Moved PICO/OpenXR right thumbstick and right trigger interpretation into `src/input.rs`.
- Kept XR locomotion as the first gameplay-facing consumer of `EngineInputState::axis2(InputAxis2::Move)`.

Validation commands:

- `cargo check`
- `cargo check --target aarch64-linux-android`
- `.\scripts\build_android_pico.ps1 -Profile release`

### 2026-06-09: FogPyramid Default Level

Status: implemented.

Source reference:

- Bevy official `3D Rendering / Fog` example: `https://bevy.org/examples/3d-rendering/fog/`.

Changes made:

- Added `LevelId::FogPyramid`.
- Set `DefaultLevel(LevelId::FogPyramid)`.
- Ported only the scene content from the Bevy fog example:
  - stone pillar structure,
  - stepped pyramid,
  - translucent green orb,
  - large gray unlit sky cube,
  - one shadow-casting point light,
  - fixed linear distance fog.
- Removed the Bevy example's Controls, Key Binding, UI text, fog editing, and orbiting camera update system.
- Added `ActiveLevelFog` so fog settings are applied to generated XR cameras as well as desktop cameras.

Validation commands:

- `cargo check`
- `cargo check --target aarch64-linux-android`
- `.\scripts\build_android_pico.ps1 -Profile release`
