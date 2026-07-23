use bevy::{
    pbr::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use bevy_mod_xr::{camera::XrCamera, session::XrTrackingRoot};

use crate::{
    app::LaunchMode,
    input::{EngineInputState, InputAxis2},
    scene::{LevelEntity, MirrorCamera, desktop_player::DesktopLevelPlayer},
    shadow_motion_policy::ShadowCasterMotionPolicy,
};

use super::{CurrentLevel, LevelId};

const XR_LEVEL_PLAYER_MOVE_SPEED: f32 = 2.5;

#[derive(Component, Clone, Copy)]
pub(super) struct OrbitingLight {
    center: Vec3,
    radius: f32,
    height: f32,
    phase: f32,
    speed: f32,
}

#[derive(Component, Clone, Copy)]
pub(super) struct MovingShadowTestCaster {
    base_translation: Vec3,
}

pub(super) fn level_fog(level: &super::LevelId) -> Option<DistanceFog> {
    match level {
        super::LevelId::FogPyramid => Some(fog_pyramid_fog()),
        super::LevelId::PerformanceLab | super::LevelId::Empty | super::LevelId::Asset(_) => None,
    }
}

pub(super) fn spawn_fog_pyramid(
    launch_mode: LaunchMode,
    level_fog: Option<DistanceFog>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.157, 0.133, 0.106),
        perceptual_roughness: 1.0,
        ..default()
    });

    for (x, z) in &[(-1.5, -1.5), (1.5, -1.5), (1.5, 1.5), (-1.5, 1.5)] {
        commands.spawn((
            Name::new("FogPillar"),
            LevelEntity,
            Mesh3d(meshes.add(Cuboid::new(1.0, 3.0, 1.0))),
            MeshMaterial3d(stone.clone()),
            Transform::from_xyz(*x, 1.5, *z),
        ));
    }

    commands.spawn((
        Name::new("FogOrb"),
        LevelEntity,
        Mesh3d(meshes.add(Sphere::default())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.071, 0.384, 0.071, 0.8),
            reflectance: 1.0,
            perceptual_roughness: 0.0,
            metallic: 0.5,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(1.75)).with_translation(Vec3::new(0.0, 4.0, 0.0)),
        NotShadowCaster,
        NotShadowReceiver,
    ));

    for i in 0..50 {
        let half_size = i as f32 / 2.0 + 3.0;
        let y = -i as f32 / 2.0;
        commands.spawn((
            Name::new("FogStep"),
            LevelEntity,
            Mesh3d(meshes.add(Cuboid::new(2.0 * half_size, 0.5, 2.0 * half_size))),
            MeshMaterial3d(stone.clone()),
            Transform::from_xyz(0.0, y + 0.25, 0.0),
        ));
    }

    commands.spawn((
        Name::new("FogSkyCube"),
        LevelEntity,
        Mesh3d(meshes.add(Cuboid::new(2.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.533, 0.533, 0.533),
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(1_000_000.0)),
    ));

    commands.spawn((
        Name::new("FogPointLight"),
        LevelEntity,
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 1.0, 0.0),
    ));

    let _ = spawn_level_camera(launch_mode, level_fog, commands);
}

pub(super) fn spawn_performance_lab(
    launch_mode: LaunchMode,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let mobile_xr = cfg!(target_os = "android") && launch_mode == LaunchMode::Xr;
    let sphere_subdivisions = if mobile_xr { 2 } else { 4 };

    let brushed_metal = materials.add(StandardMaterial {
        base_color: Color::srgb(0.58, 0.61, 0.64),
        metallic: 0.9,
        perceptual_roughness: 0.28,
        ..default()
    });
    let blue_ceramic = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.42, 0.70),
        metallic: 0.05,
        perceptual_roughness: 0.18,
        ..default()
    });
    let warm_plastic = materials.add(StandardMaterial {
        base_color: Color::srgb(0.76, 0.43, 0.30),
        metallic: 0.0,
        perceptual_roughness: 0.48,
        ..default()
    });
    let matte_green = materials.add(StandardMaterial {
        base_color: Color::srgb(0.34, 0.55, 0.38),
        metallic: 0.0,
        perceptual_roughness: 0.72,
        ..default()
    });
    let dark_rubber = materials.add(StandardMaterial {
        base_color: Color::srgb(0.06, 0.065, 0.07),
        metallic: 0.0,
        perceptual_roughness: 0.86,
        ..default()
    });
    let muted_gloss = materials.add(StandardMaterial {
        base_color: Color::srgb(0.54, 0.46, 0.70),
        metallic: 0.25,
        perceptual_roughness: 0.22,
        ..default()
    });

    let small_models = [
        (
            "PerfCube",
            meshes.add(Cuboid::new(0.36, 0.36, 0.36)),
            brushed_metal.clone(),
            Transform::from_xyz(-0.55, 0.28, -0.35).with_rotation(Quat::from_rotation_y(0.45)),
        ),
        (
            "PerfSphere",
            meshes.add(Sphere::new(0.23).mesh().ico(sphere_subdivisions).unwrap()),
            blue_ceramic.clone(),
            Transform::from_xyz(0.0, 0.30, -0.45),
        ),
        (
            "PerfCapsule",
            meshes.add(Capsule3d::new(0.13, 0.18)),
            warm_plastic.clone(),
            Transform::from_xyz(0.55, 0.32, -0.34).with_rotation(Quat::from_rotation_z(0.35)),
        ),
        (
            "PerfTorus",
            meshes.add(Torus::default()),
            matte_green.clone(),
            Transform::from_xyz(-0.42, 0.34, 0.25)
                .with_scale(Vec3::splat(0.24))
                .with_rotation(Quat::from_rotation_x(1.1)),
        ),
        (
            "PerfCylinder",
            meshes.add(Cylinder::new(0.16, 0.42).mesh().resolution(24)),
            dark_rubber.clone(),
            Transform::from_xyz(0.18, 0.32, 0.20).with_rotation(Quat::from_rotation_x(0.15)),
        ),
        (
            "PerfCone",
            meshes.add(Cone::new(0.18, 0.42).mesh().resolution(24)),
            muted_gloss.clone(),
            Transform::from_xyz(0.58, 0.31, 0.23).with_rotation(Quat::from_rotation_y(-0.55)),
        ),
    ];

    for (name, mesh, material, transform) in small_models {
        let mut entity = commands.spawn((
            Name::new(name),
            LevelEntity,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            transform,
        ));
        if name == "PerfCube" {
            entity.insert((
                ShadowCasterMotionPolicy::automatic(),
                MovingShadowTestCaster {
                    base_translation: transform.translation,
                },
            ));
        }
    }

    commands.spawn((
        Name::new("Ground"),
        LevelEntity,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(3.2, 3.2).subdivisions(4))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.075, 0.08, 0.085),
            metallic: 0.0,
            perceptual_roughness: 0.64,
            ..default()
        })),
        Transform::default(),
    ));

    let light_marker_mesh = meshes.add(Sphere::new(0.045).mesh().ico(1).unwrap());
    let light_colors = [
        (
            Color::srgb(0.92, 0.62, 0.58),
            LinearRgba::rgb(2.76, 1.86, 1.74),
        ),
        (
            Color::srgb(0.96, 0.74, 0.48),
            LinearRgba::rgb(2.88, 2.22, 1.44),
        ),
        (
            Color::srgb(0.78, 0.86, 0.52),
            LinearRgba::rgb(2.34, 2.58, 1.56),
        ),
        (
            Color::srgb(0.55, 0.84, 0.65),
            LinearRgba::rgb(1.65, 2.52, 1.95),
        ),
        (
            Color::srgb(0.52, 0.80, 0.86),
            LinearRgba::rgb(1.56, 2.40, 2.58),
        ),
        (
            Color::srgb(0.58, 0.66, 0.92),
            LinearRgba::rgb(1.74, 1.98, 2.76),
        ),
        (
            Color::srgb(0.72, 0.58, 0.90),
            LinearRgba::rgb(2.16, 1.74, 2.70),
        ),
        (
            Color::srgb(0.90, 0.58, 0.74),
            LinearRgba::rgb(2.70, 1.74, 2.22),
        ),
    ];

    for (index, (color, emissive)) in light_colors.into_iter().enumerate() {
        let phase = index as f32 / 8.0 * std::f32::consts::TAU;
        let radius = 1.05 + (index % 2) as f32 * 0.18;
        let height = 0.72 + (index % 3) as f32 * 0.08;
        let position = orbit_position(Vec3::ZERO, radius, height, phase);
        let orbit = OrbitingLight {
            center: Vec3::ZERO,
            radius,
            height,
            phase,
            speed: 0.18 + index as f32 * 0.012,
        };
        let casts_shadow = matches!(index, 0 | 4);

        let mut light_entity = commands.spawn((
            Name::new(format!("OrbitLight{index}")),
            LevelEntity,
            PointLight {
                color,
                intensity: 85_000.0,
                range: 3.0,
                radius: 0.035,
                shadows_enabled: casts_shadow,
                ..default()
            },
            orbit,
            Transform::from_translation(position),
        ));
        if casts_shadow {
            light_entity.insert(crate::shadow_motion_policy::LightShadowMotionPolicy::automatic());
        }

        commands.spawn((
            Name::new(format!("OrbitLightMarker{index}")),
            LevelEntity,
            Mesh3d(light_marker_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                emissive,
                perceptual_roughness: 0.25,
                ..default()
            })),
            NotShadowCaster,
            orbit,
            Transform::from_translation(position),
        ));
    }

    let _ = spawn_level_camera(launch_mode, None, commands);
}

pub(super) fn spawn_empty(launch_mode: LaunchMode, commands: &mut Commands) -> Option<Entity> {
    spawn_level_camera(launch_mode, None, commands)
}

fn spawn_level_camera(
    launch_mode: LaunchMode,
    fog: Option<DistanceFog>,
    commands: &mut Commands,
) -> Option<Entity> {
    #[cfg(target_os = "android")]
    if launch_mode == LaunchMode::Xr {
        return None;
    }

    let camera_name = match launch_mode {
        LaunchMode::Desktop => "DesktopCamera",
        LaunchMode::Xr => "MirrorCamera",
    };
    let mut camera = commands.spawn((
        Name::new(camera_name),
        LevelEntity,
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    if let Some(fog) = fog {
        camera.insert(fog);
    }

    match launch_mode {
        LaunchMode::Desktop => {
            camera.insert(DesktopLevelPlayer::default());
        }
        LaunchMode::Xr => {
            camera.insert(MirrorCamera);
        }
    }

    Some(camera.id())
}

fn fog_pyramid_fog() -> DistanceFog {
    DistanceFog {
        color: Color::srgb(0.25, 0.25, 0.25),
        falloff: FogFalloff::Linear {
            start: 5.0,
            end: 20.0,
        },
        ..default()
    }
}

fn orbit_position(center: Vec3, radius: f32, height: f32, angle: f32) -> Vec3 {
    center + Vec3::new(angle.cos() * radius, height, angle.sin() * radius)
}

pub(super) fn animate_orbiting_lights(
    time: Res<Time>,
    mut query: Query<(&OrbitingLight, &mut Transform)>,
) {
    let seconds = time.elapsed_secs();
    for (light, mut transform) in &mut query {
        let angle = light.phase + seconds * light.speed;
        transform.translation = orbit_position(light.center, light.radius, light.height, angle);
    }
}

pub(super) fn animate_shadow_test_caster(
    time: Res<Time>,
    mut casters: Query<(&MovingShadowTestCaster, &mut Transform)>,
) {
    let seconds = time.elapsed_secs();
    for (caster, mut transform) in &mut casters {
        transform.translation = caster.base_translation
            + Vec3::new(
                (seconds * 0.9).sin() * 0.45,
                (seconds * 1.8).sin() * 0.08,
                0.0,
            );
        transform.rotation = Quat::from_rotation_y(seconds * 0.65);
    }
}

pub(super) fn move_xr_level_player(
    current_level: Res<CurrentLevel>,
    input_state: Res<EngineInputState>,
    time: Res<Time>,
    mut tracking_roots: Query<&mut Transform, With<XrTrackingRoot>>,
    xr_cameras: Query<(&XrCamera, &Transform), Without<XrTrackingRoot>>,
) {
    if !matches!(
        current_level.0.as_ref(),
        Some(LevelId::FogPyramid | LevelId::PerformanceLab | LevelId::Asset(_))
    ) {
        return;
    }

    let input_axis = input_state.axis2(InputAxis2::Move);
    if input_axis.length_squared() <= f32::EPSILON {
        return;
    }

    let Some((_, camera_transform)) = xr_cameras.iter().min_by_key(|(xr_camera, _)| xr_camera.0)
    else {
        return;
    };

    let Ok(mut tracking_root) = tracking_roots.single_mut() else {
        return;
    };
    let camera_world_rotation = tracking_root.rotation * camera_transform.rotation;
    let movement = camera_relative_movement(camera_world_rotation, input_axis);
    tracking_root.translation += movement * XR_LEVEL_PLAYER_MOVE_SPEED * time.delta_secs();
}

fn camera_relative_movement(rotation: Quat, input_axis: Vec2) -> Vec3 {
    let forward = rotation.mul_vec3(Vec3::NEG_Z);
    let right = rotation.mul_vec3(Vec3::X);

    (right * input_axis.x + forward * input_axis.y).normalize_or_zero()
}
