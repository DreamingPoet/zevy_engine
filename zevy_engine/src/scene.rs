mod desktop_player;
mod levels;
mod zevy_level;

use std::env;

use bevy::{prelude::*, render::primitives::Aabb};
use bevy_mod_xr::session::XrTrackingRoot;

use crate::{
    app::{LaunchMode, StartupMode},
    input::EngineInputSet,
};

pub use zevy_level::{
    ImportedZevyEntity, ImportedZevyLevel, ImportedZevyLight, ZevyBevyLightParameters,
    ZevyLevelAsset, ZevyLevelAssetLoader, ZevyLevelEntityDefinition, ZevyLevelPlugin,
    ZevyLevelSceneAsset, ZevyLevelTransform, ZevyLightDefinition, ZevyLightKind,
    ZevyUnrealLightParameters, spawn_zevy_level,
};

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ZevyLevelPlugin)
            .insert_resource(DefaultLevel(startup_level_from_args()))
            .insert_resource(CurrentLevel(None))
            .insert_resource(ActiveLevelFog(None))
            .init_resource::<desktop_player::DesktopPlayerCursorState>()
            .add_event::<OpenLevel>()
            .add_systems(Startup, load_default_level)
            .add_systems(
                Update,
                (
                    open_level,
                    apply_active_level_fog_to_cameras,
                    frame_asset_level_camera,
                    apply_map_s03b_lighting_profile.after(frame_asset_level_camera),
                    animate_map_s03b_candle_lights.after(apply_map_s03b_lighting_profile),
                    desktop_player::update_desktop_level_player
                        .after(EngineInputSet::Collect)
                        .after(frame_asset_level_camera),
                    apply_map_s03b_xr_start.after(open_level),
                    levels::move_xr_level_player
                        .after(EngineInputSet::Collect)
                        .after(apply_map_s03b_xr_start),
                    levels::animate_orbiting_lights,
                ),
            );
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelId {
    #[allow(dead_code)]
    FogPyramid,
    #[allow(dead_code)]
    PerformanceLab,
    #[allow(dead_code)]
    Empty,
    Asset(String),
}

impl LevelId {
    pub fn asset(path: impl Into<String>) -> Self {
        Self::Asset(path.into().replace('\\', "/"))
    }
}

#[derive(Resource, Clone, Debug, Eq, PartialEq)]
pub struct DefaultLevel(pub LevelId);

#[derive(Resource, Clone, Debug, Eq, PartialEq)]
pub struct CurrentLevel(pub Option<LevelId>);

#[derive(Resource, Clone, Debug)]
struct ActiveLevelFog(Option<DistanceFog>);

#[derive(Event, Clone, Debug, Eq, PartialEq)]
pub struct OpenLevel(pub LevelId);

#[derive(Component)]
pub(super) struct LevelEntity;

#[derive(Component)]
pub struct MirrorCamera;

const MAP_S03B_ASSET_PATH: &str = "levels/Map_S03B/Map_S03B.zevy-level.json";
const MAP_S03B_PLAYER_START_UE_CM: Vec3 = Vec3::new(12_370.0, -250.0, -2_000.0);
const MAP_S03B_POINT_LIGHT_INTENSITY_SCALE: f32 = 1_000.0;
const MAP_S03B_POINT_LIGHT_RANGE_SCALE: f32 = 4.0;
const MAP_S03B_AMBIENT_BRIGHTNESS: f32 = 20.0;

#[derive(Component)]
struct AssetLevelCamera {
    last_mesh_count: usize,
    stable_frames: u8,
    framed: bool,
    auto_frame: bool,
}

impl Default for AssetLevelCamera {
    fn default() -> Self {
        Self {
            last_mesh_count: 0,
            stable_frames: 0,
            framed: false,
            auto_frame: true,
        }
    }
}

#[derive(Component)]
pub(crate) struct ImportedLevelCameraFramed;

#[derive(Component)]
struct AssetLevelFallbackLight;

#[derive(Component, Clone, Copy)]
struct MapS03BXrStartApplied {
    previous_translation: Vec3,
}

#[derive(Component, Clone, Copy)]
struct MapS03BPointLightTuningApplied {
    previous_intensity: f32,
    previous_range: f32,
    base_intensity: f32,
    base_range: f32,
    flicker_phase: f32,
}

#[derive(Component, Clone)]
struct MapS03BCameraLightingApplied {
    previous_ambient: Option<AmbientLight>,
}

fn load_default_level(
    default_level: Res<DefaultLevel>,
    startup_mode: Res<StartupMode>,
    mut current_level: ResMut<CurrentLevel>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_level(
        default_level.0.clone(),
        startup_mode.0,
        &mut current_level,
        &mut commands,
        &asset_server,
        &mut meshes,
        &mut materials,
    );
}

fn open_level(
    mut events: EventReader<OpenLevel>,
    level_entities: Query<Entity, With<LevelEntity>>,
    startup_mode: Res<StartupMode>,
    mut current_level: ResMut<CurrentLevel>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(level) = events.read().last().map(|event| event.0.clone()) else {
        return;
    };

    if current_level.0.as_ref() == Some(&level) {
        return;
    }

    for entity in &level_entities {
        commands.entity(entity).despawn();
    }

    spawn_level(
        level,
        startup_mode.0,
        &mut current_level,
        &mut commands,
        &asset_server,
        &mut meshes,
        &mut materials,
    );
}

fn spawn_level(
    level: LevelId,
    launch_mode: LaunchMode,
    current_level: &mut ResMut<CurrentLevel>,
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let level_fog = levels::level_fog(&level);
    commands.insert_resource(ActiveLevelFog(level_fog.clone()));

    match &level {
        LevelId::FogPyramid => {
            levels::spawn_fog_pyramid(launch_mode, level_fog, commands, meshes, materials);
        }
        LevelId::PerformanceLab => {
            levels::spawn_performance_lab(launch_mode, commands, meshes, materials);
        }
        LevelId::Empty => {
            let _ = levels::spawn_empty(launch_mode, commands);
        }
        LevelId::Asset(asset_path) => {
            let level_camera = levels::spawn_empty(launch_mode, commands);
            if launch_mode == LaunchMode::Desktop {
                if let Some(camera) = level_camera {
                    let configured_start = map_s03b_player_start(asset_path);
                    commands.entity(camera).insert((
                        AssetLevelCamera {
                            auto_frame: configured_start.is_none(),
                            ..default()
                        },
                        AmbientLight {
                            color: Color::WHITE,
                            brightness: 500.0,
                            affects_lightmapped_meshes: true,
                        },
                    ));
                    if let Some(start) = configured_start {
                        commands.entity(camera).insert(
                            Transform::from_translation(start).looking_to(Vec3::X, Vec3::Y),
                        );
                    }
                }
                commands.spawn((
                    Name::new("AssetLevelFallbackSun"),
                    LevelEntity,
                    AssetLevelFallbackLight,
                    DirectionalLight {
                        illuminance: 25_000.0,
                        shadows_enabled: false,
                        ..default()
                    },
                    Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.8, -0.9, 0.0)),
                ));
            }
            spawn_zevy_level(commands, asset_server, asset_path);
        }
    }

    info!("Opened level: {level:?}");
    current_level.0 = Some(level);
}

fn frame_asset_level_camera(
    mut commands: Commands,
    mut cameras: Query<(
        Entity,
        &mut Transform,
        &mut Projection,
        &mut AssetLevelCamera,
    )>,
    mesh_bounds: Query<(&Aabb, &GlobalTransform), With<Mesh3d>>,
    imported_lights: Query<
        (),
        (
            Or<(With<DirectionalLight>, With<PointLight>, With<SpotLight>)>,
            Without<AssetLevelFallbackLight>,
        ),
    >,
    mut fallback_lights: Query<&mut Visibility, With<AssetLevelFallbackLight>>,
) {
    let mesh_count = mesh_bounds.iter().count();
    if mesh_count == 0 {
        return;
    }

    for (camera_entity, mut camera_transform, mut projection, mut framing) in &mut cameras {
        if framing.framed {
            continue;
        }

        if framing.last_mesh_count == mesh_count {
            framing.stable_frames = framing.stable_frames.saturating_add(1);
        } else {
            framing.last_mesh_count = mesh_count;
            framing.stable_frames = 0;
        }
        if framing.stable_frames < 8 {
            continue;
        }

        let Some((minimum, maximum)) = world_mesh_bounds(&mesh_bounds) else {
            continue;
        };
        let center = (minimum + maximum) * 0.5;
        let half_extents = (maximum - minimum) * 0.5;
        let radius = half_extents.length().max(1.0);
        let mut framing_distance = None;

        if framing.auto_frame {
            let direction = Vec3::new(-1.0, 0.7, 1.0).normalize();
            let fov = match projection.as_ref() {
                Projection::Perspective(perspective) => perspective.fov,
                _ => std::f32::consts::FRAC_PI_4,
            };
            let distance = radius / (fov * 0.5).tan();
            *camera_transform = Transform::from_translation(center + direction * distance)
                .looking_at(center, Vec3::Y);
            framing_distance = Some(distance);
        }

        if let Projection::Perspective(perspective) = projection.as_mut() {
            perspective.near = (radius * 0.001).clamp(0.05, 1.0);
            let distance_to_center = camera_transform.translation.distance(center);
            perspective.far = (distance_to_center + radius * 4.0).max(1000.0);
        }

        let has_imported_lights = !imported_lights.is_empty();
        for mut visibility in &mut fallback_lights {
            *visibility = if has_imported_lights {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }

        framing.framed = true;
        commands
            .entity(camera_entity)
            .insert(ImportedLevelCameraFramed);
        info!(
            "Prepared imported Level camera: meshes={} auto_framed={} position={:?} center={:?} size={:?} framing_distance={:?} imported_lights={}",
            mesh_count,
            framing.auto_frame,
            camera_transform.translation,
            center,
            maximum - minimum,
            framing_distance,
            has_imported_lights,
        );
    }
}

fn apply_map_s03b_xr_start(
    current_level: Res<CurrentLevel>,
    mut commands: Commands,
    mut tracking_roots: Query<
        (Entity, &mut Transform, Option<&MapS03BXrStartApplied>),
        With<XrTrackingRoot>,
    >,
) {
    let use_map_start = current_level
        .0
        .as_ref()
        .is_some_and(|level| matches!(level, LevelId::Asset(path) if is_map_s03b_asset(path)));

    for (entity, mut transform, applied) in &mut tracking_roots {
        if use_map_start && applied.is_none() {
            let previous_translation = transform.translation;
            transform.translation = map_s03b_player_start_bevy();
            commands.entity(entity).insert(MapS03BXrStartApplied {
                previous_translation,
            });
            info!(
                "Applied Map_S03B XR tracking origin: UE cm {:?} -> Bevy m {:?}",
                MAP_S03B_PLAYER_START_UE_CM, transform.translation,
            );
        } else if !use_map_start && let Some(applied) = applied {
            transform.translation = applied.previous_translation;
            commands.entity(entity).remove::<MapS03BXrStartApplied>();
        }
    }
}

fn apply_map_s03b_lighting_profile(
    current_level: Res<CurrentLevel>,
    mut commands: Commands,
    mut point_lights: Query<
        (
            Entity,
            &mut PointLight,
            Option<&MapS03BPointLightTuningApplied>,
        ),
        With<ImportedZevyLight>,
    >,
    mut cameras: Query<
        (
            Entity,
            Option<&mut AmbientLight>,
            Option<&MapS03BCameraLightingApplied>,
        ),
        With<Camera3d>,
    >,
) {
    let use_profile = current_level
        .0
        .as_ref()
        .is_some_and(|level| matches!(level, LevelId::Asset(path) if is_map_s03b_asset(path)));

    for (entity, mut light, applied) in &mut point_lights {
        if use_profile && applied.is_none() {
            let previous_intensity = light.intensity;
            let previous_range = light.range;
            light.intensity *= MAP_S03B_POINT_LIGHT_INTENSITY_SCALE;
            light.range *= MAP_S03B_POINT_LIGHT_RANGE_SCALE;
            let tuning = MapS03BPointLightTuningApplied {
                previous_intensity,
                previous_range,
                base_intensity: light.intensity,
                base_range: light.range,
                flicker_phase: entity.index() as f32 * 1.618_034,
            };
            commands.entity(entity).insert(tuning);
            info!(
                "Applied Map_S03B PointLight tuning: intensity {:.3} -> {:.3} lm, range {:.3} -> {:.3} m",
                tuning.previous_intensity, light.intensity, tuning.previous_range, light.range,
            );
        } else if !use_profile && let Some(applied) = applied {
            light.intensity = applied.previous_intensity;
            light.range = applied.previous_range;
            commands
                .entity(entity)
                .remove::<MapS03BPointLightTuningApplied>();
        }
    }

    for (entity, ambient, applied) in &mut cameras {
        if use_profile && applied.is_none() {
            let previous_ambient = ambient.as_deref().cloned();
            if let Some(mut ambient) = ambient {
                ambient.brightness = MAP_S03B_AMBIENT_BRIGHTNESS;
            } else {
                commands.entity(entity).insert(AmbientLight {
                    color: Color::WHITE,
                    brightness: MAP_S03B_AMBIENT_BRIGHTNESS,
                    affects_lightmapped_meshes: true,
                });
            }
            commands
                .entity(entity)
                .insert(MapS03BCameraLightingApplied { previous_ambient });
        } else if !use_profile && let Some(applied) = applied {
            if let Some(previous_ambient) = applied.previous_ambient.clone() {
                commands.entity(entity).insert(previous_ambient);
            } else {
                commands.entity(entity).remove::<AmbientLight>();
            }
            commands
                .entity(entity)
                .remove::<MapS03BCameraLightingApplied>();
        }
    }
}

fn animate_map_s03b_candle_lights(
    current_level: Res<CurrentLevel>,
    time: Res<Time>,
    mut point_lights: Query<(&mut PointLight, &MapS03BPointLightTuningApplied)>,
) {
    let use_profile = current_level
        .0
        .as_ref()
        .is_some_and(|level| matches!(level, LevelId::Asset(path) if is_map_s03b_asset(path)));
    if !use_profile {
        return;
    }

    let seconds = time.elapsed_secs();
    for (mut light, tuning) in &mut point_lights {
        let intensity_multiplier = candle_flicker_multiplier(seconds, tuning.flicker_phase);
        let range_multiplier = 1.0 + (intensity_multiplier - 1.0) * 0.28;
        light.intensity = tuning.base_intensity * intensity_multiplier;
        light.range = tuning.base_range * range_multiplier;
    }
}

fn candle_flicker_multiplier(seconds: f32, phase: f32) -> f32 {
    let slow = (seconds * 1.9 + phase).sin() * 0.16;
    let medium = (seconds * 5.7 + phase * 1.37).sin() * 0.11;
    let fast = (seconds * 13.1 + phase * 2.11).sin() * 0.055;
    let occasional_dip = (seconds * 3.3 + phase * 0.71).sin().max(0.0).powi(10) * 0.26;
    let brief_flare = (seconds * 9.7 + phase * 1.91).sin().max(0.0).powi(14) * 0.14;
    (1.0 + slow + medium + fast - occasional_dip + brief_flare).clamp(0.55, 1.40)
}

fn map_s03b_player_start(asset_path: &str) -> Option<Vec3> {
    is_map_s03b_asset(asset_path).then(map_s03b_player_start_bevy)
}

fn is_map_s03b_asset(asset_path: &str) -> bool {
    asset_path
        .replace('\\', "/")
        .eq_ignore_ascii_case(MAP_S03B_ASSET_PATH)
}

fn map_s03b_player_start_bevy() -> Vec3 {
    ue_position_cm_to_bevy_m(MAP_S03B_PLAYER_START_UE_CM)
}

fn ue_position_cm_to_bevy_m(position: Vec3) -> Vec3 {
    Vec3::new(position.x, position.z, position.y) * 0.01
}

fn world_mesh_bounds(
    mesh_bounds: &Query<(&Aabb, &GlobalTransform), With<Mesh3d>>,
) -> Option<(Vec3, Vec3)> {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    let mut has_bounds = false;

    for (aabb, global_transform) in mesh_bounds {
        let local_min = Vec3::from(aabb.min());
        let local_max = Vec3::from(aabb.max());
        let affine = global_transform.affine();
        for x in [local_min.x, local_max.x] {
            for y in [local_min.y, local_max.y] {
                for z in [local_min.z, local_max.z] {
                    let point = affine.transform_point3(Vec3::new(x, y, z));
                    minimum = minimum.min(point);
                    maximum = maximum.max(point);
                    has_bounds = true;
                }
            }
        }
    }

    has_bounds.then_some((minimum, maximum))
}

fn startup_level_from_args() -> LevelId {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(path) = arg.strip_prefix("--level=") {
            if !path.trim().is_empty() {
                return LevelId::asset(path);
            }
        } else if arg == "--level" {
            if let Some(path) = args.next().filter(|path| !path.trim().is_empty()) {
                return LevelId::asset(path);
            }
        }
    }

    LevelId::asset("levels/Map_S03B/Map_S03B.zevy-level.json")
}

fn apply_active_level_fog_to_cameras(
    mut commands: Commands,
    active_fog: Res<ActiveLevelFog>,
    cameras_without_fog: Query<Entity, (With<Camera3d>, Without<DistanceFog>)>,
    fogged_cameras: Query<Entity, With<DistanceFog>>,
) {
    match &active_fog.0 {
        Some(fog) => {
            for entity in &cameras_without_fog {
                commands.entity(entity).insert(fog.clone());
            }
        }
        None => {
            for entity in &fogged_cameras {
                commands.entity(entity).remove::<DistanceFog>();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_map_s03b_ue_player_start_to_bevy_coordinates() {
        assert!(
            map_s03b_player_start_bevy().abs_diff_eq(Vec3::new(123.7, -20.0, -2.5), f32::EPSILON,)
        );
    }

    #[test]
    fn map_s03b_start_is_scoped_to_only_that_asset_level() {
        assert_eq!(
            map_s03b_player_start("levels\\Map_S03B\\Map_S03B.zevy-level.json"),
            Some(Vec3::new(123.7, -20.0, -2.5)),
        );
        assert_eq!(
            map_s03b_player_start("levels/Other/Other.zevy-level.json"),
            None,
        );
    }

    #[test]
    fn applies_map_s03b_start_to_xr_tracking_root() {
        let mut app = App::new();
        app.insert_resource(CurrentLevel(Some(LevelId::asset(MAP_S03B_ASSET_PATH))))
            .add_systems(Update, apply_map_s03b_xr_start);
        let previous_translation = Vec3::new(4.0, 5.0, 6.0);
        let root = app
            .world_mut()
            .spawn((
                XrTrackingRoot,
                Transform::from_translation(previous_translation),
            ))
            .id();

        app.update();

        let transform = app.world().entity(root).get::<Transform>().unwrap();
        assert!(
            transform
                .translation
                .abs_diff_eq(Vec3::new(123.7, -20.0, -2.5), f32::EPSILON,)
        );
        assert!(app.world().entity(root).contains::<MapS03BXrStartApplied>());

        app.world_mut().resource_mut::<CurrentLevel>().0 = Some(LevelId::Empty);
        app.update();

        let transform = app.world().entity(root).get::<Transform>().unwrap();
        assert_eq!(transform.translation, previous_translation);
        assert!(!app.world().entity(root).contains::<MapS03BXrStartApplied>());
    }

    #[test]
    fn candle_flicker_is_visible_but_bounded() {
        let samples = (0..600)
            .map(|sample| candle_flicker_multiplier(sample as f32 / 60.0, 0.75))
            .collect::<Vec<_>>();
        let minimum = samples.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum = samples.iter().copied().fold(f32::NEG_INFINITY, f32::max);

        assert!(minimum >= 0.55);
        assert!(maximum <= 1.40);
        assert!(maximum - minimum > 0.50);
        assert_ne!(
            candle_flicker_multiplier(1.25, 0.25),
            candle_flicker_multiplier(1.25, 2.25),
        );
    }
}
