use std::f32::consts::FRAC_PI_2;

use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};

use crate::input::EngineInputState;

const DEFAULT_MOVE_SPEED: f32 = 5.0;
const MIN_MOVE_SPEED: f32 = 0.5;
const MAX_MOVE_SPEED: f32 = 80.0;
const SPRINT_MULTIPLIER: f32 = 3.0;
const MOVE_ACCELERATION: f32 = 28.0;
const MOVE_DECELERATION: f32 = 36.0;
const LOOK_RADIANS_PER_DOT: f32 = 0.0025;
const MAX_PITCH: f32 = FRAC_PI_2 - 0.01;
const SCROLL_SPEED_STEP: f32 = 1.2;

#[derive(Component, Debug)]
pub(super) struct DesktopLevelPlayer {
    initialized: bool,
    move_speed: f32,
    velocity: Vec3,
}

impl Default for DesktopLevelPlayer {
    fn default() -> Self {
        Self {
            initialized: false,
            move_speed: DEFAULT_MOVE_SPEED,
            velocity: Vec3::ZERO,
        }
    }
}

#[derive(Resource, Debug, Default)]
pub(super) struct DesktopPlayerCursorState {
    captured: bool,
    blocked_until_right_release: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update_desktop_level_player(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    input_state: Res<EngineInputState>,
    mut cursor_state: ResMut<DesktopPlayerCursorState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut players: Query<(&mut Transform, &mut DesktopLevelPlayer), With<Camera3d>>,
) {
    let Ok((mut transform, mut player)) = players.single_mut() else {
        release_cursor_if_owned(&mut windows, &mut cursor_state);
        return;
    };

    if !player.initialized {
        player.initialized = true;
        info!(
            "Desktop Level player enabled: WASD move, hold RMB to look, Q/E down/up, Shift sprint, mouse wheel changes speed, Esc releases cursor"
        );
    }

    update_cursor_capture(&keyboard, &mouse_buttons, &mut windows, &mut cursor_state);
    update_move_speed(&mouse_scroll, &mut player);

    if cursor_state.captured {
        apply_mouse_look(input_state.mouse_delta(), &mut transform);
    }

    let movement_input = desktop_movement_input(&keyboard, transform.rotation);
    let speed = if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
        player.move_speed * SPRINT_MULTIPLIER
    } else {
        player.move_speed
    };
    let desired_velocity = movement_input * speed;
    let acceleration = if movement_input == Vec3::ZERO {
        MOVE_DECELERATION
    } else {
        MOVE_ACCELERATION
    };
    player.velocity = approach_velocity(
        player.velocity,
        desired_velocity,
        acceleration * time.delta_secs(),
    );
    transform.translation += player.velocity * time.delta_secs();
}

fn update_cursor_capture(
    keyboard: &ButtonInput<KeyCode>,
    mouse_buttons: &ButtonInput<MouseButton>,
    windows: &mut Query<&mut Window, With<PrimaryWindow>>,
    cursor_state: &mut DesktopPlayerCursorState,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };

    if keyboard.just_pressed(KeyCode::Escape) || !window.focused {
        cursor_state.blocked_until_right_release = true;
    } else if !mouse_buttons.pressed(MouseButton::Right) {
        cursor_state.blocked_until_right_release = false;
    }

    let should_capture = mouse_buttons.pressed(MouseButton::Right)
        && !cursor_state.blocked_until_right_release
        && window.focused;
    if cursor_state.captured == should_capture {
        return;
    }

    cursor_state.captured = should_capture;
    window.cursor_options.grab_mode = if should_capture {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    window.cursor_options.visible = !should_capture;
}

fn release_cursor_if_owned(
    windows: &mut Query<&mut Window, With<PrimaryWindow>>,
    cursor_state: &mut DesktopPlayerCursorState,
) {
    if !cursor_state.captured {
        return;
    }

    cursor_state.captured = false;
    cursor_state.blocked_until_right_release = false;
    if let Ok(mut window) = windows.single_mut() {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
    }
}

fn update_move_speed(scroll: &AccumulatedMouseScroll, player: &mut DesktopLevelPlayer) {
    let scroll_lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 16.0,
    };
    if scroll_lines.abs() <= f32::EPSILON {
        return;
    }

    player.move_speed = adjusted_move_speed(player.move_speed, scroll_lines);
    info!("Desktop Level player speed: {:.2} m/s", player.move_speed);
}

fn adjusted_move_speed(current_speed: f32, scroll_lines: f32) -> f32 {
    (current_speed * SCROLL_SPEED_STEP.powf(scroll_lines)).clamp(MIN_MOVE_SPEED, MAX_MOVE_SPEED)
}

fn apply_mouse_look(mouse_delta: Vec2, transform: &mut Transform) {
    if mouse_delta == Vec2::ZERO {
        return;
    }

    let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    yaw -= mouse_delta.x * LOOK_RADIANS_PER_DOT;
    pitch = (pitch - mouse_delta.y * LOOK_RADIANS_PER_DOT).clamp(-MAX_PITCH, MAX_PITCH);
    transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
}

fn desktop_movement_input(keyboard: &ButtonInput<KeyCode>, rotation: Quat) -> Vec3 {
    let mut planar_input = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        planar_input.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        planar_input.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        planar_input.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        planar_input.x -= 1.0;
    }

    let forward = (rotation * Vec3::NEG_Z).normalize_or_zero();
    let right = (rotation * Vec3::X).normalize_or_zero();
    let mut movement = right * planar_input.x + forward * planar_input.y;
    if keyboard.pressed(KeyCode::KeyE) {
        movement.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyQ) {
        movement.y -= 1.0;
    }
    movement.normalize_or_zero()
}

fn approach_velocity(current: Vec3, target: Vec3, max_delta: f32) -> Vec3 {
    let delta = target - current;
    let distance = delta.length();
    if distance <= max_delta || distance <= f32::EPSILON {
        target
    } else {
        current + delta / distance * max_delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_approaches_target_without_overshooting() {
        assert_eq!(
            approach_velocity(Vec3::ZERO, Vec3::X * 10.0, 2.0),
            Vec3::X * 2.0
        );
        assert_eq!(
            approach_velocity(Vec3::X * 9.5, Vec3::X * 10.0, 2.0),
            Vec3::X * 10.0
        );
    }

    #[test]
    fn pitch_is_clamped_for_free_look() {
        let mut transform = Transform::IDENTITY;
        apply_mouse_look(Vec2::new(0.0, -100_000.0), &mut transform);
        let (_, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
        assert!(pitch <= MAX_PITCH + 0.0001);
    }

    #[test]
    fn q_and_e_move_down_and_up() {
        let mut keyboard = ButtonInput::default();
        keyboard.press(KeyCode::KeyE);
        assert!(desktop_movement_input(&keyboard, Quat::IDENTITY).y > 0.99);

        keyboard.release(KeyCode::KeyE);
        keyboard.press(KeyCode::KeyQ);
        assert!(desktop_movement_input(&keyboard, Quat::IDENTITY).y < -0.99);
    }

    #[test]
    fn mouse_wheel_adjusts_and_clamps_speed() {
        assert!(adjusted_move_speed(5.0, 1.0) > 5.0);
        assert!(adjusted_move_speed(5.0, -1.0) < 5.0);
        assert_eq!(adjusted_move_speed(MAX_MOVE_SPEED, 100.0), MAX_MOVE_SPEED);
        assert_eq!(adjusted_move_speed(MIN_MOVE_SPEED, -100.0), MIN_MOVE_SPEED);
    }
}
