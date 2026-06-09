use std::env;
#[cfg(target_os = "android")]
use std::time::Duration;

#[cfg(target_os = "android")]
use bevy::window::ExitCondition;
#[cfg(not(target_os = "android"))]
use bevy::window::{PresentMode, Window};
use bevy::{
    math::vec3,
    prelude::*,
    render::{
        RenderPlugin, pipelined_rendering::PipelinedRenderingPlugin,
        render_resource::TextureFormat, view::NoFrustumCulling,
    },
    utils::default,
    window::WindowPlugin,
};
#[cfg(target_os = "android")]
use bevy_mod_openxr::exts::OxrEnabledExtensions;
use bevy_mod_openxr::{
    action_binding::{OxrActionBindingPlugin, OxrSendActionBindings},
    action_set_attaching::OxrActionAttachingPlugin,
    action_set_syncing::OxrActionSyncingPlugin,
    features::{handtracking::HandTrackingPlugin, overlay::OxrOverlayPlugin},
    helper_traits::ToQuat,
    init::OxrInitPlugin,
    poll_events::OxrEventsPlugin,
    reference_space::OxrReferenceSpacePlugin,
    render::OxrRenderPlugin,
    resources::{OxrCurrentSessionConfig, OxrSessionConfig, OxrViews},
    spaces::{OxrSpacePatchingPlugin, OxrSpatialPlugin},
};
use bevy_mod_xr::{
    camera::{XrCamera, XrCameraPlugin},
    session::{
        XrCreateSessionEvent, XrFirst, XrHandleEvents, XrSessionCreated, XrSessionPlugin, XrState,
        XrTracker, XrTrackingRoot,
    },
};
use bevy_xr_utils::{
    tracking_utils::{
        TrackingUtilitiesPlugin, XrTrackedLeftGrip, XrTrackedRightGrip, XrTrackedView,
        suggest_action_bindings,
    },
    xr_utils_actions::{
        ActiveSet, XRUtilsAction, XRUtilsActionSet, XRUtilsActionState, XRUtilsActionSystemSet,
        XRUtilsActionsPlugin, XRUtilsBinding,
    },
};

pub fn main() {
    run();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: bevy::window::android_activity::AndroidApp) {
    let vm = android_app.vm_as_ptr();
    let activity = android_app.activity_as_ptr();

    eprintln!("Android OpenXR context bridge: android_main vm={vm:p} activity={activity:p}");

    // android-activity initializes ndk_context with the Application object.
    // PICO's OpenXR runtime requires the Activity object for XR_KHR_android_create_instance.
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
    main();
}

pub fn run() {
    let launch_mode = LaunchMode::from_args();
    eprintln!("Starting zevy_engine in {} mode", launch_mode.label());
    log_android_context("run");

    let mut app = App::new();

    match launch_mode {
        LaunchMode::Desktop => {
            app.add_plugins(DefaultPlugins);
        }
        LaunchMode::Xr => {
            app.add_plugins(xr_plugins())
                .add_plugins(TrackingUtilitiesPlugin)
                .add_plugins(XRUtilsActionsPlugin)
                .add_plugins(bevy_mod_xr::hand_debug_gizmos::HandGizmosPlugin)
                .add_systems(
                    Startup,
                    setup_xr_actions.before(XRUtilsActionSystemSet::CreateEvents),
                )
                .add_systems(OxrSendActionBindings, suggest_action_bindings)
                .add_systems(XrSessionCreated, spawn_xr_anchor_visuals)
                .add_systems(
                    PostUpdate,
                    (
                        configure_android_display_refresh_rate,
                        begin_android_xr_session_after_action_attach,
                    )
                        .chain(),
                )
                .add_systems(
                    XrFirst,
                    (
                        poll_android_activity_events,
                        request_xr_session_when_available
                            .before(XrHandleEvents::SessionStateUpdateEvents),
                    )
                        .chain(),
                )
                .add_systems(
                    Update,
                    (
                        request_android_xr_redraw,
                        handle_xr_locomotion,
                        log_xr_trigger_input,
                        sync_mirror_camera,
                        log_xr_state_changes,
                        log_xr_render_status,
                    ),
                )
                .insert_resource(OxrSessionConfig {
                    formats: Some(vec![
                        TextureFormat::Rgba8UnormSrgb,
                        TextureFormat::Bgra8UnormSrgb,
                        TextureFormat::Rgba8Unorm,
                        TextureFormat::Bgra8Unorm,
                    ]),
                    ..default()
                });
        }
    }

    app.insert_resource(StartupMode(launch_mode))
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.03)))
        .add_systems(Startup, (log_launch_mode, setup_scene))
        .run();
}

#[cfg(target_os = "android")]
fn log_android_context(stage: &str) {
    let context = ndk_context::android_context();
    eprintln!(
        "Android OpenXR context bridge: {stage} vm={:p} context={:p}",
        context.vm(),
        context.context()
    );
}

#[cfg(not(target_os = "android"))]
fn log_android_context(_stage: &str) {}

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
fn poll_android_activity_events(mut exit: EventWriter<AppExit>) {
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
fn poll_android_activity_events() {}

fn request_xr_session_when_available(
    state: Option<Res<XrState>>,
    mut requested: Local<bool>,
    mut create_session: EventWriter<XrCreateSessionEvent>,
) {
    if *requested {
        return;
    }

    if state.is_some_and(|state| *state == XrState::Available) {
        create_session.write_default();
        *requested = true;
        eprintln!("XR create session requested from Available state");
    }
}

#[cfg(target_os = "android")]
fn configure_android_display_refresh_rate(
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

#[cfg(not(target_os = "android"))]
fn configure_android_display_refresh_rate() {}

#[cfg(target_os = "android")]
fn begin_android_xr_session_after_action_attach(
    session: Option<Res<bevy_mod_openxr::session::OxrSession>>,
    started: Option<ResMut<bevy_mod_openxr::resources::OxrSessionStarted>>,
    frame_waiter: Option<ResMut<bevy_mod_openxr::resources::OxrFrameWaiter>>,
    state: Option<Res<XrState>>,
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
            commands.insert_resource(XrState::Running);
            state_changed.write(bevy_mod_xr::session::XrStateChanged(XrState::Running));
        }
        Err(err) => {
            warn!("OpenXR session begin failed: {err}");
        }
    }
}

#[cfg(not(target_os = "android"))]
fn begin_android_xr_session_after_action_attach() {}

#[cfg(target_os = "android")]
fn request_android_xr_redraw() {}

#[cfg(not(target_os = "android"))]
fn request_android_xr_redraw() {}

fn xr_plugins() -> bevy::app::PluginGroupBuilder {
    let mut oxr_init = OxrInitPlugin::default();
    oxr_init.exts.disable_fb_passthrough();
    enable_android_openxr_extensions(&mut oxr_init);

    let plugins = DefaultPlugins
        .build()
        .disable::<PipelinedRenderingPlugin>()
        .disable::<RenderPlugin>()
        .add_before::<RenderPlugin>(XrSessionPlugin { auto_handle: true })
        .add_before::<RenderPlugin>(oxr_init)
        .add(OxrEventsPlugin)
        .add(OxrReferenceSpacePlugin::default())
        .add(OxrRenderPlugin::default())
        .add(HandTrackingPlugin::default())
        .add(XrCameraPlugin)
        .add(OxrActionAttachingPlugin)
        .add(OxrActionBindingPlugin)
        .add(OxrActionSyncingPlugin)
        .add(OxrOverlayPlugin)
        .add(OxrSpatialPlugin)
        .add(OxrSpacePatchingPlugin);

    #[cfg(target_os = "android")]
    let plugins = plugins
        .disable::<bevy::winit::WinitPlugin>()
        .add(bevy::app::ScheduleRunnerPlugin::run_loop(
            Duration::from_secs_f64(1.0 / 90.0),
        ))
        .set(WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            close_when_requested: false,
            ..default()
        });

    #[cfg(not(target_os = "android"))]
    let plugins = plugins.set(WindowPlugin {
        primary_window: Some(Window {
            transparent: true,
            present_mode: PresentMode::AutoNoVsync,
            ..default()
        }),
        ..default()
    });

    plugins
}

#[cfg(target_os = "android")]
fn enable_android_openxr_extensions(oxr_init: &mut OxrInitPlugin) {
    let exts = oxr_init.exts.raw_mut();
    exts.khr_android_create_instance = true;
    exts.khr_loader_init_android = true;
    exts.fb_display_refresh_rate = true;
}

#[cfg(not(target_os = "android"))]
fn enable_android_openxr_extensions(_oxr_init: &mut OxrInitPlugin) {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchMode {
    Desktop,
    Xr,
}

#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq)]
struct StartupMode(LaunchMode);

#[cfg(target_os = "android")]
#[derive(Resource, Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AndroidDisplayRefreshConfigured;

#[derive(Component)]
struct XrMoveAction;

#[derive(Component)]
struct XrTriggerAction;

#[derive(Component)]
struct MirrorCamera;

impl LaunchMode {
    fn from_args() -> Self {
        #[cfg(target_os = "android")]
        let mut mode = Self::Xr;
        #[cfg(not(target_os = "android"))]
        let mut mode = Self::Desktop;

        for arg in env::args().skip(1) {
            match arg.as_str() {
                "--xr" => mode = Self::Xr,
                "--desktop" => mode = Self::Desktop,
                _ => {}
            }
        }

        mode
    }

    fn label(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Xr => "xr",
        }
    }
}

fn log_launch_mode(startup_mode: Res<StartupMode>) {
    info!("Starting zevy_engine in {} mode", startup_mode.0.label());
}

fn setup_scene(
    startup_mode: Res<StartupMode>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mobile_xr = cfg!(target_os = "android") && startup_mode.0 == LaunchMode::Xr;
    let sphere_subdivisions = if mobile_xr { 3 } else { 5 };

    commands.spawn((
        Name::new("SceneSphere"),
        Mesh3d(meshes.add(Sphere::new(0.5).mesh().ico(sphere_subdivisions).unwrap())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.7, 0.9),
            metallic: 0.85,
            perceptual_roughness: 0.15,
            ..default()
        })),
        NoFrustumCulling,
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));

    commands.spawn((
        Name::new("Ground"),
        Mesh3d(meshes.add(Circle::new(6.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.08, 0.09, 0.1))),
        NoFrustumCulling,
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    commands.spawn((
        Name::new("KeyLight"),
        PointLight {
            shadows_enabled: !mobile_xr,
            intensity: if mobile_xr { 250_000.0 } else { 2_000_000.0 },
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

    #[cfg(target_os = "android")]
    if startup_mode.0 == LaunchMode::Xr {
        return;
    }

    let camera_name = match startup_mode.0 {
        LaunchMode::Desktop => "DesktopCamera",
        LaunchMode::Xr => "MirrorCamera",
    };
    let mut camera = commands.spawn((
        Name::new(camera_name),
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    if startup_mode.0 == LaunchMode::Xr {
        camera.insert(MirrorCamera);
    }
}

fn setup_xr_actions(mut commands: Commands) {
    let locomotion_set = commands
        .spawn((
            XRUtilsActionSet {
                name: "locomotion".into(),
                pretty_name: "Locomotion".into(),
                priority: 0,
            },
            ActiveSet,
        ))
        .id();

    let move_action = commands
        .spawn((
            XRUtilsAction {
                action_name: "move".into(),
                localized_name: "Move".into(),
                action_type: bevy_mod_xr::actions::ActionType::Vector,
            },
            XrMoveAction,
        ))
        .id();

    let move_binding_touch = commands
        .spawn(XRUtilsBinding {
            profile: "/interaction_profiles/oculus/touch_controller".into(),
            binding: "/user/hand/right/input/thumbstick".into(),
        })
        .id();
    let move_binding_index = commands
        .spawn(XRUtilsBinding {
            profile: "/interaction_profiles/valve/index_controller".into(),
            binding: "/user/hand/right/input/thumbstick".into(),
        })
        .id();

    commands.entity(move_action).add_child(move_binding_touch);
    commands.entity(move_action).add_child(move_binding_index);
    commands.entity(locomotion_set).add_child(move_action);

    let input_set = commands
        .spawn((
            XRUtilsActionSet {
                name: "input".into(),
                pretty_name: "Input".into(),
                priority: 1,
            },
            ActiveSet,
        ))
        .id();

    let trigger_action = commands
        .spawn((
            XRUtilsAction {
                action_name: "trigger_click".into(),
                localized_name: "Trigger Click".into(),
                action_type: bevy_mod_xr::actions::ActionType::Bool,
            },
            XrTriggerAction,
        ))
        .id();

    let trigger_binding_touch = commands
        .spawn(XRUtilsBinding {
            profile: "/interaction_profiles/oculus/touch_controller".into(),
            binding: "/user/hand/right/input/trigger".into(),
        })
        .id();
    let trigger_binding_index = commands
        .spawn(XRUtilsBinding {
            profile: "/interaction_profiles/valve/index_controller".into(),
            binding: "/user/hand/right/input/trigger".into(),
        })
        .id();

    commands
        .entity(trigger_action)
        .add_child(trigger_binding_touch);
    commands
        .entity(trigger_action)
        .add_child(trigger_binding_index);
    commands.entity(input_set).add_child(trigger_action);
}

fn spawn_xr_anchor_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("XrHeadAnchor"),
        Mesh3d(meshes.add(Cuboid::new(0.18, 0.12, 0.12))),
        MeshMaterial3d(materials.add(Color::srgb(0.95, 0.4, 0.4))),
        Transform::default(),
        XrTrackedView,
        XrTracker,
    ));

    commands.spawn((
        Name::new("XrLeftGripAnchor"),
        Mesh3d(meshes.add(Cuboid::new(0.08, 0.12, 0.18))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.7, 1.0))),
        Transform::default(),
        XrTrackedLeftGrip,
        XrTracker,
    ));

    commands.spawn((
        Name::new("XrRightGripAnchor"),
        Mesh3d(meshes.add(Cuboid::new(0.08, 0.12, 0.18))),
        MeshMaterial3d(materials.add(Color::srgb(1.0, 0.65, 0.25))),
        Transform::default(),
        XrTrackedRightGrip,
        XrTracker,
    ));
}

fn handle_xr_locomotion(
    action_query: Query<&XRUtilsActionState, With<XrMoveAction>>,
    mut tracking_root_query: Query<&mut Transform, With<XrTrackingRoot>>,
    views: Res<OxrViews>,
    time: Res<Time>,
) {
    let Ok(mut tracking_root) = tracking_root_query.single_mut() else {
        return;
    };

    let Some(view) = views.first() else {
        return;
    };

    for state in &action_query {
        let XRUtilsActionState::Vector(vector_state) = state else {
            continue;
        };

        if !vector_state.is_active {
            continue;
        }

        let input_vector = vec3(
            vector_state.current_state[0],
            0.0,
            -vector_state.current_state[1],
        );

        if input_vector.length_squared() <= f32::EPSILON {
            continue;
        }

        let speed = 2.5;
        let view_rotation = tracking_root.rotation * view.pose.orientation.to_quat();
        let locomotion = view_rotation.mul_vec3(input_vector);
        let flat_locomotion = Vec3::new(locomotion.x, 0.0, locomotion.z).normalize_or_zero();

        tracking_root.translation += flat_locomotion * speed * time.delta_secs();
    }
}

fn log_xr_trigger_input(action_query: Query<&XRUtilsActionState, With<XrTriggerAction>>) {
    for state in &action_query {
        let XRUtilsActionState::Bool(button_state) = state else {
            continue;
        };

        if button_state.is_active
            && button_state.changed_since_last_sync
            && button_state.current_state
        {
            info!("XR trigger pressed");
        }
    }
}

fn sync_mirror_camera(
    views: Res<OxrViews>,
    tracking_root_query: Query<&Transform, With<XrTrackingRoot>>,
    mut mirror_camera_query: Query<&mut Transform, (With<MirrorCamera>, Without<XrTrackingRoot>)>,
) {
    let Some(view) = views.first() else {
        return;
    };
    let Ok(tracking_root) = tracking_root_query.single() else {
        return;
    };

    let view_transform = Transform {
        translation: Vec3::new(
            view.pose.position.x,
            view.pose.position.y,
            view.pose.position.z,
        ),
        rotation: view.pose.orientation.to_quat(),
        scale: Vec3::ONE,
    };
    let mirror_transform = tracking_root.mul_transform(view_transform);

    for mut camera_transform in &mut mirror_camera_query {
        *camera_transform = mirror_transform;
    }
}

fn log_xr_render_status(
    mut logged: Local<bool>,
    mut last_missing_log: Local<f32>,
    state: Option<Res<XrState>>,
    session_config: Option<Res<OxrCurrentSessionConfig>>,
    xr_cameras: Query<&XrCamera>,
    time: Res<Time>,
) {
    if *logged {
        return;
    }

    let camera_count = xr_cameras.iter().count();
    let Some(session_config) = session_config else {
        let now = time.elapsed_secs();
        if now - *last_missing_log >= 2.0 {
            eprintln!(
                "XR waiting for render resources: state={:?}, cameras={}",
                state.as_deref(),
                camera_count,
            );
            *last_missing_log = now;
        }
        return;
    };

    eprintln!(
        "XR render ready: state={:?}, cameras={}, resolution={}x{}, blend_mode={:?}, format={:?}",
        state.as_deref(),
        camera_count,
        session_config.resolution.x,
        session_config.resolution.y,
        session_config.blend_mode,
        session_config.format,
    );
    *logged = true;
}

fn log_xr_state_changes(mut state_events: EventReader<bevy_mod_xr::session::XrStateChanged>) {
    for event in state_events.read() {
        eprintln!("XR state changed: {:?}", event.0);
    }
}
