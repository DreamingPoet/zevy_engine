mod openvr_abi;

use std::ffi::{c_char, c_void, CStr};
use std::fs::OpenOptions;
use std::io::Write;
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering},
    Arc, Mutex, OnceLock,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmd_protocol::{HmdPoseData, FRAME_SIZE, PIPE_NAME};
use openvr_abi::*;

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND},
    System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
        PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    },
};

const DEVICE_SERIAL: &[u8; 26] = b"VRHeadsetEmulator_HMD_001\0";
const DISPLAY_WIDTH: u32 = 2160;
const DISPLAY_HEIGHT: u32 = 1200;
const EYE_WIDTH: u32 = DISPLAY_WIDTH / 2;
const EYE_HEIGHT: u32 = DISPLAY_HEIGHT;
const RECOMMENDED_TARGET_WIDTH: u32 = 1512;
const RECOMMENDED_TARGET_HEIGHT: u32 = 1680;

static DRIVER_STATE: OnceLock<Arc<DriverState>> = OnceLock::new();

static PROVIDER: ServerTrackedDeviceProvider = ServerTrackedDeviceProvider {
    vtable: &PROVIDER_VTABLE,
};
static PROVIDER_VTABLE: ServerTrackedDeviceProviderVTable = ServerTrackedDeviceProviderVTable {
    init: provider_init,
    cleanup: provider_cleanup,
    get_interface_versions: provider_get_interface_versions,
    run_frame: provider_run_frame,
    should_block_standby_mode: provider_should_block_standby_mode,
    enter_standby: provider_enter_standby,
    leave_standby: provider_leave_standby,
};

static HMD_DEVICE: VirtualHmdDevice = VirtualHmdDevice {
    base: TrackedDeviceServerDriver {
        vtable: &HMD_DEVICE_VTABLE,
    },
    active_object_id: AtomicU32::new(INVALID_TRACKED_DEVICE_INDEX),
    is_active: AtomicBool::new(false),
};
static HMD_DEVICE_VTABLE: TrackedDeviceServerDriverVTable = TrackedDeviceServerDriverVTable {
    activate: hmd_activate,
    deactivate: hmd_deactivate,
    enter_standby: hmd_enter_standby,
    get_component: hmd_get_component,
    debug_request: hmd_debug_request,
    get_pose: hmd_get_pose,
};

static DISPLAY_COMPONENT: VRDisplayComponent = VRDisplayComponent {
    vtable: &DISPLAY_COMPONENT_VTABLE,
};
static DISPLAY_COMPONENT_VTABLE: VRDisplayComponentVTable = VRDisplayComponentVTable {
    get_window_bounds: display_get_window_bounds,
    is_display_on_desktop: display_is_display_on_desktop,
    is_display_real_display: display_is_display_real_display,
    get_recommended_render_target_size: display_get_recommended_render_target_size,
    get_eye_output_viewport: display_get_eye_output_viewport,
    get_projection_raw: display_get_projection_raw,
    compute_distortion: display_compute_distortion,
    compute_inverse_distortion: display_compute_inverse_distortion,
};
static LOGGED_DISPLAY_BOUNDS: AtomicBool = AtomicBool::new(false);
static LOGGED_RENDER_TARGET_SIZE: AtomicBool = AtomicBool::new(false);
static LOGGED_PROJECTION_RAW: AtomicBool = AtomicBool::new(false);
static LOGGED_DISTORTION: AtomicBool = AtomicBool::new(false);
static LOGGED_INVERSE_DISTORTION: AtomicBool = AtomicBool::new(false);

#[repr(C)]
struct VirtualHmdDevice {
    base: TrackedDeviceServerDriver,
    active_object_id: AtomicU32,
    is_active: AtomicBool,
}

unsafe impl Sync for VirtualHmdDevice {}

struct DriverState {
    latest_pose: Mutex<HmdPoseData>,
    running: AtomicBool,
    pipe_thread_started: AtomicBool,
    driver_context: AtomicPtr<DriverContext>,
    server_host: AtomicPtr<ServerDriverHost>,
    properties: AtomicPtr<VRProperties>,
}

impl DriverState {
    fn global() -> Arc<Self> {
        DRIVER_STATE
            .get_or_init(|| {
                Arc::new(Self {
                    latest_pose: Mutex::new(HmdPoseData::default()),
                    running: AtomicBool::new(false),
                    pipe_thread_started: AtomicBool::new(false),
                    driver_context: AtomicPtr::new(ptr::null_mut()),
                    server_host: AtomicPtr::new(ptr::null_mut()),
                    properties: AtomicPtr::new(ptr::null_mut()),
                })
            })
            .clone()
    }

    fn start_pipe_server_once(self: &Arc<Self>) {
        self.running.store(true, Ordering::Release);
        if !self.pipe_thread_started.swap(true, Ordering::AcqRel) {
            let pipe_state = self.clone();
            thread::spawn(move || pipe_server_loop(pipe_state));
            log_event("pipe server thread started");
        }
    }

    fn current_driver_pose(&self) -> DriverPose {
        let pose = *self.latest_pose.lock().expect("pose mutex poisoned");
        let connected = pose.is_connected();
        DriverPose {
            position: [
                pose.position[0] as f64,
                pose.position[1] as f64,
                pose.position[2] as f64,
            ],
            rotation: HmdQuaternion {
                x: pose.orientation[0] as f64,
                y: pose.orientation[1] as f64,
                z: pose.orientation[2] as f64,
                w: pose.orientation[3] as f64,
            },
            tracking_result: if connected {
                ETrackingResult::RunningOk
            } else {
                ETrackingResult::Uninitialized
            },
            pose_is_valid: connected,
            should_apply_head_model: true,
            device_is_connected: connected,
            ..Default::default()
        }
    }

    unsafe fn resolve_openvr_interfaces(&self, context: *mut DriverContext) -> EVRInitError {
        if context.is_null() || (*context).vtable.is_null() {
            log_event("provider init failed: null IVRDriverContext or vtable");
            return EVRInitError::InterfaceNotFound;
        }

        self.driver_context.store(context, Ordering::Release);

        let mut host_error = EVRInitError::None;
        let host = ((*(*context).vtable).get_generic_interface)(
            context,
            VR_SERVER_DRIVER_HOST_VERSION.as_ptr().cast(),
            &mut host_error,
        )
        .cast::<ServerDriverHost>();

        let mut properties_error = EVRInitError::None;
        let properties = ((*(*context).vtable).get_generic_interface)(
            context,
            VR_PROPERTIES_VERSION.as_ptr().cast(),
            &mut properties_error,
        )
        .cast::<VRProperties>();

        if host.is_null()
            || properties.is_null()
            || host_error != EVRInitError::None
            || properties_error != EVRInitError::None
        {
            log_event(&format!(
                "provider init failed: interface lookup host_ptr={host:p} host_error={host_error:?} properties_ptr={properties:p} properties_error={properties_error:?}"
            ));
            return EVRInitError::InterfaceNotFound;
        }

        log_event("provider init resolved IVRServerDriverHost and IVRProperties");
        self.server_host.store(host, Ordering::Release);
        self.properties.store(properties, Ordering::Release);
        EVRInitError::None
    }

    unsafe fn register_hmd(&self) -> bool {
        let host = self.server_host.load(Ordering::Acquire);
        if host.is_null() || (*host).vtable.is_null() {
            log_event("tracked device registration skipped: null host");
            return false;
        }

        let registered = ((*(*host).vtable).tracked_device_added)(
            host,
            DEVICE_SERIAL.as_ptr().cast(),
            ETrackedDeviceClass::Hmd,
            (&HMD_DEVICE.base as *const TrackedDeviceServerDriver)
                .cast_mut()
                .cast(),
        );
        log_event(&format!(
            "tracked device registration returned {registered}"
        ));
        registered
    }

    unsafe fn push_pose_to_steamvr(&self) {
        let host = self.server_host.load(Ordering::Acquire);
        let object_id = HMD_DEVICE.active_object_id.load(Ordering::Acquire);
        if host.is_null()
            || (*host).vtable.is_null()
            || object_id == INVALID_TRACKED_DEVICE_INDEX
            || !HMD_DEVICE.is_active.load(Ordering::Acquire)
        {
            return;
        }

        let pose = self.current_driver_pose();
        ((*(*host).vtable).tracked_device_pose_updated)(
            host,
            object_id,
            &pose,
            std::mem::size_of::<DriverPose>() as u32,
        );
    }
}

#[no_mangle]
pub unsafe extern "C" fn HmdDriverFactory(
    interface_name: *const c_char,
    return_code: *mut i32,
) -> *mut c_void {
    if interface_name.is_null() {
        log_event("HmdDriverFactory called with null interface name");
        write_return_code(return_code, EVRInitError::InterfaceNotFound);
        return ptr::null_mut();
    }

    let interface_name = CStr::from_ptr(interface_name).to_string_lossy();
    log_event(&format!("HmdDriverFactory requested {interface_name}"));
    if interface_name.starts_with("IServerTrackedDeviceProvider_") {
        DriverState::global().start_pipe_server_once();
        write_return_code(return_code, EVRInitError::None);
        log_event("HmdDriverFactory returned provider");
        return (&PROVIDER as *const ServerTrackedDeviceProvider)
            .cast_mut()
            .cast();
    }

    log_event(&format!(
        "HmdDriverFactory rejected unsupported interface {interface_name}"
    ));
    write_return_code(return_code, EVRInitError::InterfaceNotFound);
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn VirtualHmdGetLatestPoseForTest() -> DriverPose {
    DriverState::global().current_driver_pose()
}

unsafe fn write_return_code(return_code: *mut i32, code: EVRInitError) {
    if !return_code.is_null() {
        *return_code = code as i32;
    }
}

unsafe extern "C" fn provider_init(
    _this: *mut ServerTrackedDeviceProvider,
    driver_context: *mut DriverContext,
) -> i32 {
    log_event("provider init entered");
    let state = DriverState::global();
    state.start_pipe_server_once();

    let init_error = state.resolve_openvr_interfaces(driver_context);
    if init_error != EVRInitError::None {
        log_event(&format!("provider init returning {init_error:?}"));
        return init_error as i32;
    }

    if !state.register_hmd() {
        log_event("provider init failed: TrackedDeviceAdded returned false");
        return EVRInitError::InterfaceNotFound as i32;
    }

    log_event("provider init completed");
    EVRInitError::None as i32
}

unsafe extern "C" fn provider_cleanup(_this: *mut ServerTrackedDeviceProvider) {
    log_event("provider cleanup entered");
    let state = DriverState::global();
    state.running.store(false, Ordering::Release);
    state.pipe_thread_started.store(false, Ordering::Release);
    state
        .driver_context
        .store(ptr::null_mut(), Ordering::Release);
    state.server_host.store(ptr::null_mut(), Ordering::Release);
    state.properties.store(ptr::null_mut(), Ordering::Release);
    HMD_DEVICE
        .active_object_id
        .store(INVALID_TRACKED_DEVICE_INDEX, Ordering::Release);
    HMD_DEVICE.is_active.store(false, Ordering::Release);
}

unsafe extern "C" fn provider_get_interface_versions(
    _this: *mut ServerTrackedDeviceProvider,
) -> *const *const c_char {
    static INTERFACE_VERSIONS: InterfaceVersions = InterfaceVersions([
        SERVER_TRACKED_DEVICE_PROVIDER_VERSION
            .as_ptr()
            .cast::<c_char>(),
        TRACKED_DEVICE_SERVER_DRIVER_VERSION
            .as_ptr()
            .cast::<c_char>(),
        VR_DISPLAY_COMPONENT_VERSION.as_ptr().cast::<c_char>(),
        ptr::null(),
    ]);

    INTERFACE_VERSIONS.0.as_ptr()
}

struct InterfaceVersions([*const c_char; 4]);

unsafe impl Sync for InterfaceVersions {}

unsafe extern "C" fn provider_run_frame(_this: *mut ServerTrackedDeviceProvider) {
    DriverState::global().push_pose_to_steamvr();
}

unsafe extern "C" fn provider_should_block_standby_mode(
    _this: *mut ServerTrackedDeviceProvider,
) -> bool {
    false
}

unsafe extern "C" fn provider_enter_standby(_this: *mut ServerTrackedDeviceProvider) {}

unsafe extern "C" fn provider_leave_standby(_this: *mut ServerTrackedDeviceProvider) {}

unsafe extern "C" fn hmd_activate(_this: *mut TrackedDeviceServerDriver, object_id: u32) -> i32 {
    log_event(&format!("hmd activate object_id={object_id}"));
    HMD_DEVICE
        .active_object_id
        .store(object_id, Ordering::Release);
    HMD_DEVICE.is_active.store(true, Ordering::Release);

    set_hmd_properties(object_id);

    EVRInitError::None as i32
}

unsafe extern "C" fn hmd_deactivate(_this: *mut TrackedDeviceServerDriver) {
    log_event("hmd deactivate");
    HMD_DEVICE.is_active.store(false, Ordering::Release);
    HMD_DEVICE
        .active_object_id
        .store(INVALID_TRACKED_DEVICE_INDEX, Ordering::Release);
}

unsafe extern "C" fn hmd_enter_standby(_this: *mut TrackedDeviceServerDriver) {}

unsafe extern "C" fn hmd_get_component(
    _this: *mut TrackedDeviceServerDriver,
    component_name_and_version: *const c_char,
) -> *mut c_void {
    if component_name_and_version.is_null() {
        return ptr::null_mut();
    }

    let component = CStr::from_ptr(component_name_and_version).to_bytes();
    if component == b"IVRDisplayComponent_002" || component == b"IVRDisplayComponent_003" {
        log_event("hmd get_component returned IVRDisplayComponent");
        return (&DISPLAY_COMPONENT as *const VRDisplayComponent)
            .cast_mut()
            .cast();
    }

    log_event(&format!(
        "hmd get_component rejected {}",
        String::from_utf8_lossy(component)
    ));
    ptr::null_mut()
}

unsafe extern "C" fn hmd_debug_request(
    _this: *mut TrackedDeviceServerDriver,
    _request: *const c_char,
    response_buffer: *mut c_char,
    response_buffer_size: u32,
) {
    copy_debug_response(
        response_buffer,
        response_buffer_size,
        b"VRHeadsetEmulator OK\0",
    );
}

unsafe extern "C" fn hmd_get_pose(_this: *mut TrackedDeviceServerDriver) -> DriverPose {
    DriverState::global().current_driver_pose()
}

unsafe fn set_hmd_properties(object_id: u32) {
    let state = DriverState::global();
    let properties = state.properties.load(Ordering::Acquire);
    if properties.is_null() || (*properties).vtable.is_null() {
        log_event("set_hmd_properties skipped: null IVRProperties");
        return;
    }

    let container =
        ((*(*properties).vtable).tracked_device_to_property_container)(properties, object_id);

    let mut tracking_system = *b"VRHeadsetEmulator\0";
    let mut model_number = *b"Virtual Rust HMD\0";
    let mut manufacturer = *b"Zevy Engine\0";
    let mut serial = *DEVICE_SERIAL;
    let mut resource_root = *b"virtual_hmd\0";
    let mut device_class = ETrackedDeviceClass::Hmd as i32;
    let mut contains_proximity = true;
    let mut reports_vsync = false;
    let mut seconds_to_photons = 0.11_f32;
    let mut display_frequency = 0.0_f32;
    let mut ipd = 0.063_f32;
    let mut universe_id = 1_u64;
    let mut is_on_desktop = true;
    let mut display_debug_mode = true;
    let mut lens_left_u = 0.5_f32;
    let mut lens_left_v = 0.5_f32;
    let mut lens_right_u = 0.5_f32;
    let mut lens_right_v = 0.5_f32;
    let mut head_to_eye_depth = 0.0_f32;
    let mut tracking_reference_count = 0_i32;
    let mut controller_count = 0_i32;
    let mut do_not_predict = true;

    let mut writes = [
        property_string(
            ETrackedDeviceProperty::TrackingSystemNameString,
            &mut tracking_system,
        ),
        property_string(ETrackedDeviceProperty::ModelNumberString, &mut model_number),
        property_string(ETrackedDeviceProperty::SerialNumberString, &mut serial),
        property_string(
            ETrackedDeviceProperty::ManufacturerNameString,
            &mut manufacturer,
        ),
        property_string(
            ETrackedDeviceProperty::ResourceRootString,
            &mut resource_root,
        ),
        property_i32(ETrackedDeviceProperty::DeviceClassInt32, &mut device_class),
        property_bool(
            ETrackedDeviceProperty::ContainsProximitySensorBool,
            &mut contains_proximity,
        ),
        property_bool(
            ETrackedDeviceProperty::ReportsTimeSinceVSyncBool,
            &mut reports_vsync,
        ),
        property_f32(
            ETrackedDeviceProperty::SecondsFromVsyncToPhotonsFloat,
            &mut seconds_to_photons,
        ),
        property_f32(
            ETrackedDeviceProperty::DisplayFrequencyFloat,
            &mut display_frequency,
        ),
        property_f32(ETrackedDeviceProperty::UserIpdMetersFloat, &mut ipd),
        property_u64(
            ETrackedDeviceProperty::CurrentUniverseIdUint64,
            &mut universe_id,
        ),
        property_bool(ETrackedDeviceProperty::IsOnDesktopBool, &mut is_on_desktop),
        property_bool(
            ETrackedDeviceProperty::DisplayDebugModeBool,
            &mut display_debug_mode,
        ),
        property_f32(
            ETrackedDeviceProperty::LensCenterLeftUFloat,
            &mut lens_left_u,
        ),
        property_f32(
            ETrackedDeviceProperty::LensCenterLeftVFloat,
            &mut lens_left_v,
        ),
        property_f32(
            ETrackedDeviceProperty::LensCenterRightUFloat,
            &mut lens_right_u,
        ),
        property_f32(
            ETrackedDeviceProperty::LensCenterRightVFloat,
            &mut lens_right_v,
        ),
        property_f32(
            ETrackedDeviceProperty::UserHeadToEyeDepthMetersFloat,
            &mut head_to_eye_depth,
        ),
        property_i32(
            ETrackedDeviceProperty::ExpectedTrackingReferenceCountInt32,
            &mut tracking_reference_count,
        ),
        property_i32(
            ETrackedDeviceProperty::ExpectedControllerCountInt32,
            &mut controller_count,
        ),
        property_bool(
            ETrackedDeviceProperty::DoNotApplyPredictionBool,
            &mut do_not_predict,
        ),
    ];

    let result = ((*(*properties).vtable).write_property_batch)(
        properties,
        container,
        writes.as_mut_ptr(),
        writes.len() as u32,
    );
    log_event(&format!("set_hmd_properties result={result:?}"));
}

#[allow(dead_code)]
unsafe fn push_display_metadata(object_id: u32) {
    let state = DriverState::global();
    let host = state.server_host.load(Ordering::Acquire);
    if host.is_null() || (*host).vtable.is_null() {
        log_event("push_display_metadata skipped: null host");
        return;
    }

    let left = HmdMatrix34 {
        m: [
            [1.0, 0.0, 0.0, -0.0315],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
    };
    let right = HmdMatrix34 {
        m: [
            [1.0, 0.0, 0.0, 0.0315],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
    };
    ((*(*host).vtable).set_display_eye_to_head)(host, object_id, &left, &right);

    let projection = HmdRect2 {
        top_left: HmdVector2 { v: [-1.0, -1.0] },
        bottom_right: HmdVector2 { v: [1.0, 1.0] },
    };
    ((*(*host).vtable).set_display_projection_raw)(host, object_id, &projection, &projection);
    ((*(*host).vtable).set_recommended_render_target_size)(
        host,
        object_id,
        RECOMMENDED_TARGET_WIDTH,
        RECOMMENDED_TARGET_HEIGHT,
    );
}

fn property_string<const N: usize>(
    prop: ETrackedDeviceProperty,
    value: &mut [u8; N],
) -> PropertyWrite {
    property_write(
        prop,
        value.as_mut_ptr().cast(),
        N as u32,
        STRING_PROPERTY_TAG,
    )
}

fn property_bool(prop: ETrackedDeviceProperty, value: &mut bool) -> PropertyWrite {
    property_write(
        prop,
        (value as *mut bool).cast(),
        std::mem::size_of::<bool>() as u32,
        BOOL_PROPERTY_TAG,
    )
}

fn property_f32(prop: ETrackedDeviceProperty, value: &mut f32) -> PropertyWrite {
    property_write(
        prop,
        (value as *mut f32).cast(),
        std::mem::size_of::<f32>() as u32,
        FLOAT_PROPERTY_TAG,
    )
}

fn property_i32(prop: ETrackedDeviceProperty, value: &mut i32) -> PropertyWrite {
    property_write(
        prop,
        (value as *mut i32).cast(),
        std::mem::size_of::<i32>() as u32,
        INT32_PROPERTY_TAG,
    )
}

fn property_u64(prop: ETrackedDeviceProperty, value: &mut u64) -> PropertyWrite {
    property_write(
        prop,
        (value as *mut u64).cast(),
        std::mem::size_of::<u64>() as u32,
        UINT64_PROPERTY_TAG,
    )
}

fn property_write(
    prop: ETrackedDeviceProperty,
    buffer: *mut c_void,
    buffer_size: u32,
    tag: PropertyTypeTag,
) -> PropertyWrite {
    PropertyWrite {
        prop,
        write_type: EPropertyWriteType::Set,
        set_error: ETrackedPropertyError::Success,
        buffer,
        buffer_size,
        tag,
        error: ETrackedPropertyError::Success,
    }
}

unsafe fn copy_debug_response(
    response_buffer: *mut c_char,
    response_buffer_size: u32,
    response: &[u8],
) {
    if response_buffer.is_null() || response_buffer_size == 0 {
        return;
    }

    let bytes_to_copy = response.len().min(response_buffer_size as usize);
    ptr::copy_nonoverlapping(response.as_ptr().cast(), response_buffer, bytes_to_copy);
    *response_buffer.add(response_buffer_size as usize - 1) = 0;
}

unsafe extern "C" fn display_get_window_bounds(
    _this: *mut VRDisplayComponent,
    x: *mut i32,
    y: *mut i32,
    width: *mut u32,
    height: *mut u32,
) {
    log_once(&LOGGED_DISPLAY_BOUNDS, "display get_window_bounds");
    write_if_present(x, 0);
    write_if_present(y, 0);
    write_if_present(width, DISPLAY_WIDTH);
    write_if_present(height, DISPLAY_HEIGHT);
}

unsafe extern "C" fn display_is_display_on_desktop(_this: *mut VRDisplayComponent) -> bool {
    true
}

unsafe extern "C" fn display_is_display_real_display(_this: *mut VRDisplayComponent) -> bool {
    false
}

unsafe extern "C" fn display_get_recommended_render_target_size(
    _this: *mut VRDisplayComponent,
    width: *mut u32,
    height: *mut u32,
) {
    log_once(
        &LOGGED_RENDER_TARGET_SIZE,
        "display get_recommended_render_target_size",
    );
    write_if_present(width, RECOMMENDED_TARGET_WIDTH);
    write_if_present(height, RECOMMENDED_TARGET_HEIGHT);
}

unsafe extern "C" fn display_get_eye_output_viewport(
    _this: *mut VRDisplayComponent,
    eye: EVREye,
    x: *mut u32,
    y: *mut u32,
    width: *mut u32,
    height: *mut u32,
) {
    let left_x = match eye {
        EVREye::Left => 0,
        EVREye::Right => EYE_WIDTH,
    };
    write_if_present(x, left_x);
    write_if_present(y, 0);
    write_if_present(width, EYE_WIDTH);
    write_if_present(height, EYE_HEIGHT);
}

unsafe extern "C" fn display_get_projection_raw(
    _this: *mut VRDisplayComponent,
    _eye: EVREye,
    left: *mut f32,
    right: *mut f32,
    top: *mut f32,
    bottom: *mut f32,
) {
    log_once(&LOGGED_PROJECTION_RAW, "display get_projection_raw");
    write_if_present(left, -1.0);
    write_if_present(right, 1.0);
    write_if_present(top, -1.0);
    write_if_present(bottom, 1.0);
}

unsafe extern "C" fn display_compute_distortion(
    _this: *mut VRDisplayComponent,
    _eye: EVREye,
    u: f32,
    v: f32,
) -> DistortionCoordinates {
    log_once(&LOGGED_DISTORTION, "display compute_distortion");
    DistortionCoordinates {
        red: [u, v],
        green: [u, v],
        blue: [u, v],
    }
}

unsafe extern "C" fn display_compute_inverse_distortion(
    _this: *mut VRDisplayComponent,
    result: *mut HmdVector2,
    _eye: EVREye,
    _channel: u32,
    u: f32,
    v: f32,
) -> bool {
    log_once(
        &LOGGED_INVERSE_DISTORTION,
        "display compute_inverse_distortion",
    );
    if !result.is_null() {
        *result = HmdVector2 { v: [u, v] };
    }
    true
}

fn log_once(flag: &AtomicBool, message: &str) {
    if !flag.swap(true, Ordering::AcqRel) {
        log_event(message);
    }
}

unsafe fn write_if_present<T>(target: *mut T, value: T) {
    if !target.is_null() {
        *target = value;
    }
}

fn pipe_server_loop(state: Arc<DriverState>) {
    log_event("pipe server loop entered");
    while state.running.load(Ordering::Acquire) {
        run_one_pipe_session(&state);
        thread::sleep(Duration::from_millis(20));
    }
    log_event("pipe server loop exited");
}

#[cfg(windows)]
fn run_one_pipe_session(state: &DriverState) {
    let pipe_name = wide_null(PIPE_NAME);

    unsafe {
        let pipe = CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_INBOUND,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            FRAME_SIZE as u32,
            FRAME_SIZE as u32,
            0,
            ptr::null_mut(),
        );

        if pipe == INVALID_HANDLE_VALUE {
            log_event(&format!("CreateNamedPipeW failed: {}", GetLastError()));
            thread::sleep(Duration::from_millis(250));
            return;
        }

        log_event(&format!("pipe waiting for controller at {PIPE_NAME}"));
        let connected =
            ConnectNamedPipe(pipe, ptr::null_mut()) != 0 || GetLastError() == ERROR_PIPE_CONNECTED;

        if connected {
            log_event("pipe controller connected");
            read_pose_frames(pipe, state);
        } else {
            log_event(&format!("ConnectNamedPipe failed: {}", GetLastError()));
        }

        log_event("pipe session closed");
        DisconnectNamedPipe(pipe);
        CloseHandle(pipe);
    }
}

#[cfg(windows)]
unsafe fn read_pose_frames(pipe: isize, state: &DriverState) {
    let mut buffer = [0_u8; FRAME_SIZE];
    while state.running.load(Ordering::Acquire) {
        let mut bytes_read = 0_u32;
        let ok = ReadFile(
            pipe,
            buffer.as_mut_ptr().cast(),
            FRAME_SIZE as u32,
            &mut bytes_read,
            ptr::null_mut(),
        );

        if ok == 0 || bytes_read != FRAME_SIZE as u32 {
            log_event(&format!(
                "ReadFile ended ok={ok} bytes_read={bytes_read} error={}",
                GetLastError()
            ));
            break;
        }

        let pose = HmdPoseData::from_bytes(&buffer);
        *state.latest_pose.lock().expect("pose mutex poisoned") = pose;
        state.push_pose_to_steamvr();
    }
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(not(windows))]
fn run_one_pipe_session(_state: &DriverState) {
    thread::sleep(Duration::from_millis(250));
}

fn log_event(message: &str) {
    let Ok(mut path) = std::env::var("TEMP").or_else(|_| std::env::var("TMP")) else {
        return;
    };
    path.push_str("\\VRHeadsetEmulator_driver.log");

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default();

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{timestamp:.3} {message}");
    }
}
