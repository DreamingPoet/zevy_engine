# Resolved Complex Issues

This document records difficult project issues that required multi-step investigation. Keep each entry focused on symptoms, false leads, root cause, and the final solution so future development does not repeat the same path.

## 2026-06-09: PICO 4 Ultra Black Loading Screen

### Status

Resolved.

### Symptoms

- APK installed and launched on PICO 4 Ultra.
- The headset remained in a black loading state.
- Logcat showed Vulkan and PICO OpenXR runtime initialization, but the app did not continue into stable XR rendering.
- Bevy reported behavior consistent with the app exiting because no desktop window existed.

### Key Clues

- Android used `NativeActivity`, not a desktop window.
- PICO Native SDK samples drive the app from Android lifecycle events and keep polling the native app loop.
- A headless Android XR run with `primary_window = None` must not use Bevy's default "exit when all windows close" behavior.

### Investigation Path

1. Compared the app startup path with PICO Native SDK samples under `G:\zevy_engine\OpenXR_Native_SDK`.
2. Verified that PICO's OpenXR runtime requires Android Activity context for `XR_KHR_android_create_instance`.
3. Confirmed the runtime could create a Vulkan OpenXR swapchain.
4. Found that the Bevy Android path still behaved like a windowed app unless explicitly configured otherwise.

### Root Cause

The Android XR runtime path was still too close to a desktop/window-driven Bevy app. On PICO, the app needed a NativeActivity/OpenXR lifecycle loop and a headless window configuration that does not exit when no normal window exists.

### Final Solution

- Initialize `ndk_context` with the Android Activity pointer before starting OpenXR.
- Poll Android NativeActivity lifecycle events explicitly.
- Disable winit on Android XR.
- Use `ScheduleRunnerPlugin` for continuous Android XR ticking.
- Use `WindowPlugin` with:
  - `primary_window = None`
  - `exit_condition = DontExit`
  - `close_when_requested = false`
- Start the OpenXR session after action attach and after required Android/PICO runtime setup.

### Validation

- APK installs and launches on PICO 4 Ultra.
- Scene renders in headset.
- XR hand tracking initializes and renders.
- PICO metrics show sustained frame submission instead of a single submitted frame.

### Lessons

- Do not reintroduce a winit-driven Android XR frame loop without a full PICO lifecycle test.
- `primary_window = None` on Android XR is valid only if the app is explicitly configured not to exit.
- Keep PICO Native SDK samples as the lifecycle reference, while preserving the Rust + Bevy + OpenXR + Vulkan engine direction.

## 2026-06-09: Debug APK Limited PICO Runtime to About 18 FPS

### Status

Resolved.

### Symptoms

- The scene rendered, but headset-observed frame rate was around 18 FPS.
- PICO metrics reported the app around `17-19/90`.
- `FrmGpu` was extremely low in the debug build, while CPU frame time was high.

### Key Clues

- The APK path was `target\debug\apk\zevy_engine.apk`.
- Logcat showed CPU-side frame time in the approximate `16-26ms` range.
- GPU time was not the bottleneck in the debug build.

### Investigation Path

1. Reduced scene complexity to rule out obvious mobile GPU overload.
2. Removed unnecessary per-frame Android redraw events after disabling winit.
3. Removed the desktop mirror camera from Android XR.
4. Checked the actual build profile used by the packaging script.

### Root Cause

The PICO performance baseline was being measured with an unoptimized debug APK. Bevy's ECS and render pipeline are far too expensive in debug mode for XR frame-rate evaluation.

### Final Solution

- Changed Android build/deploy scripts to default to `release`.
- Kept `-Profile debug` available for diagnostic builds.
- Added a release profile in `Cargo.toml`:
  - `opt-level = 3`
  - `lto = "thin"`
  - `codegen-units = 1`
  - `debug = false`
  - `strip = "symbols"`
  - `panic = "abort"`
- Signed the release iteration APK with the local Android debug keystore for device testing.

### Validation

- `.\scripts\build_android_pico.ps1 -Profile release`
- `.\scripts\deploy_pico.ps1 -Profile release`
- PICO metrics improved from about `17-19/90` to `88-90/90`.
- CPU frame time dropped to roughly `3.3-5.6ms` after warmup in the simple demo scene.

### Lessons

- Never use a debug APK as the performance baseline for Android XR.
- Always record the build profile when comparing headset performance.
- If `FrmGpu` is low but FPS is low, suspect CPU/debug build overhead first.

## 2026-06-09: Stereo Scene Flicker and Right-Eye Black/Magenta on PICO 4 Ultra

### Status

Resolved for the current Bevy 0.16 + OpenXR stereo scene path.

### Symptoms

- After release build fixes, left eye became stable at about 90 FPS.
- Right eye was nearly black.
- With diagnostic clear color, the right eye showed a magenta background, proving the right-eye render target was being cleared.
- The scene only flashed into the right eye when XR hands appeared.
- XR hand tracking/rendering was stable in both eyes.

### Key Clues

- The right-eye array layer was writable because it showed the diagnostic clear color.
- Hand tracking rendered correctly in both eyes, so the OpenXR session, tracking, and composition were not fundamentally broken.
- Removing projection layer alpha blending did not fix right-eye scene rendering.
- Disabling frustum culling on scene meshes did not fix the issue.
- Moving OpenXR acquire/wait/texture view update earlier fixed left-eye flicker when hands appeared, but right-eye scene rendering still failed.
- Adding `NoIndirectDrawing` to both XR cameras made both eyes stable.

### Investigation Path

1. Confirmed stereo swapchain setup:
   - one OpenXR swapchain
   - `array_size = 2`
   - left eye writes array layer 0
   - right eye writes array layer 1
   - projection layer submits image array indices 0 and 1
2. Removed `BLEND_TEXTURE_SOURCE_ALPHA` from the main projection layer because the primary scene should be opaque.
3. Added per-eye diagnostic clear colors:
   - left eye dark blue
   - right eye magenta
4. Verified right-eye target writes were working because the right eye displayed magenta.
5. Disabled mesh frustum culling on demo scene meshes; issue persisted.
6. Moved OpenXR frame begin/acquire/wait and manual texture view update from `XrRenderSet::PreRender` to `RenderSet::ManageViews`, before Bevy prepares view attachments.
7. Added `NoIndirectDrawing` to XR cameras; issue resolved.

### Root Cause

The scene rendering issue was not caused by OpenXR swapchain array-layer writes or basic camera clearing. The unstable part was Bevy's GPU preprocessing / indirect drawing path with the current OpenXR stereo multi-view setup on Android/PICO.

The right eye could clear normally but did not consistently receive stable scene mesh draw results until GPU indirect drawing was disabled for the XR cameras. Hand rendering used a different path and remained stable, which is why hand tracking could appear correctly while the scene failed.

### Final Solution

- Keep OpenXR swapchain image acquire/wait and manual texture view insertion in `RenderSet::ManageViews`, before `prepare_view_attachments`.
- Keep the main OpenXR projection layer opaque.
- Add `NoIndirectDrawing` to spawned XR cameras so scene meshes use the non-indirect drawing path for stereo XR rendering.
- Remove temporary per-eye diagnostic clear colors after validation.
- Remove temporary demo-mesh `NoFrustumCulling` workaround because it was not the root cause.

### Validation

Headset observation after adding `NoIndirectDrawing`:

- Left eye stable.
- Right eye stable.
- Reaching hands into view no longer causes scene/background flicker.
- XR hand tracking remains stable in both eyes.
- PICO metrics remain around `89-90/90` in the current demo scene.

### Tradeoffs

- Disabling indirect drawing may increase GPU cost compared with the ideal GPU preprocessing path.
- Current metrics remain acceptable for the demo scene, but future large scenes must be profiled.
- Long-term optimization can revisit Bevy GPU preprocessing after the OpenXR stereo path is fully characterized.

### Lessons

- If a stereo eye clears correctly but does not render scene meshes, separate render-target validity from render-phase/mesh-queue validity.
- Hand rendering stability does not guarantee the scene render path is stable; they may use different render paths.
- For XR bring-up, correctness and stereo stability come before advanced GPU preprocessing.
- Keep diagnostic clear colors temporary and remove them once the root cause is confirmed.

## 2026-06-09: Startup Crash After FogPyramid Movement System

### Status

Resolved.

### Symptoms

- `cargo run` on Windows exited immediately.
- PICO launched to the startup page and then returned home.
- Android logcat showed `Fatal signal 6 (SIGABRT)` in `android_main`.
- Windows panic showed Bevy error `B0001` in `zevy_engine::scene::levels::move_fog_pyramid_player`.

### Root Cause

`move_fog_pyramid_player` had two mutable `Transform` queries in the same system:

- `Query<&mut Transform, With<FogPyramidPlayerCamera>>`
- `Query<&mut Transform, With<XrTrackingRoot>>`

Even though those entities are intended to be different, Bevy cannot prove the filters are disjoint. Because both queries mutably access `Transform`, Bevy rejects the system at startup to prevent aliasing.

### Final Solution

- Replaced the two independent mutable queries with a `ParamSet`.
- Desktop camera movement uses `transforms.p0()`.
- XR tracking-root movement uses `transforms.p1()`.

### Validation

- `cargo check` passed.
- Short `cargo run` no longer panicked during startup.
- `cargo check --target aarch64-linux-android` passed.
- Release APK built successfully.
- Release APK deployed to PICO 4 Ultra and entered the XR frame loop without `Fatal signal`, panic, or `B0001`.

### Lessons

- When a Bevy system needs two mutable queries over the same component type, use `ParamSet` unless the query filters are explicitly disjoint through `Without<T>` or equivalent filters.
- Startup-time Bevy ECS validation failures on Android may appear as native `SIGABRT`, so always reproduce with Windows `cargo run` when possible.

## 2026-06-09: PICO Controller Actions Did Not Reach Gameplay

### Status

Code-side OpenXR binding issue resolved for PICO 4 Ultra. Live input still needs one headset check with the right controller awake.

### Symptoms

- Windows `W/A/S/D` movement worked in `FogPyramid`.
- On PICO 4 Ultra, controller/hand tracking could render, but the right controller thumbstick and trigger did not affect gameplay.

### Root Cause

Gameplay movement was already correctly wired through `EngineInputState::axis2(InputAxis2::Move)`. The missing part was the OpenXR action binding layer for PICO controller input.

- The input module only suggested Oculus Touch and Valve Index interaction profiles.
- PICO 4 Ultra uses `/interaction_profiles/bytedance/pico4s_controller`.
- The trigger binding used `/user/hand/right/input/trigger`, while PICO's OpenXR native sample binds trigger click through `/user/hand/right/input/trigger/click`.

### Investigation Path

- Checked `G:\zevy_engine\OpenXR_Native_SDK\Samples\framework\src\openxrWrapper\BasicOpenXrWrapper.cpp`.
- Confirmed PICO controller paths:
  - right thumbstick vector: `/user/hand/right/input/thumbstick`
  - right trigger click: `/user/hand/right/input/trigger/click`
  - right trigger analog: `/user/hand/right/input/trigger/value`
- Deployed to PICO 4 Ultra Enterprise and checked logcat.
- The runtime selected `/interaction_profiles/bytedance/pico4s_controller`.
- The same runtime reported `/interaction_profiles/bytedance/pico4_controller`, `/interaction_profiles/bytedance/pico_neo3_controller`, and `/interaction_profiles/bytedance/pico_g3_controller` as unsupported on this headset.

### Final Solution

- Added `/interaction_profiles/bytedance/pico4s_controller` to the OpenXR controller binding profile set.
- Kept right thumbstick movement bound to `/user/hand/right/input/thumbstick`.
- Changed the primary trigger button binding to `/user/hand/right/input/trigger/click`.
- Added low-volume logs for XR thumbstick movement and trigger transitions:
  - `XR controller move axis: ...`
  - `XR controller trigger click: ...`
- Removed non-Ultra PICO profiles from the default binding set to avoid unsupported-profile runtime errors on PICO 4 Ultra.

### Validation

- `cargo check` passed.
- `cargo check --target aarch64-linux-android` passed.
- Release APK built successfully.
- Release APK deployed to PICO 4 Ultra.
- PICO logcat confirmed:
  - app starts in XR mode,
  - `FogPyramid` opens,
  - `/interaction_profiles/bytedance/pico4s_controller` is suggested and selected,
  - unsupported ByteDance profile errors are gone after narrowing the binding set.

### Remaining Runtime Check

- The deployment-time logcat sample showed both controllers offline, so no live thumbstick/trigger action was observed in that capture.
- When the right controller is awake/connected, verify:
  - pushing the right thumbstick prints `XR controller move axis`,
  - clicking the trigger prints `XR controller trigger click` and `Primary action pressed from XrController`.
