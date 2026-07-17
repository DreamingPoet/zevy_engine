use std::{
    env,
    path::PathBuf,
    time::{Duration, Instant},
};

use bevy::{
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    utils::default,
    window::WindowPlugin,
};
use bevy_mod_openxr::resources::OxrSessionConfig;
use bevy_xr_utils::{
    tracking_utils::suggest_action_bindings, xr_utils_actions::XRUtilsActionSystemSet,
};

use crate::{
    input,
    input::EngineInputPlugin,
    platform,
    scene::{ImportedLevelCameraFramed, ScenePlugin},
    xr,
};

pub fn main() {
    run();
}

pub fn run() {
    let launch_mode = LaunchMode::from_args();
    let screenshot_path = screenshot_path_from_args();
    let screenshot_delay = screenshot_delay_from_args();
    eprintln!("Starting zevy_engine in {} mode", launch_mode.label());
    platform::log_android_context("run");

    let mut app = App::new();

    match launch_mode {
        LaunchMode::Desktop => {
            if screenshot_path.is_some() {
                app.add_plugins(DefaultPlugins.set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Zevy Level Preview".to_owned(),
                        resolution: (1600.0, 900.0).into(),
                        ..default()
                    }),
                    ..default()
                }));
            } else {
                app.add_plugins(DefaultPlugins);
            }
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
        .add_plugins(ScenePlugin);

    if let Some(path) = screenshot_path {
        if let Some(parent) = path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            eprintln!(
                "Failed to create screenshot directory '{}': {error}",
                parent.display()
            );
        }

        app.insert_resource(LevelScreenshotRequest {
            path,
            requested: false,
            captured: false,
            frames_after_framing: 0,
            started_at: Instant::now(),
            capture_after: screenshot_delay,
        })
        .add_systems(
            Update,
            (capture_level_screenshot, finish_level_screenshot).chain(),
        );
    }

    app.run();
}

#[derive(Resource)]
struct LevelScreenshotRequest {
    path: PathBuf,
    requested: bool,
    captured: bool,
    frames_after_framing: u16,
    started_at: Instant,
    capture_after: Duration,
}

fn screenshot_path_from_args() -> Option<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(path) = arg.strip_prefix("--screenshot=") {
            if !path.trim().is_empty() {
                return Some(PathBuf::from(path));
            }
        } else if arg == "--screenshot"
            && let Some(path) = args.next().filter(|path| !path.trim().is_empty())
        {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn screenshot_delay_from_args() -> Duration {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = if let Some(value) = arg.strip_prefix("--screenshot-delay=") {
            Some(value.to_owned())
        } else if arg == "--screenshot-delay" {
            args.next()
        } else {
            None
        };
        if let Some(seconds) = value.and_then(|value| value.parse::<u64>().ok()) {
            return Duration::from_secs(seconds.min(240));
        }
    }
    Duration::ZERO
}

fn capture_level_screenshot(
    mut commands: Commands,
    framed_cameras: Query<(), With<ImportedLevelCameraFramed>>,
    mut request: ResMut<LevelScreenshotRequest>,
) {
    if request.requested || request.captured || framed_cameras.is_empty() {
        return;
    }

    request.frames_after_framing += 1;
    if request.frames_after_framing < 30 || request.started_at.elapsed() < request.capture_after {
        return;
    }

    let output_path = request.path.clone();
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(output_path.clone()))
        .observe(mark_level_screenshot_captured);
    request.requested = true;
    info!(
        "Capturing Zevy Level screenshot to {}",
        output_path.display()
    );
}

fn mark_level_screenshot_captured(
    _trigger: Trigger<ScreenshotCaptured>,
    mut request: ResMut<LevelScreenshotRequest>,
) {
    request.captured = true;
}

fn finish_level_screenshot(request: Res<LevelScreenshotRequest>, mut exit: EventWriter<AppExit>) {
    if request.captured {
        info!(
            "Zevy Level screenshot completed: {}",
            request.path.display()
        );
        exit.write(AppExit::Success);
    } else if request.started_at.elapsed().as_secs() >= 300 {
        error!(
            "Timed out while rendering Zevy Level screenshot: {}",
            request.path.display()
        );
        exit.write(AppExit::error());
    }
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
