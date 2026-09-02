use bevy::{
    pbr::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};

use crate::{
    app::LaunchMode,
    shadow_motion_policy::{
        LightShadowMotionClass, LightShadowMotionPolicy, ShadowCasterMotionClass,
        ShadowCasterMotionPolicy,
    },
};

use super::{LevelEntity, levels};

pub(super) const MAX_LIGHT_COUNT: usize = 64;
const GRID_SIDE: usize = 8;
const DYNAMIC_CASTER_COUNT: usize = 4;

const GRID_SPACING_X_M: f32 = 1.15;
const GRID_SPACING_Z_M: f32 = 1.00;
const GRID_FRONT_Z_M: f32 = -1.35;
const ROOM_HALF_WIDTH_M: f32 = 5.15;
const ROOM_BACK_Z_M: f32 = -9.25;
const ROOM_HEIGHT_M: f32 = 5.5;

#[derive(Component, Clone, Copy, Debug)]
pub(super) struct ShadowMotionLabLightMotion {
    center: Vec3,
    horizontal_amplitude_m: f32,
    vertical_amplitude_m: f32,
    depth_amplitude_m: f32,
    angular_speed: f32,
    phase: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub(super) struct ShadowMotionLabDynamicCaster {
    center: Vec3,
    horizontal_amplitude_m: f32,
    vertical_amplitude_m: f32,
    depth_amplitude_m: f32,
    angular_speed: f32,
    phase: f32,
}

#[derive(Component)]
struct ShadowMotionLabStaticGeometry;

pub(super) fn spawn(
    launch_mode: LaunchMode,
    requested_light_count: usize,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let light_count = requested_light_count.clamp(1, MAX_LIGHT_COUNT);
    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let static_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.19, 0.205, 0.22),
        metallic: 0.0,
        perceptual_roughness: 0.78,
        ..default()
    });
    let accent_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.34, 0.30, 0.27),
        metallic: 0.05,
        perceptual_roughness: 0.64,
        ..default()
    });

    spawn_static_box(
        commands,
        &unit_cube,
        &static_material,
        "ShadowLabFloor",
        Vec3::new(0.0, -0.10, -3.75),
        Vec3::new(ROOM_HALF_WIDTH_M * 2.0, 0.20, 11.0),
    );
    spawn_static_box(
        commands,
        &unit_cube,
        &static_material,
        "ShadowLabBackWall",
        Vec3::new(0.0, ROOM_HEIGHT_M * 0.5, ROOM_BACK_Z_M),
        Vec3::new(ROOM_HALF_WIDTH_M * 2.0, ROOM_HEIGHT_M, 0.20),
    );
    for (name, x) in [
        ("ShadowLabLeftWall", -ROOM_HALF_WIDTH_M),
        ("ShadowLabRightWall", ROOM_HALF_WIDTH_M),
    ] {
        spawn_static_box(
            commands,
            &unit_cube,
            &static_material,
            name,
            Vec3::new(x, ROOM_HEIGHT_M * 0.5, -3.75),
            Vec3::new(0.20, ROOM_HEIGHT_M, 11.0),
        );
    }
    spawn_static_box(
        commands,
        &unit_cube,
        &static_material,
        "ShadowLabCeiling",
        Vec3::new(0.0, ROOM_HEIGHT_M, -3.75),
        Vec3::new(ROOM_HALF_WIDTH_M * 2.0, 0.20, 11.0),
    );

    // A fixed, symmetric occluder set gives every profile the same static
    // shadow workload and makes snapshot motion visible on both floor and wall.
    for row in 0..2 {
        for column in 0..4 {
            let x = -3.3 + column as f32 * 2.2;
            let z = -3.0 - row as f32 * 3.2;
            spawn_static_box(
                commands,
                &unit_cube,
                &accent_material,
                "ShadowLabStaticPillar",
                Vec3::new(x, 1.15, z),
                Vec3::new(0.34, 2.30, 0.34),
            );
        }
    }

    let light_marker_mesh = meshes.add(Sphere::new(0.055).mesh().ico(1).unwrap());
    let light_marker_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.82, 0.56),
        emissive: LinearRgba::rgb(7.0, 4.8, 2.4),
        metallic: 0.0,
        perceptual_roughness: 0.25,
        ..default()
    });

    for grid_index in profile_grid_indices(light_count) {
        let motion = light_motion_for_grid_index(grid_index);
        let position = light_translation(motion, 0.0);
        let color = lab_light_color(grid_index);

        commands.spawn((
            Name::new(format!("ShadowLabSlowLight{grid_index:02}")),
            LevelEntity,
            PointLight {
                color,
                intensity: 15_000.0,
                range: 2.65,
                radius: 0.045,
                shadows_enabled: true,
                shadow_map_near_z: 0.08,
                ..default()
            },
            LightShadowMotionPolicy::fixed(LightShadowMotionClass::SlowMoving),
            motion,
            Transform::from_translation(position),
        ));

        commands.spawn((
            Name::new(format!("ShadowLabLightMarker{grid_index:02}")),
            LevelEntity,
            Mesh3d(light_marker_mesh.clone()),
            MeshMaterial3d(light_marker_material.clone()),
            NotShadowCaster,
            NotShadowReceiver,
            motion,
            Transform::from_translation(position),
        ));
    }

    let caster_mesh = meshes.add(Sphere::new(0.30).mesh().ico(2).unwrap());
    let caster_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.74, 0.78),
        metallic: 0.15,
        perceptual_roughness: 0.42,
        ..default()
    });
    for index in 0..DYNAMIC_CASTER_COUNT {
        let motion = dynamic_caster_motion(index);
        commands.spawn((
            Name::new(format!("ShadowLabDynamicCaster{index}")),
            LevelEntity,
            Mesh3d(caster_mesh.clone()),
            MeshMaterial3d(caster_material.clone()),
            ShadowCasterMotionPolicy::fixed(ShadowCasterMotionClass::DynamicOverlay),
            motion,
            Transform::from_translation(dynamic_caster_translation(motion, 0.0)),
        ));
    }

    if let Some(camera) = levels::spawn_empty(launch_mode, commands) {
        commands.entity(camera).insert((
            Transform::from_xyz(0.0, 2.8, 5.2).looking_at(Vec3::new(0.0, 1.5, -4.2), Vec3::Y),
            AmbientLight {
                color: Color::WHITE,
                brightness: 8.0,
                affects_lightmapped_meshes: true,
            },
        ));
    }

    info!(
        "Spawned ShadowMotionLab: {light_count} fixed SlowMoving shadowed PointLights, {DYNAMIC_CASTER_COUNT} fixed DynamicOverlay casters, nested 8x8 profile"
    );
}

fn spawn_static_box(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
    name: &'static str,
    translation: Vec3,
    size: Vec3,
) {
    commands.spawn((
        Name::new(name),
        LevelEntity,
        ShadowMotionLabStaticGeometry,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        ShadowCasterMotionPolicy::fixed(ShadowCasterMotionClass::Static),
        Transform::from_translation(translation).with_scale(size),
    ));
}

fn profile_grid_indices(light_count: usize) -> Vec<usize> {
    let mut indices = (0..MAX_LIGHT_COUNT).collect::<Vec<_>>();
    indices.sort_unstable_by_key(|index| {
        let column = index % GRID_SIDE;
        let row = index / GRID_SIDE;
        let tier = if column % 2 == 0 && row % 2 == 0 {
            0
        } else if (column + row) % 2 == 0 {
            1
        } else {
            2
        };
        (tier, *index)
    });
    indices.truncate(light_count.clamp(1, MAX_LIGHT_COUNT));
    indices
}

fn light_motion_for_grid_index(grid_index: usize) -> ShadowMotionLabLightMotion {
    let column = grid_index % GRID_SIDE;
    let row = grid_index / GRID_SIDE;
    let center = Vec3::new(
        (column as f32 - 3.5) * GRID_SPACING_X_M,
        1.25 + (grid_index % 3) as f32 * 0.22,
        GRID_FRONT_Z_M - row as f32 * GRID_SPACING_Z_M,
    );
    ShadowMotionLabLightMotion {
        center,
        horizontal_amplitude_m: 0.075 + (grid_index % 3) as f32 * 0.008,
        vertical_amplitude_m: 0.028 + (grid_index % 2) as f32 * 0.006,
        depth_amplitude_m: 0.055 + (grid_index % 4) as f32 * 0.006,
        angular_speed: 0.58 + (grid_index % 5) as f32 * 0.035,
        phase: (grid_index as f32 * 2.399_963_1) % std::f32::consts::TAU,
    }
}

fn light_translation(motion: ShadowMotionLabLightMotion, seconds: f32) -> Vec3 {
    let angle = motion.phase + seconds * motion.angular_speed;
    motion.center
        + Vec3::new(
            angle.sin() * motion.horizontal_amplitude_m,
            (angle * 0.73 + motion.phase * 0.7).sin() * motion.vertical_amplitude_m,
            (angle * 0.61 + motion.phase * 0.37).cos() * motion.depth_amplitude_m,
        )
}

fn dynamic_caster_motion(index: usize) -> ShadowMotionLabDynamicCaster {
    ShadowMotionLabDynamicCaster {
        center: Vec3::new(-2.7 + index as f32 * 1.8, 0.55, -4.7),
        horizontal_amplitude_m: 0.48,
        vertical_amplitude_m: 0.22,
        depth_amplitude_m: 0.75,
        angular_speed: 0.52 + index as f32 * 0.045,
        phase: index as f32 * std::f32::consts::FRAC_PI_2,
    }
}

fn dynamic_caster_translation(motion: ShadowMotionLabDynamicCaster, seconds: f32) -> Vec3 {
    let angle = motion.phase + seconds * motion.angular_speed;
    motion.center
        + Vec3::new(
            angle.sin() * motion.horizontal_amplitude_m,
            (angle * 1.37).sin().abs() * motion.vertical_amplitude_m,
            (angle * 0.83).cos() * motion.depth_amplitude_m,
        )
}

fn lab_light_color(grid_index: usize) -> Color {
    const COLORS: [[f32; 3]; 8] = [
        [1.00, 0.46, 0.30],
        [1.00, 0.70, 0.32],
        [0.82, 0.92, 0.40],
        [0.42, 0.90, 0.58],
        [0.35, 0.80, 0.96],
        [0.43, 0.58, 1.00],
        [0.72, 0.48, 1.00],
        [1.00, 0.42, 0.72],
    ];
    let [r, g, b] = COLORS[grid_index % COLORS.len()];
    Color::srgb(r, g, b)
}

pub(super) fn animate_lights(
    time: Res<Time>,
    mut lights_and_markers: Query<(&ShadowMotionLabLightMotion, &mut Transform)>,
) {
    let seconds = time.elapsed_secs();
    for (motion, mut transform) in &mut lights_and_markers {
        transform.translation = light_translation(*motion, seconds);
    }
}

pub(super) fn animate_dynamic_casters(
    time: Res<Time>,
    mut casters: Query<(&ShadowMotionLabDynamicCaster, &mut Transform)>,
) {
    let seconds = time.elapsed_secs();
    for (motion, mut transform) in &mut casters {
        transform.translation = dynamic_caster_translation(*motion, seconds);
        transform.rotation = Quat::from_euler(
            EulerRot::YXZ,
            seconds * (0.35 + motion.phase * 0.02),
            seconds * 0.21,
            0.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::shadow_motion_policy::{LightShadowMotionMode, ShadowCasterMotionMode};

    #[test]
    fn standard_profiles_are_unique_nested_and_spatially_distributed() {
        let sixteen = profile_grid_indices(16);
        let thirty_two = profile_grid_indices(32);
        let sixty_four = profile_grid_indices(64);
        let sixteen_set = sixteen.iter().copied().collect::<HashSet<_>>();
        let thirty_two_set = thirty_two.iter().copied().collect::<HashSet<_>>();
        let sixty_four_set = sixty_four.iter().copied().collect::<HashSet<_>>();

        assert_eq!(sixteen_set.len(), 16);
        assert_eq!(thirty_two_set.len(), 32);
        assert_eq!(sixty_four_set.len(), 64);
        assert!(sixteen_set.is_subset(&thirty_two_set));
        assert!(thirty_two_set.is_subset(&sixty_four_set));
        assert!(sixteen.iter().all(|index| {
            let column = index % GRID_SIDE;
            let row = index / GRID_SIDE;
            column % 2 == 0 && row % 2 == 0
        }));
    }

    #[test]
    fn deterministic_light_motion_crosses_the_snapshot_distance() {
        let motion = light_motion_for_grid_index(17);
        assert_eq!(
            light_translation(motion, 0.0),
            light_translation(motion, 0.0)
        );
        assert!(light_translation(motion, 1.5).distance(light_translation(motion, 0.0)) > 0.04);
    }

    #[test]
    fn lab_spawns_requested_slow_lights_and_dynamic_overlay_casters() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Startup, spawn_test_lab);
        app.update();

        let mut lights = app
            .world_mut()
            .query::<(&PointLight, &LightShadowMotionPolicy)>();
        let lights = lights.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(lights.len(), 16);
        assert!(lights.iter().all(|(light, policy)| {
            light.shadows_enabled && policy.mode == LightShadowMotionMode::SlowMoving
        }));

        let mut casters = app
            .world_mut()
            .query::<(&ShadowMotionLabDynamicCaster, &ShadowCasterMotionPolicy)>();
        let casters = casters.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(casters.len(), DYNAMIC_CASTER_COUNT);
        assert!(
            casters
                .iter()
                .all(|(_, policy)| policy.mode == ShadowCasterMotionMode::DynamicOverlay)
        );
    }

    fn spawn_test_lab(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        spawn(
            LaunchMode::Desktop,
            16,
            &mut commands,
            &mut meshes,
            &mut materials,
        );
    }
}
