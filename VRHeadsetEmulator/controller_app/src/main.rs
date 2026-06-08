use std::fs::OpenOptions;
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

use device_query::{DeviceQuery, DeviceState, Keycode};
use hmd_protocol::{HmdPoseData, PIPE_NAME};
use nalgebra::{UnitQuaternion, Vector3};

const TICK_RATE: Duration = Duration::from_micros(11_111);
const MOVE_SPEED_METERS_PER_SECOND: f32 = 1.5;
const MOUSE_SENSITIVITY_RADIANS: f32 = 0.0025;
const MAX_PITCH_RADIANS: f32 = 89.0_f32.to_radians();

fn main() -> io::Result<()> {
    println!("=== SteamVR Virtual HMD Controller ===");
    println!("W/S forward, A/D strafe, Space/Ctrl vertical, mouse rotates.");
    println!("R resets pose, C toggles connected/disconnected.");

    let mut stream = connect_to_driver_pipe()?;
    println!("IPC connected. Sending 6DOF frames at roughly 90Hz.");

    let device_state = DeviceState::new();
    let mut pose_state = PoseState::default();
    let mut last_mouse = device_state.get_mouse().coords;
    let mut was_toggle_pressed = false;
    let mut next_tick = Instant::now();

    loop {
        let now = Instant::now();
        if now < next_tick {
            thread::sleep(next_tick - now);
        }
        next_tick += TICK_RATE;

        let keys = device_state.get_keys();
        let mouse = device_state.get_mouse();
        let mouse_delta = (mouse.coords.0 - last_mouse.0, mouse.coords.1 - last_mouse.1);
        last_mouse = mouse.coords;

        if keys.contains(&Keycode::R) {
            pose_state = PoseState::default();
        }

        let is_toggle_pressed = keys.contains(&Keycode::C);
        if is_toggle_pressed && !was_toggle_pressed {
            pose_state.connected = !pose_state.connected;
            println!(
                "Virtual HMD {}",
                if pose_state.connected {
                    "connected"
                } else {
                    "disconnected"
                }
            );
        }
        was_toggle_pressed = is_toggle_pressed;

        pose_state.integrate(&keys, mouse_delta, TICK_RATE.as_secs_f32());
        if let Err(error) = stream.write_all(pose_state.to_hmd_pose().as_bytes()) {
            eprintln!("IPC disconnected: {error}. Reconnecting ...");
            stream = connect_to_driver_pipe()?;
            println!("IPC reconnected.");
        }
    }
}

fn connect_to_driver_pipe() -> io::Result<std::fs::File> {
    loop {
        match OpenOptions::new().write(true).open(PIPE_NAME) {
            Ok(stream) => return Ok(stream),
            Err(error) => {
                eprintln!("Waiting for driver pipe {PIPE_NAME}: {error}");
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

struct PoseState {
    position: Vector3<f32>,
    yaw: f32,
    pitch: f32,
    connected: bool,
}

impl Default for PoseState {
    fn default() -> Self {
        let pose = HmdPoseData::default();
        Self {
            position: Vector3::new(pose.position[0], pose.position[1], pose.position[2]),
            yaw: 0.0,
            pitch: 0.0,
            connected: true,
        }
    }
}

impl PoseState {
    fn integrate(&mut self, keys: &[Keycode], mouse_delta: (i32, i32), dt: f32) {
        self.yaw -= mouse_delta.0 as f32 * MOUSE_SENSITIVITY_RADIANS;
        self.pitch = (self.pitch - mouse_delta.1 as f32 * MOUSE_SENSITIVITY_RADIANS)
            .clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS);

        let rotation = self.rotation();
        let forward = rotation * Vector3::new(0.0, 0.0, -1.0);
        let right = rotation * Vector3::new(1.0, 0.0, 0.0);
        let up = Vector3::new(0.0, 1.0, 0.0);

        let forward_axis = key_axis(keys, Keycode::W, Keycode::S);
        let strafe_axis = key_axis(keys, Keycode::D, Keycode::A);
        let vertical_axis = key_axis(keys, Keycode::Space, Keycode::LControl);

        let movement = forward * forward_axis + right * strafe_axis + up * vertical_axis;
        let movement = movement.try_normalize(f32::EPSILON).unwrap_or_default()
            * MOVE_SPEED_METERS_PER_SECOND
            * dt;

        self.position += movement;
    }

    fn to_hmd_pose(&self) -> HmdPoseData {
        let rotation = self.rotation();
        let q = rotation.quaternion();
        HmdPoseData {
            position: [self.position.x, self.position.y, self.position.z],
            orientation: [q.i, q.j, q.k, q.w],
            connected: u32::from(self.connected),
        }
    }

    fn rotation(&self) -> UnitQuaternion<f32> {
        UnitQuaternion::from_euler_angles(self.pitch, self.yaw, 0.0)
    }
}

fn key_axis(keys: &[Keycode], positive: Keycode, negative: Keycode) -> f32 {
    let mut axis = 0.0;
    if keys.contains(&positive) {
        axis += 1.0;
    }
    if keys.contains(&negative) {
        axis -= 1.0;
    }
    axis
}
