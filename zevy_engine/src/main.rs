use std::env;

use bevy::{
    math::vec3,
    prelude::*,
    render::{RenderPlugin, pipelined_rendering::PipelinedRenderingPlugin},
    utils::default,
    window::{PresentMode, Window, WindowPlugin},
};
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
    resources::{OxrCurrentSessionConfig, OxrViews},
    spaces::{OxrSpacePatchingPlugin, OxrSpatialPlugin},
};
use bevy_mod_xr::{
    camera::{XrCamera, XrCameraPlugin},
    session::{XrSessionCreated, XrSessionPlugin, XrState, XrTracker, XrTrackingRoot},
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

fn main() {
    let launch_mode = LaunchMode::from_args();
    eprintln!("Starting zevy_engine in {} mode", launch_mode.label());

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
                    Update,
                    (
                        handle_xr_locomotion,
                        log_xr_trigger_input,
                        sync_mirror_camera,
                        log_xr_state_changes,
                        log_xr_render_status,
                    ),
                );
        }
    }

    app.insert_resource(StartupMode(launch_mode))
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.03)))
        .add_systems(Startup, (log_launch_mode, setup_scene))
        .run();
}

fn xr_plugins() -> impl PluginGroup {
    let mut oxr_init = OxrInitPlugin::default();
    oxr_init.exts.disable_fb_passthrough();

    DefaultPlugins
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
        .add(OxrSpacePatchingPlugin)
        .set(WindowPlugin {
            primary_window: Some(Window {
                transparent: true,
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            #[cfg(target_os = "android")]
            close_when_requested: true,
            ..default()
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchMode {
    Desktop,
    Xr,
}

#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq)]
struct StartupMode(LaunchMode);

#[derive(Component)]
struct XrMoveAction;

#[derive(Component)]
struct XrTriggerAction;

#[derive(Component)]
struct MirrorCamera;

impl LaunchMode {
    fn from_args() -> Self {
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
    commands.spawn((
        Name::new("SceneSphere"),
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().ico(5).unwrap())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.7, 0.9),
            metallic: 0.85,
            perceptual_roughness: 0.15,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));

    commands.spawn((
        Name::new("Ground"),
        Mesh3d(meshes.add(Circle::new(6.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.08, 0.09, 0.1))),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    commands.spawn((
        Name::new("KeyLight"),
        PointLight {
            shadows_enabled: true,
            intensity: 2_000_000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

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
