use std::fs::OpenOptions;
use std::io::{self, Write};
use std::mem;
use std::ptr;
use std::time::{Duration, Instant};

use hmd_protocol::{HmdPoseData, PIPE_NAME};
use nalgebra::{UnitQuaternion, Vector3};
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, TextOutW, PAINTSTRUCT},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_A, VK_C, VK_D, VK_E, VK_Q, VK_R, VK_S, VK_W},
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
            LoadCursorW, PostQuitMessage, RegisterClassW, SetTimer, TranslateMessage, CS_HREDRAW,
            CS_VREDRAW, CW_USEDEFAULT, HMENU, IDC_ARROW, MSG, SW_SHOW, WM_DESTROY, WM_KEYDOWN,
            WM_KEYUP, WM_KILLFOCUS, WM_MOUSEMOVE, WM_PAINT, WM_SETFOCUS, WM_TIMER, WNDCLASSW,
            WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        },
    },
};

const TICK_RATE: Duration = Duration::from_micros(11_111);
const MK_RBUTTON: usize = 0x0002;
const TIMER_ID: usize = 1;
const TIMER_INTERVAL_MS: u32 = 11;
const MOVE_SPEED_METERS_PER_SECOND: f32 = 1.5;
const MOUSE_SENSITIVITY_RADIANS: f32 = 0.0025;
const MAX_PITCH_RADIANS: f32 = 89.0_f32.to_radians();

fn main() -> io::Result<()> {
    println!("=== SteamVR Virtual HMD Controller ===");
    println!("Input is active only while the controller window is focused.");
    println!("W/S forward, A/D strafe, Q/E vertical, hold right mouse to rotate.");
    println!("R resets pose, C toggles connected/disconnected.");

    let app = Box::new(ControllerApp::new());
    run_window(app)
}

fn run_window(app: Box<ControllerApp>) -> io::Result<()> {
    let class_name = wide_null("VRHeadsetEmulatorControllerWindow");
    let window_title = wide_null("VRHeadsetEmulator Controller");

    unsafe {
        let instance = GetModuleHandleW(ptr::null());
        let mut window_class: WNDCLASSW = mem::zeroed();
        window_class.style = CS_HREDRAW | CS_VREDRAW;
        window_class.lpfnWndProc = Some(window_proc);
        window_class.hInstance = instance;
        window_class.lpszClassName = class_name.as_ptr();
        window_class.hCursor = LoadCursorW(0, IDC_ARROW);

        if RegisterClassW(&window_class) == 0 {
            return Err(io::Error::last_os_error());
        }

        let app_ptr = Box::into_raw(app);
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            560,
            260,
            0,
            0 as HMENU,
            instance,
            app_ptr.cast(),
        );

        if hwnd == 0 {
            drop(Box::from_raw(app_ptr));
            return Err(io::Error::from_raw_os_error(GetLastError() as i32));
        }

        windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(hwnd, SW_SHOW);
        SetTimer(hwnd, TIMER_ID, TIMER_INTERVAL_MS, None);

        let mut message: MSG = mem::zeroed();
        while GetMessageW(&mut message, 0, 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

struct ControllerApp {
    pose_state: PoseState,
    input_state: InputState,
    stream: Option<std::fs::File>,
    last_tick: Instant,
    status: String,
}

impl ControllerApp {
    fn new() -> Self {
        Self {
            pose_state: PoseState::default(),
            input_state: InputState::default(),
            stream: None,
            last_tick: Instant::now(),
            status: "Waiting for SteamVR driver pipe".to_string(),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_tick)
            .as_secs_f32()
            .clamp(0.0, TICK_RATE.as_secs_f32() * 3.0);
        self.last_tick = now;

        if self.input_state.focused {
            if self.input_state.take_reset() {
                self.pose_state = PoseState::default();
                self.status = "Pose reset".to_string();
            }

            if self.input_state.take_connect_toggle() {
                self.pose_state.connected = !self.pose_state.connected;
                self.status = if self.pose_state.connected {
                    "Virtual HMD connected".to_string()
                } else {
                    "Virtual HMD disconnected".to_string()
                };
            }

            let mouse_delta = self.input_state.take_mouse_delta();
            self.pose_state.integrate(
                &self.input_state,
                mouse_delta,
                dt.max(TICK_RATE.as_secs_f32()),
            );
        } else {
            self.input_state.clear_motion();
        }

        self.send_pose();
    }

    fn send_pose(&mut self) {
        if self.stream.is_none() {
            match OpenOptions::new().write(true).open(PIPE_NAME) {
                Ok(stream) => {
                    self.stream = Some(stream);
                    self.status = "IPC connected".to_string();
                }
                Err(error) => {
                    self.status = format!("Waiting for driver pipe: {error}");
                    return;
                }
            }
        }

        let pose = self.pose_state.to_hmd_pose();
        if let Some(stream) = &mut self.stream {
            if let Err(error) = stream.write_all(pose.as_bytes()) {
                self.stream = None;
                self.status = format!("IPC disconnected: {error}");
            }
        }
    }
}

#[derive(Default)]
struct InputState {
    focused: bool,
    w: bool,
    s: bool,
    a: bool,
    d: bool,
    q: bool,
    e: bool,
    right_mouse_down: bool,
    reset_pending: bool,
    connect_toggle_pending: bool,
    has_last_mouse: bool,
    last_mouse: (i32, i32),
    mouse_delta: (i32, i32),
}

impl InputState {
    fn set_key(&mut self, key: VIRTUAL_KEY, pressed: bool) {
        match key {
            VK_W => self.w = pressed,
            VK_S => self.s = pressed,
            VK_A => self.a = pressed,
            VK_D => self.d = pressed,
            VK_Q => self.q = pressed,
            VK_E => self.e = pressed,
            VK_R if pressed => self.reset_pending = true,
            VK_C if pressed => self.connect_toggle_pending = true,
            _ => {}
        }
    }

    fn update_mouse_position(&mut self, x: i32, y: i32, right_mouse_down: bool) {
        if !self.focused || !right_mouse_down {
            self.right_mouse_down = false;
            self.has_last_mouse = false;
            self.last_mouse = (x, y);
            return;
        }

        if self.right_mouse_down && self.has_last_mouse {
            self.mouse_delta.0 += x - self.last_mouse.0;
            self.mouse_delta.1 += y - self.last_mouse.1;
        }

        self.right_mouse_down = true;
        self.last_mouse = (x, y);
        self.has_last_mouse = true;
    }

    fn take_mouse_delta(&mut self) -> (i32, i32) {
        let delta = self.mouse_delta;
        self.mouse_delta = (0, 0);
        delta
    }

    fn take_reset(&mut self) -> bool {
        let pending = self.reset_pending;
        self.reset_pending = false;
        pending
    }

    fn take_connect_toggle(&mut self) -> bool {
        let pending = self.connect_toggle_pending;
        self.connect_toggle_pending = false;
        pending
    }

    fn clear_focus_sensitive_state(&mut self) {
        self.focused = false;
        self.w = false;
        self.s = false;
        self.a = false;
        self.d = false;
        self.q = false;
        self.e = false;
        self.right_mouse_down = false;
        self.clear_motion();
    }

    fn clear_motion(&mut self) {
        self.has_last_mouse = false;
        self.mouse_delta = (0, 0);
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
    fn integrate(&mut self, input: &InputState, mouse_delta: (i32, i32), dt: f32) {
        self.yaw -= mouse_delta.0 as f32 * MOUSE_SENSITIVITY_RADIANS;
        self.pitch = (self.pitch - mouse_delta.1 as f32 * MOUSE_SENSITIVITY_RADIANS)
            .clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS);

        let rotation = self.rotation();
        let forward = rotation * Vector3::new(0.0, 0.0, -1.0);
        let right = rotation * Vector3::new(1.0, 0.0, 0.0);
        let up = Vector3::new(0.0, 1.0, 0.0);

        let forward_axis = axis(input.w, input.s);
        let strafe_axis = axis(input.d, input.a);
        let vertical_axis = axis(input.e, input.q);

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

fn axis(positive: bool, negative: bool) -> f32 {
    match (positive, negative) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        windows_sys::Win32::UI::WindowsAndMessaging::WM_CREATE => {
            let create_struct =
                lparam as *const windows_sys::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
            let app_ptr = (*create_struct).lpCreateParams as *mut ControllerApp;
            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                app_ptr as isize,
            );
            0
        }
        WM_SETFOCUS => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.input_state.focused = true;
                app.input_state.clear_motion();
                app.status = "Input active".to_string();
                InvalidateRect(hwnd, ptr::null(), 1);
            }
            0
        }
        WM_KILLFOCUS => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.input_state.clear_focus_sensitive_state();
                app.status = "Input paused: focus the controller window".to_string();
                InvalidateRect(hwnd, ptr::null(), 1);
            }
            0
        }
        WM_KEYDOWN => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.input_state.set_key(wparam as VIRTUAL_KEY, true);
            }
            0
        }
        WM_KEYUP => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.input_state.set_key(wparam as VIRTUAL_KEY, false);
            }
            0
        }
        WM_MOUSEMOVE => {
            if let Some(app) = app_from_hwnd(hwnd) {
                let x = loword(lparam as u32) as i16 as i32;
                let y = hiword(lparam as u32) as i16 as i32;
                app.input_state
                    .update_mouse_position(x, y, (wparam & MK_RBUTTON as usize) != 0);
            }
            0
        }
        WM_TIMER => {
            if wparam == TIMER_ID {
                if let Some(app) = app_from_hwnd(hwnd) {
                    app.tick();
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
                0
            } else {
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
        }
        WM_PAINT => {
            if let Some(app) = app_from_hwnd(hwnd) {
                paint_window(hwnd, app);
                0
            } else {
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
        }
        WM_DESTROY => {
            let app_ptr = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ) as *mut ControllerApp;
            if !app_ptr.is_null() {
                drop(Box::from_raw(app_ptr));
                windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                    hwnd,
                    windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                    0,
                );
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn app_from_hwnd(hwnd: HWND) -> Option<&'static mut ControllerApp> {
    let ptr = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
        hwnd,
        windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
    ) as *mut ControllerApp;
    ptr.as_mut()
}

unsafe fn paint_window(hwnd: HWND, app: &ControllerApp) {
    let mut paint: PAINTSTRUCT = mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut paint);
    let mut rect: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut rect);

    let lines = [
        "VRHeadsetEmulator Controller".to_string(),
        "Input only works while this window is focused.".to_string(),
        "W/S forward, A/D strafe, Q/E vertical, hold right mouse to rotate.".to_string(),
        "R reset pose, C toggle connected/disconnected.".to_string(),
        format!(
            "Position: x={:.2} y={:.2} z={:.2}",
            app.pose_state.position.x, app.pose_state.position.y, app.pose_state.position.z
        ),
        format!(
            "Yaw: {:.1} deg  Pitch: {:.1} deg",
            app.pose_state.yaw.to_degrees(),
            app.pose_state.pitch.to_degrees()
        ),
        format!(
            "HMD: {}",
            if app.pose_state.connected {
                "connected"
            } else {
                "disconnected"
            }
        ),
        format!("Status: {}", app.status),
    ];

    for (index, line) in lines.iter().enumerate() {
        let text = wide_null(line);
        TextOutW(
            hdc,
            18,
            18 + index as i32 * 24,
            text.as_ptr(),
            (text.len() - 1) as i32,
        );
    }

    EndPaint(hwnd, &paint);
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn loword(value: u32) -> u16 {
    (value & 0xffff) as u16
}

fn hiword(value: u32) -> u16 {
    ((value >> 16) & 0xffff) as u16
}
