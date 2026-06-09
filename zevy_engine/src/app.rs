use std::env;

use bevy::{prelude::*, render::render_resource::TextureFormat, utils::default};
use bevy_mod_openxr::resources::OxrSessionConfig;
use bevy_xr_utils::{
    tracking_utils::suggest_action_bindings, xr_utils_actions::XRUtilsActionSystemSet,
};

use crate::{input, input::EngineInputPlugin, platform, scene::ScenePlugin, xr};

pub fn main() {
    run();
}

pub fn run() {
    let launch_mode = LaunchMode::from_args();
    eprintln!("Starting zevy_engine in {} mode", launch_mode.label());
    platform::log_android_context("run");

    let mut app = App::new();

    match launch_mode {
        LaunchMode::Desktop => {
            app.add_plugins(DefaultPlugins);
        }
        LaunchMode::Xr => {
            app.add_plugins(xr::plugins())
                .add_plugins(bevy_xr_utils::tracking_utils::TrackingUtilitiesPlugin)
                .add_plugins(bevy_xr_utils::xr_utils_actions::XRUtilsActionsPlugin)
                .add_plugins(bevy_mod_xr::hand_debug_gizmos::HandGizmosPlugin)
                .add_systems(
                    Startup,
                    input::setup_xr_actions.before(XRUtilsActionSystemSet::CreateEvents),
                )
                .add_systems(
                    bevy_mod_openxr::action_binding::OxrSendActionBindings,
                    suggest_action_bindings,
                )
                .add_systems(
                    bevy_mod_xr::session::XrSessionCreated,
                    xr::spawn_anchor_visuals,
                )
                .add_systems(
                    PostUpdate,
                    (
                        platform::configure_android_display_refresh_rate,
                        platform::begin_android_xr_session_after_action_attach,
                    )
                        .chain(),
                )
                .add_systems(
                    bevy_mod_xr::session::XrFirst,
                    (
                        platform::poll_android_activity_events,
                        xr::request_session_when_available
                            .before(bevy_mod_xr::session::XrHandleEvents::SessionStateUpdateEvents),
                    )
                        .chain(),
                )
                .add_systems(
                    Update,
                    (
                        platform::request_android_xr_redraw,
                        xr::sync_mirror_camera,
                        xr::log_state_changes,
                        xr::log_render_status,
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
        .add_systems(Startup, log_launch_mode)
        .add_plugins(EngineInputPlugin)
        .add_plugins(ScenePlugin)
        .run();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchMode {
    Desktop,
    Xr,
}

#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartupMode(pub LaunchMode);

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

    pub fn label(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Xr => "xr",
        }
    }
}

fn log_launch_mode(startup_mode: Res<StartupMode>) {
    info!("Starting zevy_engine in {} mode", startup_mode.0.label());
}
