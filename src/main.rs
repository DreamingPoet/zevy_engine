use std::env;

use bevy::{prelude::*, render::pipelined_rendering::PipelinedRenderingPlugin};
use bevy_mod_openxr::add_xr_plugins;

fn main() {
    let launch_mode = LaunchMode::from_args();
    let mut app = App::new();

    match launch_mode {
        LaunchMode::Desktop => {
            app.add_plugins(DefaultPlugins);
        }
        LaunchMode::Xr => {
            app.add_plugins(add_xr_plugins(
                DefaultPlugins.build().disable::<PipelinedRenderingPlugin>(),
            ));
        }
    }

    app.insert_resource(StartupMode(launch_mode))
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.03)))
        .add_systems(Startup, (log_launch_mode, setup_scene))
        .run();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchMode {
    Desktop,
    Xr,
}

#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq)]
struct StartupMode(LaunchMode);

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
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
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
        Mesh3d(meshes.add(Circle::new(6.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.08, 0.09, 0.1))),
        Transform::from_rotation(Quat::from_rotation_x(
            -std::f32::consts::FRAC_PI_2,
        )),
    ));

    commands.spawn((
        PointLight {
            shadows_enabled: true,
            intensity: 2_000_000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
