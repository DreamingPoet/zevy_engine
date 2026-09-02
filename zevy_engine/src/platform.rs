#[cfg(target_os = "android")]
use std::time::Duration;

#[cfg(target_os = "android")]
use bevy::{prelude::*, window::ExitCondition};
#[cfg(all(target_os = "android", feature = "render_debug"))]
use bevy_mod_openxr::exts::OxrAvailableExtensions;
#[cfg(not(target_os = "android"))]
use bevy_mod_openxr::init::OxrInitPlugin;
#[cfg(target_os = "android")]
use bevy_mod_openxr::{exts::OxrEnabledExtensions, init::OxrInitPlugin};

#[cfg(target_os = "android")]
pub fn android_main(android_app: bevy::window::android_activity::AndroidApp) {
    let vm = android_app.vm_as_ptr();
    let activity = android_app.activity_as_ptr();

    eprintln!("Android OpenXR context bridge: android_main vm={vm:p} activity={activity:p}");

    unsafe {
        ndk_context::release_android_context();
        ndk_context::initialize_android_context(vm, activity);
    }

    let context = ndk_context::android_context();
    eprintln!(
        "Android OpenXR context bridge: ndk_context vm={:p} context={:p}",
        context.vm(),
        context.context()
    );

    ack_android_activity_startup(&android_app);

    let _ = bevy::window::ANDROID_APP.set(android_app);
    crate::app::main();
}

#[cfg(target_os = "android")]
pub fn log_android_context(stage: &str) {
    let context = ndk_context::android_context();
    eprintln!(
        "Android OpenXR context bridge: {stage} vm={:p} context={:p}",
        context.vm(),
        context.context()
    );
}

#[cfg(not(target_os = "android"))]
pub fn log_android_context(_stage: &str) {}

/// Reads one Android system property without depending on Java or an Activity
/// lifecycle callback. Profiling-only callers use this for deterministic A/B
/// selection before Bevy resources and render pipelines are created.
#[cfg(all(target_os = "android", feature = "render_debug"))]
pub(crate) fn android_system_property(name: &str) -> Option<String> {
    use std::{
        ffi::{CStr, CString},
        os::raw::c_char,
    };

    const PROPERTY_VALUE_MAX: usize = 92;
    unsafe extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> i32;
    }

    let name = CString::new(name).ok()?;
    let mut value = [0 as c_char; PROPERTY_VALUE_MAX];
    // SAFETY: `name` is NUL-terminated and `value` is the Bionic-documented
    // PROPERTY_VALUE_MAX-sized writable output buffer.
    let length = unsafe { __system_property_get(name.as_ptr(), value.as_mut_ptr()) };
    if length <= 0 {
        return None;
    }
    // SAFETY: a successful __system_property_get call NUL-terminates `value`.
    Some(
        unsafe { CStr::from_ptr(value.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(target_os = "android")]
fn ack_android_activity_startup(android_app: &bevy::window::android_activity::AndroidApp) {
    use bevy::window::android_activity::{MainEvent, PollEvent};

    let mut saw_resume = false;
    let mut has_window = android_app.native_window().is_some();

    for _ in 0..100 {
        android_app.poll_events(Some(Duration::from_millis(10)), |event| {
            if let PollEvent::Main(main_event) = event {
                match main_event {
                    MainEvent::Start => eprintln!("Android lifecycle ack before XR init: Start"),
                    MainEvent::Resume { .. } => {
                        saw_resume = true;
                        eprintln!("Android lifecycle ack before XR init: Resume");
                    }
                    MainEvent::InitWindow { .. } => {
                        has_window = true;
                        eprintln!("Android lifecycle ack before XR init: InitWindow");
                    }
                    MainEvent::TerminateWindow { .. } => {
                        has_window = false;
                        eprintln!("Android lifecycle ack before XR init: TerminateWindow");
                    }
                    MainEvent::GainedFocus => {
                        eprintln!("Android lifecycle ack before XR init: GainedFocus")
                    }
                    MainEvent::LostFocus => {
                        eprintln!("Android lifecycle ack before XR init: LostFocus")
                    }
                    _ => {}
                }
            }
        });

        has_window |= android_app.native_window().is_some();
        if saw_resume && has_window {
            break;
        }
    }
}

#[cfg(target_os = "android")]
pub fn poll_android_activity_events(mut exit: EventWriter<AppExit>) {
    use bevy::window::android_activity::{MainEvent, PollEvent};

    let Some(android_app) = bevy::window::ANDROID_APP.get() else {
        return;
    };

    android_app.poll_events(Some(Duration::ZERO), |event| {
        if let PollEvent::Main(main_event) = event {
            match main_event {
                MainEvent::Destroy => {
                    info!("Android lifecycle event: Destroy");
                    exit.write(AppExit::Success);
                }
                MainEvent::Start => info!("Android lifecycle event: Start"),
                MainEvent::Resume { .. } => info!("Android lifecycle event: Resume"),
                MainEvent::Pause => info!("Android lifecycle event: Pause"),
                MainEvent::Stop => info!("Android lifecycle event: Stop"),
                MainEvent::InitWindow { .. } => info!("Android lifecycle event: InitWindow"),
                MainEvent::TerminateWindow { .. } => {
                    info!("Android lifecycle event: TerminateWindow")
                }
                MainEvent::GainedFocus => info!("Android lifecycle event: GainedFocus"),
                MainEvent::LostFocus => info!("Android lifecycle event: LostFocus"),
                _ => {}
            }
        }
    });
}

#[cfg(not(target_os = "android"))]
pub fn poll_android_activity_events() {}

#[cfg(target_os = "android")]
#[derive(Resource, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AndroidDisplayRefreshConfigured;

#[cfg(target_os = "android")]
pub fn configure_android_display_refresh_rate(
    session: Option<Res<bevy_mod_openxr::session::OxrSession>>,
    enabled_exts: Option<Res<OxrEnabledExtensions>>,
    configured: Option<Res<AndroidDisplayRefreshConfigured>>,
    mut commands: Commands,
) {
    if configured.is_some() {
        return;
    }

    let Some(session) = session else {
        return;
    };

    if !enabled_exts.is_some_and(|exts| exts.raw().fb_display_refresh_rate) {
        warn!("XR_FB_display_refresh_rate is not enabled; continuing without refresh-rate request");
        commands.insert_resource(AndroidDisplayRefreshConfigured);
        return;
    }

    let rates = match session.enumerate_display_refresh_rates() {
        Ok(rates) => rates,
        Err(err) => {
            warn!("Failed to enumerate OpenXR display refresh rates: {err}");
            commands.insert_resource(AndroidDisplayRefreshConfigured);
            return;
        }
    };
    info!("OpenXR display refresh rates available: {:?}", rates);

    match session.get_display_refresh_rate() {
        Ok(rate) => info!("OpenXR current display refresh rate before request: {rate}"),
        Err(err) => warn!("Failed to get OpenXR display refresh rate before request: {err}"),
    }

    let target_rate = rates
        .iter()
        .copied()
        .find(|rate| (*rate - 90.0).abs() < 0.01)
        .or_else(|| {
            rates
                .iter()
                .copied()
                .find(|rate| (*rate - 72.0).abs() < 0.01)
        })
        .or_else(|| rates.first().copied());

    match target_rate {
        Some(rate) => match session.request_display_refresh_rate(rate) {
            Ok(()) => info!("OpenXR requested display refresh rate: {rate}"),
            Err(err) => warn!("Failed to request OpenXR display refresh rate {rate}: {err}"),
        },
        None => warn!("OpenXR runtime reported no display refresh rates"),
    }

    match session.get_display_refresh_rate() {
        Ok(rate) => info!("OpenXR current display refresh rate after request: {rate}"),
        Err(err) => warn!("Failed to get OpenXR display refresh rate after request: {err}"),
    }

    commands.insert_resource(AndroidDisplayRefreshConfigured);
}

/// Reports the three distinct layers involved in XR foveated rendering:
/// runtime availability, application-enabled OpenXR extensions, and the
/// vendor's current system policy. This is intentionally read-only; in
/// particular it never writes PICO's persistent foveation property.
#[cfg(all(target_os = "android", feature = "render_debug"))]
pub fn log_android_xr_foveation_capabilities(
    available: Option<Res<OxrAvailableExtensions>>,
    enabled: Option<Res<OxrEnabledExtensions>>,
) {
    let Some(available) = available else {
        warn!("OpenXR foveation capability probe: runtime extension set unavailable");
        return;
    };
    let available = available.raw();
    let enabled = enabled.as_deref().map(|extensions| extensions.raw());
    let enabled_flag = |read: fn(&openxr::ExtensionSet) -> bool| enabled.map(read).unwrap_or(false);

    info!(
        "OpenXR foveation capability probe: available [swapchain_update_state={}, foveation={}, configuration={}, vulkan={}, eye_tracked={}] enabled [swapchain_update_state={}, foveation={}, configuration={}, vulkan={}, eye_tracked={}]",
        available.fb_swapchain_update_state,
        available.fb_foveation,
        available.fb_foveation_configuration,
        available.fb_foveation_vulkan,
        available.meta_foveation_eye_tracked,
        enabled_flag(|extensions| extensions.fb_swapchain_update_state),
        enabled_flag(|extensions| extensions.fb_foveation),
        enabled_flag(|extensions| extensions.fb_foveation_configuration),
        enabled_flag(|extensions| extensions.fb_foveation_vulkan),
        enabled_flag(|extensions| extensions.meta_foveation_eye_tracked),
    );

    match android_system_property("persist.pvr.foveation.level") {
        Some(value) => info!(
            "PICO system foveation policy (vendor-opaque, not proof of application shading rate): persist.pvr.foveation.level={value}"
        ),
        None => info!("PICO system foveation policy property is not present or not readable"),
    }

    if available.fb_foveation_vulkan && !enabled_flag(|extensions| extensions.fb_foveation_vulkan) {
        info!(
            "XR_FB_foveation_vulkan is advertised but not enabled by Zevy; the current swapchain therefore cannot be claimed as Fragment Density Map backed"
        );
    }
}

#[cfg(not(all(target_os = "android", feature = "render_debug")))]
pub fn log_android_xr_foveation_capabilities() {}

#[cfg(not(target_os = "android"))]
pub fn configure_android_display_refresh_rate() {}

#[cfg(target_os = "android")]
pub fn begin_android_xr_session_after_action_attach(
    session: Option<Res<bevy_mod_openxr::session::OxrSession>>,
    started: Option<ResMut<bevy_mod_openxr::resources::OxrSessionStarted>>,
    frame_waiter: Option<ResMut<bevy_mod_openxr::resources::OxrFrameWaiter>>,
    state: Option<Res<bevy_mod_xr::session::XrState>>,
    mut commands: Commands,
    mut state_changed: EventWriter<bevy_mod_xr::session::XrStateChanged>,
) {
    let Some(session) = session else {
        return;
    };
    let Some(mut started) = started else {
        return;
    };
    let Some(mut frame_waiter) = frame_waiter else {
        return;
    };
    if started.0 {
        return;
    }

    info!(
        "trying OpenXR session begin on Android after action attach; current state={:?}",
        state.as_deref()
    );
    match session.begin(openxr::ViewConfigurationType::PRIMARY_STEREO) {
        Ok(_) => {
            info!("OpenXR session begin succeeded");
            match frame_waiter.wait() {
                Ok(frame_state) => {
                    commands
                        .insert_resource(bevy_mod_openxr::resources::OxrFrameState(frame_state));
                    started.0 = true;
                    info!("Android OpenXR first frame waited after session begin");
                }
                Err(err) => {
                    warn!("Android OpenXR first frame wait failed after begin: {err}");
                    return;
                }
            }
            commands.insert_resource(bevy_mod_xr::session::XrState::Running);
            state_changed.write(bevy_mod_xr::session::XrStateChanged(
                bevy_mod_xr::session::XrState::Running,
            ));
        }
        Err(err) => {
            warn!("OpenXR session begin failed: {err}");
        }
    }
}

#[cfg(not(target_os = "android"))]
pub fn begin_android_xr_session_after_action_attach() {}

#[cfg(target_os = "android")]
pub fn request_android_xr_redraw() {}

#[cfg(not(target_os = "android"))]
pub fn request_android_xr_redraw() {}

#[cfg(target_os = "android")]
pub fn configure_android_window_plugin(plugin: &mut bevy::window::WindowPlugin) {
    plugin.primary_window = None;
    plugin.exit_condition = ExitCondition::DontExit;
    plugin.close_when_requested = false;
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub fn configure_android_window_plugin(_plugin: &mut bevy::window::WindowPlugin) {}

#[cfg(target_os = "android")]
pub fn enable_android_openxr_extensions(oxr_init: &mut OxrInitPlugin) {
    let exts = oxr_init.exts.raw_mut();
    exts.khr_android_create_instance = true;
    exts.khr_loader_init_android = true;
    exts.fb_display_refresh_rate = true;
}

#[cfg(not(target_os = "android"))]
pub fn enable_android_openxr_extensions(_oxr_init: &mut OxrInitPlugin) {}
