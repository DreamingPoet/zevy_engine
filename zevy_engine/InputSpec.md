# zevy_engine Input Spec

## Goal

The input module is the engine-side abstraction for user input.

It must collect input from multiple device families and expose unified events/resources for gameplay systems:

- Keyboard.
- Mouse buttons and mouse motion.
- OpenXR controller actions.
- PICO XR controller buttons, axes, and trigger/grip style inputs through the OpenXR action path.

Gameplay code should not depend directly on Bevy keyboard APIs, mouse APIs, or PICO/OpenXR action details. It should consume the input module's semantic events and state.

## Non-Negotiable Direction

- Keep platform/runtime input details out of scene and game logic.
- Prefer OpenXR interaction profiles for XR controller support.
- PICO-specific controller support should be layered through explicit bindings, not hidden inside generic gameplay code.
- The input module should work on Windows desktop for debug iteration and Android/PICO for runtime validation.
- Input should be usable by both event-driven gameplay and continuously sampled gameplay.

## Initial Architecture

Current module:

- `src/input.rs`

Initial public API:

- `EngineInputPlugin`
- `EngineInputEvent`
- `EngineInputState`
- `InputButton`
- `InputAxis2`
- `InputSource`

Initial events:

- `EngineInputEvent::Button`
- `EngineInputEvent::Axis2`
- `EngineInputEvent::MouseMotion`

Initial state:

- `EngineInputState.buttons`
- `EngineInputState.axes2`
- `EngineInputState.mouse_delta`

Initial schedule sets:

- `EngineInputSet::Reset`
- `EngineInputSet::Collect`
- `EngineInputSet::React`

Gameplay systems that consume current-frame input should run after `EngineInputSet::Collect`.

## Initial Device Coverage

### Keyboard and Mouse

Keyboard/mouse is the first desktop debug input source.

Initial mapping:

- `W`, `A`, `S`, `D`, and arrow keys -> `InputAxis2::Move`.
- `Space` -> `InputButton::PrimaryAction`.
- Left mouse button -> `InputButton::PrimaryAction`.
- Right mouse button -> `InputButton::SecondaryAction`.
- Mouse motion -> `EngineInputEvent::MouseMotion`.

### PICO XR Controllers Through OpenXR

PICO controller input should enter through OpenXR actions first.

Initial OpenXR action mapping:

- Right thumbstick -> `InputAxis2::Move`.
- Right trigger -> `InputButton::PrimaryAction`.

Initial interaction profiles:

- `/interaction_profiles/oculus/touch_controller`
- `/interaction_profiles/valve/index_controller`
- `/interaction_profiles/bytedance/pico4s_controller`

PICO-specific paths were confirmed from `G:\zevy_engine\OpenXR_Native_SDK\Samples\framework\src\openxrWrapper\BasicOpenXrWrapper.cpp`.

`PICO 4 Ultra Enterprise` headset logs confirmed that its runtime-selected controller profile is `/interaction_profiles/bytedance/pico4s_controller`.

Confirmed PICO paths used by the current input module:

- Right thumbstick vector -> `/user/hand/right/input/thumbstick`.
- Right trigger click -> `/user/hand/right/input/trigger/click`.

The PICO native example also exposes trigger analog input through `/user/hand/right/input/trigger/value`. Use that path when the engine needs analog trigger pressure instead of a semantic click button.

## Current Implementation Status

### 2026-06-09: Initial Input Module

Implemented:

- Created `src/input.rs`.
- Created this spec file.
- Added `EngineInputPlugin`.
- Added keyboard and mouse event/state collection.
- Moved OpenXR action creation from `xr.rs` into `input.rs`.
- Moved OpenXR right thumbstick and trigger interpretation into `input.rs`.
- `FogPyramid` and `PerformanceLab` consume `EngineInputState::axis2(InputAxis2::Move)` for player movement.

### 2026-06-09: FogPyramid Camera Movement

Implemented:

- Windows desktop: `FogPyramid` camera moves with `W/A/S/D` and arrow keys.
- Android/PICO XR: `FogPyramid` moves the XR tracking root with the right controller thumbstick.
- Movement is camera-relative and flattened onto the ground plane:
  - forward/back follows the current camera or HMD forward direction,
  - left/right follows the current camera or HMD right direction.
- Removed global input-module locomotion so movement behavior belongs to Level/gameplay logic.
- `FogPyramid` movement runs after `EngineInputSet::Collect`.

### 2026-06-09: PerformanceLab Movement

Implemented:

- `PerformanceLab` is now the default Level.
- Windows desktop: `PerformanceLab` camera moves with `W/A/S/D` and arrow keys.
- Android/PICO XR: `PerformanceLab` moves the XR tracking root with the right controller thumbstick.
- Movement reuses the same Level-side camera/HMD-relative locomotion path as `FogPyramid`.

### 2026-06-09: PICO Controller Binding Fix

Issue:

- Windows `W/A/S/D` movement worked.
- On PICO, controller tracking and hand rendering worked, but OpenXR controller button/axis events did not affect gameplay.

Root cause:

- The input module only suggested Oculus Touch and Valve Index interaction profiles.
- PICO 4 Ultra's OpenXR runtime exposes controller bindings through `/interaction_profiles/bytedance/pico4s_controller`.
- The trigger binding used `/user/hand/right/input/trigger`, while PICO's OpenXR native sample binds trigger click to `/user/hand/right/input/trigger/click` and analog trigger to `/user/hand/right/input/trigger/value`.

Implemented:

- Added the ByteDance PICO controller interaction profile used by PICO 4 Ultra: `/interaction_profiles/bytedance/pico4s_controller`.
- Kept right thumbstick movement on `/user/hand/right/input/thumbstick`.
- Changed the primary trigger button binding to `/user/hand/right/input/trigger/click`.
- Added low-volume XR input logs for active right thumbstick movement and trigger click transitions.

Runtime validation note:

- PICO 4 Ultra logcat reported `/interaction_profiles/bytedance/pico4s_controller` as the selected device profile.
- The same runtime reported `/interaction_profiles/bytedance/pico4_controller`, `/interaction_profiles/bytedance/pico_neo3_controller`, and `/interaction_profiles/bytedance/pico_g3_controller` as unsupported on this device, so those profiles are not included in the current PICO 4 Ultra default binding set.

Validation:

- `cargo check`
- `cargo check --target aarch64-linux-android`
- `.\scripts\build_android_pico.ps1 -Profile release`

## Next Steps

- Add explicit PICO interaction profile bindings if required by headset testing.
- Add grip, A/B/X/Y, menu, and thumbstick-click semantic buttons.
- Add left/right hand source distinction for XR controller buttons.
- Add input buffering or action phases if gameplay needs press/release timing independent of frame rate.
- Add configurable input maps instead of hard-coded startup bindings.
- Add tests for keyboard axis aggregation and button transition events.
