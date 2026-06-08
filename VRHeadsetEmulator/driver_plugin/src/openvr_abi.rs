use std::ffi::{c_char, c_void};

pub const INVALID_TRACKED_DEVICE_INDEX: u32 = 0xFFFF_FFFF;

pub const SERVER_TRACKED_DEVICE_PROVIDER_VERSION: &[u8] = b"IServerTrackedDeviceProvider_004\0";
pub const TRACKED_DEVICE_SERVER_DRIVER_VERSION: &[u8] = b"ITrackedDeviceServerDriver_005\0";
pub const VR_DISPLAY_COMPONENT_VERSION: &[u8] = b"IVRDisplayComponent_003\0";
pub const VR_SERVER_DRIVER_HOST_VERSION: &[u8] = b"IVRServerDriverHost_006\0";
pub const VR_PROPERTIES_VERSION: &[u8] = b"IVRProperties_001\0";

pub const FLOAT_PROPERTY_TAG: PropertyTypeTag = 1;
pub const INT32_PROPERTY_TAG: PropertyTypeTag = 2;
pub const UINT64_PROPERTY_TAG: PropertyTypeTag = 3;
pub const BOOL_PROPERTY_TAG: PropertyTypeTag = 4;
pub const STRING_PROPERTY_TAG: PropertyTypeTag = 5;

pub type PropertyContainerHandle = u64;
pub type PropertyTypeTag = u32;

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EVRInitError {
    None = 0,
    InterfaceNotFound = 105,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ETrackedDeviceClass {
    Hmd = 1,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ETrackingResult {
    Uninitialized = 1,
    RunningOk = 200,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EVREye {
    Left = 0,
    Right = 1,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ETrackedDeviceProperty {
    TrackingSystemNameString = 1000,
    ModelNumberString = 1001,
    SerialNumberString = 1002,
    ManufacturerNameString = 1005,
    ContainsProximitySensorBool = 1025,
    DeviceClassInt32 = 1029,
    ResourceRootString = 1035,
    ReportsTimeSinceVSyncBool = 2000,
    SecondsFromVsyncToPhotonsFloat = 2001,
    DisplayFrequencyFloat = 2002,
    UserIpdMetersFloat = 2003,
    CurrentUniverseIdUint64 = 2004,
    IsOnDesktopBool = 2007,
    DisplayDebugModeBool = 2044,
    LensCenterLeftUFloat = 2022,
    LensCenterLeftVFloat = 2023,
    LensCenterRightUFloat = 2024,
    LensCenterRightVFloat = 2025,
    UserHeadToEyeDepthMetersFloat = 2026,
    ExpectedTrackingReferenceCountInt32 = 2049,
    ExpectedControllerCountInt32 = 2050,
    DoNotApplyPredictionBool = 2054,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ETrackedPropertyError {
    Success = 0,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EPropertyWriteType {
    Set = 0,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct HmdVector2 {
    pub v: [f32; 2],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct HmdRect2 {
    pub top_left: HmdVector2,
    pub bottom_right: HmdVector2,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct HmdQuaternion {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl HmdQuaternion {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct HmdMatrix34 {
    pub m: [[f32; 4]; 3],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DistortionCoordinates {
    pub red: [f32; 2],
    pub green: [f32; 2],
    pub blue: [f32; 2],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DriverPose {
    pub pose_time_offset: f64,
    pub world_from_driver_rotation: HmdQuaternion,
    pub world_from_driver_translation: [f64; 3],
    pub driver_from_head_rotation: HmdQuaternion,
    pub driver_from_head_translation: [f64; 3],
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub acceleration: [f64; 3],
    pub rotation: HmdQuaternion,
    pub angular_velocity: [f64; 3],
    pub angular_acceleration: [f64; 3],
    pub tracking_result: ETrackingResult,
    pub pose_is_valid: bool,
    pub will_drift_in_yaw: bool,
    pub should_apply_head_model: bool,
    pub device_is_connected: bool,
}

impl Default for DriverPose {
    fn default() -> Self {
        Self {
            pose_time_offset: 0.0,
            world_from_driver_rotation: HmdQuaternion::IDENTITY,
            world_from_driver_translation: [0.0; 3],
            driver_from_head_rotation: HmdQuaternion::IDENTITY,
            driver_from_head_translation: [0.0; 3],
            position: [0.0, 1.75, -0.5],
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            rotation: HmdQuaternion::IDENTITY,
            angular_velocity: [0.0; 3],
            angular_acceleration: [0.0; 3],
            tracking_result: ETrackingResult::Uninitialized,
            pose_is_valid: false,
            will_drift_in_yaw: false,
            should_apply_head_model: false,
            device_is_connected: false,
        }
    }
}

#[repr(C)]
pub struct PropertyWrite {
    pub prop: ETrackedDeviceProperty,
    pub write_type: EPropertyWriteType,
    pub set_error: ETrackedPropertyError,
    pub buffer: *mut c_void,
    pub buffer_size: u32,
    pub tag: PropertyTypeTag,
    pub error: ETrackedPropertyError,
}

#[repr(C)]
pub struct DriverContext {
    pub vtable: *const DriverContextVTable,
}

#[repr(C)]
pub struct DriverContextVTable {
    pub get_generic_interface:
        unsafe extern "C" fn(*mut DriverContext, *const c_char, *mut EVRInitError) -> *mut c_void,
    pub get_driver_handle: unsafe extern "C" fn(*mut DriverContext) -> PropertyContainerHandle,
}

#[repr(C)]
pub struct ServerDriverHost {
    pub vtable: *const ServerDriverHostVTable,
}

#[repr(C)]
pub struct ServerDriverHostVTable {
    pub tracked_device_added: unsafe extern "C" fn(
        *mut ServerDriverHost,
        *const c_char,
        ETrackedDeviceClass,
        *mut TrackedDeviceServerDriver,
    ) -> bool,
    pub tracked_device_pose_updated:
        unsafe extern "C" fn(*mut ServerDriverHost, u32, *const DriverPose, u32),
    pub vsync_event: unsafe extern "C" fn(*mut ServerDriverHost, f64),
    pub vendor_specific_event: usize,
    pub is_exiting: unsafe extern "C" fn(*mut ServerDriverHost) -> bool,
    pub poll_next_event: usize,
    pub get_raw_tracked_device_poses: usize,
    pub request_restart: usize,
    pub get_frame_timings: usize,
    pub set_display_eye_to_head:
        unsafe extern "C" fn(*mut ServerDriverHost, u32, *const HmdMatrix34, *const HmdMatrix34),
    pub set_display_projection_raw:
        unsafe extern "C" fn(*mut ServerDriverHost, u32, *const HmdRect2, *const HmdRect2),
    pub set_recommended_render_target_size:
        unsafe extern "C" fn(*mut ServerDriverHost, u32, u32, u32),
}

#[repr(C)]
pub struct VRProperties {
    pub vtable: *const VRPropertiesVTable,
}

#[repr(C)]
pub struct VRPropertiesVTable {
    pub read_property_batch: usize,
    pub write_property_batch: unsafe extern "C" fn(
        *mut VRProperties,
        PropertyContainerHandle,
        *mut PropertyWrite,
        u32,
    ) -> ETrackedPropertyError,
    pub get_prop_error_name_from_enum: usize,
    pub tracked_device_to_property_container:
        unsafe extern "C" fn(*mut VRProperties, u32) -> PropertyContainerHandle,
}

#[repr(C)]
pub struct ServerTrackedDeviceProvider {
    pub vtable: *const ServerTrackedDeviceProviderVTable,
}

#[repr(C)]
pub struct ServerTrackedDeviceProviderVTable {
    pub init: unsafe extern "C" fn(*mut ServerTrackedDeviceProvider, *mut DriverContext) -> i32,
    pub cleanup: unsafe extern "C" fn(*mut ServerTrackedDeviceProvider),
    pub get_interface_versions:
        unsafe extern "C" fn(*mut ServerTrackedDeviceProvider) -> *const *const c_char,
    pub run_frame: unsafe extern "C" fn(*mut ServerTrackedDeviceProvider),
    pub should_block_standby_mode: unsafe extern "C" fn(*mut ServerTrackedDeviceProvider) -> bool,
    pub enter_standby: unsafe extern "C" fn(*mut ServerTrackedDeviceProvider),
    pub leave_standby: unsafe extern "C" fn(*mut ServerTrackedDeviceProvider),
}

#[repr(C)]
pub struct TrackedDeviceServerDriver {
    pub vtable: *const TrackedDeviceServerDriverVTable,
}

#[repr(C)]
pub struct TrackedDeviceServerDriverVTable {
    pub activate: unsafe extern "C" fn(*mut TrackedDeviceServerDriver, u32) -> i32,
    pub deactivate: unsafe extern "C" fn(*mut TrackedDeviceServerDriver),
    pub enter_standby: unsafe extern "C" fn(*mut TrackedDeviceServerDriver),
    pub get_component:
        unsafe extern "C" fn(*mut TrackedDeviceServerDriver, *const c_char) -> *mut c_void,
    pub debug_request:
        unsafe extern "C" fn(*mut TrackedDeviceServerDriver, *const c_char, *mut c_char, u32),
    pub get_pose: unsafe extern "C" fn(*mut TrackedDeviceServerDriver) -> DriverPose,
}

#[repr(C)]
pub struct VRDisplayComponent {
    pub vtable: *const VRDisplayComponentVTable,
}

#[repr(C)]
pub struct VRDisplayComponentVTable {
    pub get_window_bounds:
        unsafe extern "C" fn(*mut VRDisplayComponent, *mut i32, *mut i32, *mut u32, *mut u32),
    pub is_display_on_desktop: unsafe extern "C" fn(*mut VRDisplayComponent) -> bool,
    pub is_display_real_display: unsafe extern "C" fn(*mut VRDisplayComponent) -> bool,
    pub get_recommended_render_target_size:
        unsafe extern "C" fn(*mut VRDisplayComponent, *mut u32, *mut u32),
    pub get_eye_output_viewport: unsafe extern "C" fn(
        *mut VRDisplayComponent,
        EVREye,
        *mut u32,
        *mut u32,
        *mut u32,
        *mut u32,
    ),
    pub get_projection_raw: unsafe extern "C" fn(
        *mut VRDisplayComponent,
        EVREye,
        *mut f32,
        *mut f32,
        *mut f32,
        *mut f32,
    ),
    pub compute_distortion:
        unsafe extern "C" fn(*mut VRDisplayComponent, EVREye, f32, f32) -> DistortionCoordinates,
    pub compute_inverse_distortion: unsafe extern "C" fn(
        *mut VRDisplayComponent,
        *mut HmdVector2,
        EVREye,
        u32,
        f32,
        f32,
    ) -> bool,
}

unsafe impl Sync for ServerTrackedDeviceProvider {}
unsafe impl Sync for TrackedDeviceServerDriver {}
unsafe impl Sync for VRDisplayComponent {}
