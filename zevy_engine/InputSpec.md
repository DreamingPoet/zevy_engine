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

PICO-specific paths should be added after the runtime profile strings and path behavior are confirmed on headset logs or PICO documentation.

## Current Implementation Status

### 2026-06-09: Initial Input Module

Implemented:

- Created `src/input.rs`.
- Created this spec file.
- Added `EngineInputPlugin`.
- Added keyboard and mouse event/state collection.
- Moved OpenXR action creation from `xr.rs` into `input.rs`.
- Moved OpenXR right thumbstick and trigger interpretation into `input.rs`.
- Kept XR locomotion as the first gameplay-facing consumer of `EngineInputState::axis2(InputAxis2::Move)`.

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
