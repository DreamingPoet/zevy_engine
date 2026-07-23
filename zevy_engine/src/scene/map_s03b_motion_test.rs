//! Runtime-only motion harness for Map_S03B.
//!
//! None of these entities are part of the UE export or the imported level
//! hierarchy. The imported map remains static scene data. This harness derives
//! two patrol lanes from the imported movable-light positions, moves one shadow
//! caster ball along each lane, and makes a real dynamic PointLight orbit each
//! ball. It exercises moving-light shadows and the dynamic-caster overlay
//! without adding calibration floors or walls to the authored scene.

use bevy::{
    pbr::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};

use crate::shadow_motion_policy::{LightShadowMotionPolicy, ShadowCasterMotionPolicy};

use super::{CurrentLevel, ImportedZevyLight, LevelEntity, LevelId, is_map_s03b_asset};

const FLYING_LIGHT_INTENSITY_LM: f32 = 150_000.0;
const FLYING_LIGHT_RANGE_M: f32 = 12.0;
const FLYING_LIGHT_RADIUS_M: f32 = 0.08;
const FLYING_LIGHT_MARKER_RADIUS_M: f32 = 0.14;
const FLYING_SHADOW_BALL_RADIUS_M: f32 = 0.38;

const PATROL_SECONDS_PER_SEGMENT: f32 = 3.8;
const PATROL_MIN_LANE_DISTANCE_M: f32 = 2.5;
const PATROL_INWARD_OFFSET_M: f32 = 1.6;
const PATROL_BELOW_LIGHT_M: f32 = 1.15;
const PATROL_BOB_AMPLITUDE_M: f32 = 0.16;
const PATROL_BOB_ANGULAR_SPEED: f32 = 1.35;

#[derive(Component)]
pub(super) struct MapS03BFlyingShadowTestRoot;

#[derive(Component)]
pub(super) struct MapS03BFlyingPointLight;

#[derive(Component)]
pub(super) struct MapS03BFlyingShadowBall;

/// A closed, deterministic route built from the current level's imported
/// movable PointLight positions. Smoothstep interpolation deliberately stays
/// inside each waypoint segment instead of overshooting into nearby walls.
#[derive(Component, Clone, Debug)]
pub(super) struct SceneLightPatrolPath {
    waypoints: Vec<Vec3>,
    seconds_per_segment: f32,
    phase_segments: f32,
    bob_amplitude: f32,
    bob_angular_speed: f32,
}

impl SceneLightPatrolPath {
    fn sample(&self, seconds: f32) -> Vec3 {
        debug_assert!(self.waypoints.len() >= 2);
        let segment_phase = seconds / self.seconds_per_segment + self.phase_segments;
        let segment_floor = segment_phase.floor();
        let segment_index = (segment_floor as i64).rem_euclid(self.waypoints.len() as i64) as usize;
        let next_index = (segment_index + 1) % self.waypoints.len();
        let linear_t = segment_phase - segment_floor;
        let smooth_t = linear_t * linear_t * (3.0 - 2.0 * linear_t);
        let mut position = self.waypoints[segment_index].lerp(self.waypoints[next_index], smooth_t);
        position.y +=
            self.bob_amplitude * (seconds * self.bob_angular_speed + self.phase_segments).sin();
        position
    }
}

/// Local-space orbit around a moving ball. Since the PointLight is parented to
/// the ball, this one transform supplies both its orbit and its translation
/// through the level.
#[derive(Component, Clone, Copy, Debug)]
pub(super) struct OrbitAroundBall {
    radius: f32,
    height: f32,
    vertical_amplitude: f32,
    angular_speed: f32,
    phase: f32,
}

impl OrbitAroundBall {
    fn sample(self, seconds: f32) -> Vec3 {
        let angle = seconds * self.angular_speed + self.phase;
        Vec3::new(
            self.radius * angle.cos(),
            self.height + self.vertical_amplitude * (angle * 1.7 + self.phase).sin(),
            self.radius * angle.sin(),
        )
    }
}

#[derive(Clone, Copy)]
struct FlyingPairSpec {
    ball_name: &'static str,
    light_name: &'static str,
    marker_name: &'static str,
    light_color: Color,
    ball_color: Color,
    path_index: usize,
    patrol_phase_segments: f32,
    orbit: OrbitAroundBall,
}

pub(super) fn sync_flying_shadow_test(
    current_level: Res<CurrentLevel>,
    existing: Query<(), With<MapS03BFlyingShadowTestRoot>>,
    imported_point_lights: Query<(&GlobalTransform, &ImportedZevyLight), With<PointLight>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let use_harness = current_level
        .0
        .as_ref()
        .is_some_and(|level| matches!(level, LevelId::Asset(path) if is_map_s03b_asset(path)));
    if !use_harness || !existing.is_empty() {
        return;
    }

    // Imported glTF lights arrive asynchronously. Waiting for both wall lanes
    // also avoids sampling their default GlobalTransform before propagation.
    let movable_scene_light_positions = imported_point_lights
        .iter()
        .filter(|(_, imported)| !imported.source.unreal.is_static_mobility())
        .map(|(transform, _)| transform.translation())
        .filter(|position| position.is_finite())
        .collect::<Vec<_>>();
    let Some(patrol_paths) =
        build_scene_light_patrol_paths(movable_scene_light_positions.iter().copied())
    else {
        return;
    };

    let root = commands
        .spawn((
            Name::new("MapS03BFlyingShadowTest"),
            LevelEntity,
            MapS03BFlyingShadowTestRoot,
            Transform::default(),
            Visibility::Inherited,
        ))
        .id();
    let light_marker_mesh = meshes.add(
        Sphere::new(FLYING_LIGHT_MARKER_RADIUS_M)
            .mesh()
            .ico(2)
            .expect("the fixed flying-light marker subdivision is valid"),
    );
    let shadow_ball_mesh = meshes.add(
        Sphere::new(FLYING_SHADOW_BALL_RADIUS_M)
            .mesh()
            .ico(3)
            .expect("the fixed flying-ball subdivision is valid"),
    );

    for spec in flying_pair_specs() {
        let mut patrol_path = patrol_paths[spec.path_index].clone();
        patrol_path.phase_segments = spec.patrol_phase_segments;
        let initial_ball_position = patrol_path.sample(0.0);
        let ball_material = materials.add(StandardMaterial {
            base_color: spec.ball_color,
            metallic: 0.05,
            perceptual_roughness: 0.38,
            ..default()
        });
        let ball = commands
            .spawn((
                Name::new(spec.ball_name),
                MapS03BFlyingShadowBall,
                ShadowCasterMotionPolicy::automatic(),
                Mesh3d(shadow_ball_mesh.clone()),
                MeshMaterial3d(ball_material),
                patrol_path,
                Transform::from_translation(initial_ball_position),
                ChildOf(root),
            ))
            .id();

        let light = commands
            .spawn((
                Name::new(spec.light_name),
                MapS03BFlyingPointLight,
                PointLight {
                    color: spec.light_color,
                    intensity: FLYING_LIGHT_INTENSITY_LM,
                    range: FLYING_LIGHT_RANGE_M,
                    radius: FLYING_LIGHT_RADIUS_M,
                    shadows_enabled: true,
                    shadow_depth_bias: 0.04,
                    shadow_normal_bias: 0.35,
                    ..default()
                },
                spec.orbit,
                LightShadowMotionPolicy::automatic(),
                Transform::from_translation(spec.orbit.sample(0.0)),
                ChildOf(ball),
            ))
            .id();

        let emissive = LinearRgba::from(spec.light_color) * 80.0;
        let marker_material = materials.add(StandardMaterial {
            base_color: spec.light_color,
            emissive,
            emissive_exposure_weight: 0.0,
            perceptual_roughness: 1.0,
            unlit: true,
            ..default()
        });
        commands.spawn((
            Name::new(spec.marker_name),
            Mesh3d(light_marker_mesh.clone()),
            MeshMaterial3d(marker_material),
            NotShadowCaster,
            NotShadowReceiver,
            Transform::default(),
            ChildOf(light),
        ));
    }

    // The two automatic policies must resolve meter-scale patrol/orbit motion
    // to FullyDynamic / DynamicOverlay. The harness no longer hand-selects
    // cache, jitter, or DynamicShadowCaster implementation markers.
    info!(
        "Spawned Map_S03B scene-light patrol harness: 2 DynamicShadowCaster balls following {} imported movable-light anchors, with one fully dynamic orbiting PointLight each and no calibration geometry",
        movable_scene_light_positions.len(),
    );
}

pub(super) fn animate_flying_shadow_test(
    time: Res<Time>,
    mut balls: Query<
        (&SceneLightPatrolPath, &mut Transform),
        (
            With<MapS03BFlyingShadowBall>,
            Without<MapS03BFlyingPointLight>,
        ),
    >,
    mut lights: Query<
        (&OrbitAroundBall, &mut Transform),
        (
            With<MapS03BFlyingPointLight>,
            Without<MapS03BFlyingShadowBall>,
        ),
    >,
) {
    let seconds = time.elapsed_secs();
    for (path, mut transform) in &mut balls {
        transform.translation = path.sample(seconds);
    }
    for (orbit, mut transform) in &mut lights {
        let angle = seconds * orbit.angular_speed + orbit.phase;
        transform.translation = orbit.sample(seconds);
        transform.rotation = Quat::from_rotation_y(-angle);
    }
}

fn build_scene_light_patrol_paths(
    positions: impl IntoIterator<Item = Vec3>,
) -> Option<[SceneLightPatrolPath; 2]> {
    let mut positive_lane = Vec::new();
    let mut negative_lane = Vec::new();
    for position in positions {
        if !position.is_finite() || position.z.abs() < PATROL_MIN_LANE_DISTANCE_M {
            continue;
        }
        if position.z > 0.0 {
            positive_lane.push(position);
        } else {
            negative_lane.push(position);
        }
    }

    Some([
        build_patrol_lane(positive_lane)?,
        build_patrol_lane(negative_lane)?,
    ])
}

fn build_patrol_lane(mut scene_light_positions: Vec<Vec3>) -> Option<SceneLightPatrolPath> {
    scene_light_positions.sort_by(|left, right| left.x.total_cmp(&right.x));
    scene_light_positions.dedup_by(|left, right| left.distance_squared(*right) < 0.04);
    if scene_light_positions.len() < 3 {
        return None;
    }

    let mut forward = scene_light_positions
        .into_iter()
        .map(|mut position| {
            position.y -= PATROL_BELOW_LIGHT_M;
            position.z -= position.z.signum() * PATROL_INWARD_OFFSET_M;
            position
        })
        .collect::<Vec<_>>();

    // Return along the same authored lights instead of closing the loop with a
    // long diagonal jump from one end of the room to the other.
    let reverse = forward[1..forward.len() - 1]
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();
    forward.extend(reverse);

    Some(SceneLightPatrolPath {
        waypoints: forward,
        seconds_per_segment: PATROL_SECONDS_PER_SEGMENT,
        phase_segments: 0.0,
        bob_amplitude: PATROL_BOB_AMPLITUDE_M,
        bob_angular_speed: PATROL_BOB_ANGULAR_SPEED,
    })
}

fn flying_pair_specs() -> [FlyingPairSpec; 2] {
    [
        FlyingPairSpec {
            ball_name: "MapS03BGreenOrbitShadowBall",
            light_name: "MapS03BFlyingGreenLight",
            marker_name: "MapS03BFlyingGreenLightMarker",
            light_color: Color::srgb(0.10, 1.0, 0.20),
            ball_color: Color::srgb(0.72, 0.76, 0.82),
            path_index: 0,
            patrol_phase_segments: 0.0,
            orbit: OrbitAroundBall {
                radius: 1.25,
                height: 0.85,
                vertical_amplitude: 0.32,
                angular_speed: 0.78,
                phase: 0.0,
            },
        },
        FlyingPairSpec {
            ball_name: "MapS03BYellowOrbitShadowBall",
            light_name: "MapS03BFlyingYellowLight",
            marker_name: "MapS03BFlyingYellowLightMarker",
            light_color: Color::srgb(1.0, 0.78, 0.08),
            ball_color: Color::srgb(0.72, 0.30, 0.16),
            path_index: 1,
            patrol_phase_segments: 2.35,
            orbit: OrbitAroundBall {
                radius: 1.35,
                height: 0.95,
                vertical_amplitude: 0.38,
                angular_speed: -0.67,
                phase: 1.70,
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        scene::{
            ZevyBevyLightParameters, ZevyLightDefinition, ZevyLightKind, ZevyUnrealLightParameters,
        },
        shadow_cache::CachedPointLightShadow,
        shadow_overlay::DynamicShadowCaster,
    };
    use bevy::pbr::PointLightShadowMapJitter;

    fn scene_light_fixture() -> Vec<Vec3> {
        vec![
            Vec3::new(76.9, -28.4, 7.3),
            Vec3::new(91.7, -25.3, 7.1),
            Vec3::new(106.8, -21.7, 7.1),
            Vec3::new(121.7, -18.4, 7.1),
            Vec3::new(76.9, -28.4, -7.4),
            Vec3::new(91.7, -25.3, -7.6),
            Vec3::new(106.8, -21.7, -7.6),
            Vec3::new(121.7, -18.4, -7.6),
        ]
    }

    fn movable_imported_light(position: Vec3, index: usize) -> impl Bundle {
        (
            PointLight::default(),
            GlobalTransform::from(Transform::from_translation(position)),
            ImportedZevyLight {
                source: ZevyLightDefinition {
                    component_name: format!("FixtureLight{index}"),
                    gltf_name: format!("FixtureLightNode{index}"),
                    kind: ZevyLightKind::Point,
                    bevy: ZevyBevyLightParameters::default(),
                    unreal: ZevyUnrealLightParameters {
                        mobility: "movable".to_string(),
                        ..default()
                    },
                },
            },
        )
    }

    #[test]
    fn patrol_paths_are_derived_from_both_scene_light_lanes() {
        let source = scene_light_fixture();
        let paths = build_scene_light_patrol_paths(source.iter().copied()).unwrap();

        assert_eq!(paths[0].waypoints.len(), 6);
        assert_eq!(paths[1].waypoints.len(), 6);
        for point in &paths[0].waypoints {
            assert!(point.z > 0.0);
            assert!(source.iter().any(|light| {
                (point.x - light.x).abs() < 0.001
                    && (point.y - (light.y - PATROL_BELOW_LIGHT_M)).abs() < 0.001
                    && (point.z - (light.z - PATROL_INWARD_OFFSET_M)).abs() < 0.001
            }));
        }
        for point in &paths[1].waypoints {
            assert!(point.z < 0.0);
            assert!(source.iter().any(|light| {
                (point.x - light.x).abs() < 0.001
                    && (point.y - (light.y - PATROL_BELOW_LIGHT_M)).abs() < 0.001
                    && (point.z - (light.z + PATROL_INWARD_OFFSET_M)).abs() < 0.001
            }));
        }
    }

    #[test]
    fn patrol_samples_remain_inside_authored_segments() {
        let paths = build_scene_light_patrol_paths(scene_light_fixture()).unwrap();
        for path in paths {
            for sample in 0..2_000 {
                let seconds = sample as f32 * 0.03125;
                let position = path.sample(seconds);
                let bob = path.bob_amplitude
                    * (seconds * path.bob_angular_speed + path.phase_segments).sin();
                let unbobbed = position - Vec3::Y * bob;
                let phase = seconds / path.seconds_per_segment + path.phase_segments;
                let index = (phase.floor() as i64).rem_euclid(path.waypoints.len() as i64) as usize;
                let next = (index + 1) % path.waypoints.len();
                let segment = path.waypoints[next] - path.waypoints[index];
                let projection = if segment.length_squared() > 0.0 {
                    (unbobbed - path.waypoints[index]).dot(segment) / segment.length_squared()
                } else {
                    0.0
                };
                assert!((-0.000_01..=1.000_01).contains(&projection));
            }
        }
    }

    #[test]
    fn orbiting_lights_keep_their_horizontal_radius() {
        for spec in flying_pair_specs() {
            for sample in 0..2_000 {
                let local = spec.orbit.sample(sample as f32 * 0.03125);
                let horizontal_radius = Vec2::new(local.x, local.z).length();
                assert!((horizontal_radius - spec.orbit.radius).abs() < 0.000_01);
                assert!(
                    (local.y - spec.orbit.height).abs() <= spec.orbit.vertical_amplitude + 0.000_01
                );
            }
        }
    }

    #[test]
    fn map_harness_spawns_two_orbiting_lights_and_two_overlay_casters() {
        let mut app = App::new();
        app.insert_resource(CurrentLevel(Some(LevelId::asset(
            super::super::MAP_S03B_ASSET_PATH,
        ))))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Time>()
        .add_plugins(crate::shadow_motion_policy::ShadowMotionPolicyPlugin)
        .add_systems(Update, sync_flying_shadow_test);
        for (index, position) in scene_light_fixture().into_iter().enumerate() {
            app.world_mut()
                .spawn(movable_imported_light(position, index));
        }

        app.update();

        let world = app.world_mut();
        let lights = world
            .query_filtered::<Entity, With<MapS03BFlyingPointLight>>()
            .iter(world)
            .collect::<Vec<_>>();
        let balls = world
            .query_filtered::<Entity, With<MapS03BFlyingShadowBall>>()
            .iter(world)
            .collect::<Vec<_>>();
        assert_eq!(lights.len(), 2);
        assert_eq!(balls.len(), 2);
        assert_eq!(world.query::<&Mesh3d>().iter(world).count(), 4);
        let root = world
            .query_filtered::<Entity, With<MapS03BFlyingShadowTestRoot>>()
            .single(world)
            .unwrap();
        assert!(world.entity(root).contains::<InheritedVisibility>());

        for entity in lights {
            let entity_ref = world.entity(entity);
            let light = entity_ref.get::<PointLight>().unwrap();
            assert!(light.shadows_enabled);
            assert!(!entity_ref.contains::<CachedPointLightShadow>());
            assert!(!entity_ref.contains::<PointLightShadowMapJitter>());
            assert!(!entity_ref.contains::<ImportedZevyLight>());
            assert!(entity_ref.contains::<OrbitAroundBall>());
            assert!(entity_ref.contains::<LightShadowMotionPolicy>());
            let parent = entity_ref.get::<ChildOf>().unwrap().parent();
            assert!(world.entity(parent).contains::<MapS03BFlyingShadowBall>());
        }
        for entity in balls {
            let entity_ref = world.entity(entity);
            assert!(entity_ref.contains::<DynamicShadowCaster>());
            assert!(entity_ref.contains::<ShadowCasterMotionPolicy>());
            assert!(entity_ref.contains::<SceneLightPatrolPath>());
        }
    }

    #[test]
    fn harness_waits_for_scene_lights_and_is_not_injected_into_other_levels() {
        let mut map_app = App::new();
        map_app
            .insert_resource(CurrentLevel(Some(LevelId::asset(
                super::super::MAP_S03B_ASSET_PATH,
            ))))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Time>()
            .add_plugins(crate::shadow_motion_policy::ShadowMotionPolicyPlugin)
            .add_systems(Update, sync_flying_shadow_test);
        map_app.update();
        assert_eq!(
            map_app
                .world_mut()
                .query_filtered::<Entity, With<MapS03BFlyingShadowTestRoot>>()
                .iter(map_app.world())
                .count(),
            0
        );

        let mut other_app = App::new();
        other_app
            .insert_resource(CurrentLevel(Some(LevelId::asset(
                "levels/Other/Other.zevy-level.json",
            ))))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Time>()
            .add_plugins(crate::shadow_motion_policy::ShadowMotionPolicyPlugin)
            .add_systems(Update, sync_flying_shadow_test);
        for (index, position) in scene_light_fixture().into_iter().enumerate() {
            other_app
                .world_mut()
                .spawn(movable_imported_light(position, index));
        }
        other_app.update();
        assert_eq!(
            other_app
                .world_mut()
                .query_filtered::<Entity, With<MapS03BFlyingShadowTestRoot>>()
                .iter(other_app.world())
                .count(),
            0
        );
    }
}
