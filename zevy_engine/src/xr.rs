#[cfg(target_os = "android")]
use std::time::Duration;

use bevy::{
    math::vec3,
    prelude::*,
    render::{RenderPlugin, pipelined_rendering::PipelinedRenderingPlugin},
    window::WindowPlugin,
};
#[cfg(not(target_os = "android"))]
use bevy::window::{PresentMode, Window};
use bevy_mod_openxr::{
    action_binding::OxrActionBindingPlugin,
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
    session::{XrCreateSessionEvent, XrSessionPlugin, XrState, XrTracker, XrTrackingRoot},
};
use bevy_xr_utils::{
    tracking_utils::{XrTrackedLeftGrip, XrTrackedRightGrip, XrTrackedView},
    xr_utils_actions::{
        ActiveSet, XRUtilsAction, XRUtilsActionSet, XRUtilsActionState, XRUtilsBinding,
    },
};

use crate::{platform, scene::MirrorCamera};

#[derive(Component)]
pub struct XrMoveAction;

#[derive(Component)]
pub struct XrTriggerAction;

pub fn plugins() -> bevy::app::PluginGroupBuilder {
    let mut oxr_init = OxrInitPlugin::default();
    oxr_init.exts.disable_fb_passthrough();
    platform::enable_android_openxr_extensions(&mut oxr_init);

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
        .set({
            let mut window_plugin = WindowPlugin::default();
            platform::configure_android_window_plugin(&mut window_plugin);
            window_plugin
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

pub fn request_session_when_available(
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

pub fn setup_actions(mut commands: Commands) {
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

pub fn spawn_anchor_visuals(
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

pub fn handle_locomotion(
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

pub fn log_trigger_input(action_query: Query<&XRUtilsActionState, With<XrTriggerAction>>) {
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

pub fn sync_mirror_camera(
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

pub fn log_render_status(
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

pub fn log_state_changes(mut state_events: EventReader<bevy_mod_xr::session::XrStateChanged>) {
    for event in state_events.read() {
        eprintln!("XR state changed: {:?}", event.0);
    }
}
