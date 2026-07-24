//! Map_S03B XR hand-motion shadow harness.
//!
//! This is deliberately scene test content, not imported level data and not a
//! renderer policy special case. It turns valid OpenXR grip poses into one
//! fixed DynamicOverlay caster and one real FullyDynamic SpotLight so the
//! generic motion paths can be observed directly on a headset.

use bevy::prelude::*;
use bevy_mod_xr::spaces::XrSpaceLocationFlags;

use crate::{
    shadow_motion_policy::{
        LightShadowMotionClass, LightShadowMotionPolicy, ShadowCasterMotionClass,
        ShadowCasterMotionPolicy,
    },
    xr::{XrLeftGripPoseAnchor, XrRightGripPoseAnchor},
};

use super::{CurrentLevel, LevelEntity, LevelId, is_map_s03b_asset};

const LEFT_HAND_BALL_RADIUS_M: f32 = 0.04;
const RIGHT_HAND_FLASHLIGHT_INTENSITY_LM: f32 = 520_000.0;
const RIGHT_HAND_FLASHLIGHT_RANGE_M: f32 = 30.0;
const RIGHT_HAND_FLASHLIGHT_INNER_ANGLE_DEGREES: f32 = 16.0;
const RIGHT_HAND_FLASHLIGHT_OUTER_ANGLE_DEGREES: f32 = 28.0;
const RIGHT_HAND_FLASHLIGHT_LOCAL_OFFSET_M: Vec3 = Vec3::new(0.0, 0.0, -0.12);

#[derive(Component)]
pub(super) struct MapS03BLeftHandDynamicOverlayBall;

#[derive(Component)]
pub(super) struct MapS03BRightHandFullyDynamicFlashlight;

#[derive(Default)]
pub(super) struct XrHandShadowTestAssets {
    ball_mesh: Option<Handle<Mesh>>,
    ball_material: Option<Handle<StandardMaterial>>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_xr_hand_shadow_test(
    current_level: Res<CurrentLevel>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut assets: Local<XrHandShadowTestAssets>,
    left_anchors: Query<(Entity, &XrSpaceLocationFlags), With<XrLeftGripPoseAnchor>>,
    right_anchors: Query<(Entity, &XrSpaceLocationFlags), With<XrRightGripPoseAnchor>>,
    left_balls: Query<(Entity, &ChildOf), With<MapS03BLeftHandDynamicOverlayBall>>,
    right_flashlights: Query<(Entity, &ChildOf), With<MapS03BRightHandFullyDynamicFlashlight>>,
) {
    let use_harness = current_level
        .0
        .as_ref()
        .is_some_and(|level| matches!(level, LevelId::Asset(path) if is_map_s03b_asset(path)));
    if !use_harness {
        despawn_all(&mut commands, &left_balls);
        despawn_all(&mut commands, &right_flashlights);
        return;
    }

    let tracked_left = left_anchors
        .iter()
        .find_map(|(entity, flags)| flags.position_tracked.then_some(entity));
    sync_left_hand_ball(
        tracked_left,
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut assets,
        &left_balls,
    );

    // A flashlight needs both a valid position and orientation. A position-only
    // pose can place a sphere correctly but cannot define a trustworthy beam.
    let tracked_right = right_anchors.iter().find_map(|(entity, flags)| {
        (flags.position_tracked && flags.rotation_tracked).then_some(entity)
    });
    sync_right_hand_flashlight(tracked_right, &mut commands, &right_flashlights);
}

fn sync_left_hand_ball(
    tracked_parent: Option<Entity>,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    assets: &mut XrHandShadowTestAssets,
    existing: &Query<(Entity, &ChildOf), With<MapS03BLeftHandDynamicOverlayBall>>,
) {
    let Some(parent) = tracked_parent else {
        despawn_all(commands, existing);
        return;
    };

    let mut attached = false;
    for (entity, child_of) in existing {
        if child_of.parent() == parent && !attached {
            attached = true;
        } else {
            commands.entity(entity).despawn();
        }
    }
    if attached {
        return;
    }

    let mesh = assets
        .ball_mesh
        .get_or_insert_with(|| {
            meshes.add(
                Sphere::new(LEFT_HAND_BALL_RADIUS_M)
                    .mesh()
                    .ico(3)
                    .expect("the fixed XR hand-ball subdivision is valid"),
            )
        })
        .clone();
    let material = assets
        .ball_material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.58, 0.95),
                metallic: 0.05,
                perceptual_roughness: 0.42,
                ..default()
            })
        })
        .clone();

    commands.spawn((
        Name::new("MapS03BLeftHandDynamicOverlayBall"),
        LevelEntity,
        MapS03BLeftHandDynamicOverlayBall,
        ShadowCasterMotionPolicy::fixed(ShadowCasterMotionClass::DynamicOverlay),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        ChildOf(parent),
    ));
    info!("Map_S03B left grip tracked: spawned fixed DynamicOverlay shadow ball");
}

fn sync_right_hand_flashlight(
    tracked_parent: Option<Entity>,
    commands: &mut Commands,
    existing: &Query<(Entity, &ChildOf), With<MapS03BRightHandFullyDynamicFlashlight>>,
) {
    let Some(parent) = tracked_parent else {
        despawn_all(commands, existing);
        return;
    };

    let mut attached = false;
    for (entity, child_of) in existing {
        if child_of.parent() == parent && !attached {
            attached = true;
        } else {
            commands.entity(entity).despawn();
        }
    }
    if attached {
        return;
    }

    commands.spawn((
        Name::new("MapS03BRightHandFullyDynamicFlashlight"),
        LevelEntity,
        MapS03BRightHandFullyDynamicFlashlight,
        SpotLight {
            color: Color::srgb(1.0, 0.94, 0.80),
            intensity: RIGHT_HAND_FLASHLIGHT_INTENSITY_LM,
            range: RIGHT_HAND_FLASHLIGHT_RANGE_M,
            radius: 0.04,
            shadows_enabled: true,
            shadow_depth_bias: 0.03,
            shadow_normal_bias: 0.40,
            inner_angle: RIGHT_HAND_FLASHLIGHT_INNER_ANGLE_DEGREES.to_radians(),
            outer_angle: RIGHT_HAND_FLASHLIGHT_OUTER_ANGLE_DEGREES.to_radians(),
            ..default()
        },
        LightShadowMotionPolicy::fixed(LightShadowMotionClass::FullyDynamic),
        // Bevy SpotLight shines along local -Z, matching OpenXR's forward axis.
        Transform::from_translation(RIGHT_HAND_FLASHLIGHT_LOCAL_OFFSET_M),
        ChildOf(parent),
    ));
    info!("Map_S03B right grip tracked: spawned FullyDynamic shadowed flashlight SpotLight");
}

fn despawn_all<T: Component>(
    commands: &mut Commands,
    entities: &Query<(Entity, &ChildOf), With<T>>,
) {
    for (entity, _) in entities {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        shadow_motion_policy::{
            LightShadowMotionMode, ResolvedLightShadowMotion, ShadowCasterMotionMode,
            ShadowMotionPolicyPlugin,
        },
        shadow_overlay::DynamicShadowCaster,
    };

    #[test]
    fn tracked_hands_spawn_the_requested_shadow_paths_under_the_pose_anchors() {
        let mut app = test_app(LevelId::asset(super::super::MAP_S03B_ASSET_PATH));
        let left = spawn_left_anchor(&mut app, true);
        let right = spawn_right_anchor(&mut app, true, true);

        app.update();

        let world = app.world_mut();
        let ball = world
            .query_filtered::<Entity, With<MapS03BLeftHandDynamicOverlayBall>>()
            .single(world)
            .unwrap();
        let flashlight = world
            .query_filtered::<Entity, With<MapS03BRightHandFullyDynamicFlashlight>>()
            .single(world)
            .unwrap();

        let ball_ref = world.entity(ball);
        assert_eq!(ball_ref.get::<ChildOf>().unwrap().parent(), left);
        assert_eq!(
            ball_ref.get::<ShadowCasterMotionPolicy>().unwrap().mode,
            ShadowCasterMotionMode::DynamicOverlay
        );
        assert!(ball_ref.contains::<DynamicShadowCaster>());

        let flashlight_ref = world.entity(flashlight);
        assert_eq!(flashlight_ref.get::<ChildOf>().unwrap().parent(), right);
        assert_eq!(
            flashlight_ref
                .get::<LightShadowMotionPolicy>()
                .unwrap()
                .mode,
            LightShadowMotionMode::FullyDynamic
        );
        assert_eq!(
            flashlight_ref
                .get::<ResolvedLightShadowMotion>()
                .unwrap()
                .class,
            LightShadowMotionClass::FullyDynamic
        );
        let light = flashlight_ref.get::<SpotLight>().unwrap();
        assert!(light.shadows_enabled);
        assert_eq!(light.range, RIGHT_HAND_FLASHLIGHT_RANGE_M);
    }

    #[test]
    fn tracking_loss_removes_both_runtime_attachments() {
        let mut app = test_app(LevelId::asset(super::super::MAP_S03B_ASSET_PATH));
        let left = spawn_left_anchor(&mut app, true);
        let right = spawn_right_anchor(&mut app, true, true);
        app.update();

        app.world_mut()
            .entity_mut(left)
            .insert(XrSpaceLocationFlags::default());
        app.world_mut()
            .entity_mut(right)
            .insert(XrSpaceLocationFlags::default());
        app.update();

        let world = app.world_mut();
        assert_eq!(
            world
                .query_filtered::<Entity, With<MapS03BLeftHandDynamicOverlayBall>>()
                .iter(world)
                .count(),
            0
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<MapS03BRightHandFullyDynamicFlashlight>>()
                .iter(world)
                .count(),
            0
        );
    }

    #[test]
    fn flashlight_requires_orientation_and_the_harness_is_map_scoped() {
        let mut map_app = test_app(LevelId::asset(super::super::MAP_S03B_ASSET_PATH));
        spawn_left_anchor(&mut map_app, true);
        spawn_right_anchor(&mut map_app, true, false);
        map_app.update();
        assert_eq!(count::<MapS03BLeftHandDynamicOverlayBall>(&mut map_app), 1);
        assert_eq!(
            count::<MapS03BRightHandFullyDynamicFlashlight>(&mut map_app),
            0
        );

        let mut other_app = test_app(LevelId::asset("levels/Other/Other.zevy-level.json"));
        spawn_left_anchor(&mut other_app, true);
        spawn_right_anchor(&mut other_app, true, true);
        other_app.update();
        assert_eq!(
            count::<MapS03BLeftHandDynamicOverlayBall>(&mut other_app),
            0
        );
        assert_eq!(
            count::<MapS03BRightHandFullyDynamicFlashlight>(&mut other_app),
            0
        );
    }

    fn test_app(level: LevelId) -> App {
        let mut app = App::new();
        app.insert_resource(CurrentLevel(Some(level)))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Time>()
            .add_plugins(ShadowMotionPolicyPlugin)
            .add_systems(Update, sync_xr_hand_shadow_test);
        app
    }

    fn spawn_left_anchor(app: &mut App, position_tracked: bool) -> Entity {
        app.world_mut()
            .spawn((
                XrLeftGripPoseAnchor,
                XrSpaceLocationFlags {
                    position_tracked,
                    rotation_tracked: true,
                },
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id()
    }

    fn spawn_right_anchor(app: &mut App, position_tracked: bool, rotation_tracked: bool) -> Entity {
        app.world_mut()
            .spawn((
                XrRightGripPoseAnchor,
                XrSpaceLocationFlags {
                    position_tracked,
                    rotation_tracked,
                },
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id()
    }

    fn count<T: Component>(app: &mut App) -> usize {
        let world = app.world_mut();
        world
            .query_filtered::<Entity, With<T>>()
            .iter(world)
            .count()
    }
}
