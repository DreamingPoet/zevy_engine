mod desktop_player;
mod levels;
mod mip_texture;
mod zevy_level;

use std::env;

use bevy::{
    pbr::{
        ClusterConfig, ClusterFarZMode, ClusterZConfig, NotShadowCaster, NotShadowReceiver,
        ShadowFilteringMethod,
    },
    prelude::*,
    render::primitives::Aabb,
};
use bevy_mod_xr::session::XrTrackingRoot;

use crate::{
    app::{LaunchMode, StartupMode},
    config::RenderQualityConfig,
    input::EngineInputSet,
    shadow_cache::{CachedPointLightShadow, ZevyShadowCacheFrame},
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
        app.add_plugins((ZevyLevelPlugin, mip_texture::ZevyMipTexturePlugin))
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
                    sync_map_s03b_candle_visuals.after(apply_map_s03b_lighting_profile),
                    apply_map_s03b_shadow_residency
                        .after(apply_map_s03b_lighting_profile)
                        .after(sync_map_s03b_candle_visuals),
                    animate_map_s03b_candle_lights
                        .after(sync_map_s03b_candle_visuals)
                        .after(apply_map_s03b_shadow_residency),
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
const MAP_S03B_CANDLE_EMISSIVE_STRENGTH: f32 = 80.0;
const MAP_S03B_CANDLE_HORIZONTAL_SWAY_M: f32 = 0.005;
const MAP_S03B_CANDLE_VERTICAL_SWAY_M: f32 = 0.005;

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
    previous_shadows_enabled: bool,
    previous_translation: Vec3,
    base_intensity: f32,
    base_range: f32,
    flicker_phase: f32,
}

#[derive(Component, Clone, Copy)]
struct MapS03BCandleVisualSpawned {
    child: Entity,
}

#[derive(Component, Clone)]
struct MapS03BCandleGlow {
    material: Handle<StandardMaterial>,
    base_emissive: LinearRgba,
    base_scale: Vec3,
    flicker_phase: f32,
}

#[derive(Component, Clone)]
struct MapS03BCameraLightingApplied {
    previous_ambient: Option<AmbientLight>,
    previous_cluster_config: Option<ClusterConfig>,
    previous_shadow_filter: Option<ShadowFilteringMethod>,
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
                            Transform::from_translation(start)
                                .looking_to(Vec3::new(-90_f32, 0_f32, 0_f32), Vec3::Y),
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
    quality: Res<RenderQualityConfig>,
    mut commands: Commands,
    mut point_lights: Query<
        (
            Entity,
            &mut PointLight,
            &mut Transform,
            Option<&MapS03BPointLightTuningApplied>,
        ),
        With<ImportedZevyLight>,
    >,
    mut cameras: Query<
        (
            Entity,
            Option<&mut AmbientLight>,
            Option<&mut ClusterConfig>,
            Option<&mut ShadowFilteringMethod>,
            Option<&MapS03BCameraLightingApplied>,
        ),
        With<Camera3d>,
    >,
) {
    let use_profile = current_level
        .0
        .as_ref()
        .is_some_and(|level| matches!(level, LevelId::Asset(path) if is_map_s03b_asset(path)));

    for (entity, mut light, mut transform, applied) in &mut point_lights {
        if use_profile && applied.is_none() {
            let previous_intensity = light.intensity;
            let previous_range = light.range;
            let previous_shadows_enabled = light.shadows_enabled;
            let previous_translation = transform.translation;
            light.intensity *= MAP_S03B_POINT_LIGHT_INTENSITY_SCALE;
            light.range *= MAP_S03B_POINT_LIGHT_RANGE_SCALE;
            // Stable shadow residency is applied after this profile. Starting
            // disabled prevents a one-frame allocation with incomplete data.
            light.shadows_enabled = false;
            let tuning = MapS03BPointLightTuningApplied {
                previous_intensity,
                previous_range,
                previous_shadows_enabled,
                previous_translation,
                base_intensity: light.intensity,
                base_range: light.range,
                flicker_phase: entity.index() as f32 * 1.618_034,
            };
            commands
                .entity(entity)
                .insert((tuning, CachedPointLightShadow::default()));
            info!(
                "Applied Map_S03B PointLight tuning: intensity {:.3} -> {:.3} lm, range {:.3} -> {:.3} m",
                tuning.previous_intensity, light.intensity, tuning.previous_range, light.range,
            );
        } else if !use_profile && let Some(applied) = applied {
            light.intensity = applied.previous_intensity;
            light.range = applied.previous_range;
            light.shadows_enabled = applied.previous_shadows_enabled;
            transform.translation = applied.previous_translation;
            commands
                .entity(entity)
                .remove::<(MapS03BPointLightTuningApplied, CachedPointLightShadow)>();
        }
    }

    let optimized_cluster_config = map_s03b_cluster_config(*quality);
    for (entity, ambient, cluster_config, shadow_filter, applied) in &mut cameras {
        if use_profile && applied.is_none() {
            let previous_ambient = ambient.as_deref().cloned();
            let previous_cluster_config = cluster_config.as_deref().copied();
            let previous_shadow_filter = shadow_filter.as_deref().copied();
            if let Some(mut ambient) = ambient {
                ambient.brightness = MAP_S03B_AMBIENT_BRIGHTNESS;
            } else {
                commands.entity(entity).insert(AmbientLight {
                    color: Color::WHITE,
                    brightness: MAP_S03B_AMBIENT_BRIGHTNESS,
                    affects_lightmapped_meshes: true,
                });
            }
            if let Some(mut cluster_config) = cluster_config {
                *cluster_config = optimized_cluster_config;
            } else {
                commands.entity(entity).insert(optimized_cluster_config);
            }
            if let Some(mut shadow_filter) = shadow_filter {
                *shadow_filter = ShadowFilteringMethod::Hardware2x2;
            } else {
                commands
                    .entity(entity)
                    .insert(ShadowFilteringMethod::Hardware2x2);
            }
            commands
                .entity(entity)
                .insert(MapS03BCameraLightingApplied {
                    previous_ambient,
                    previous_cluster_config,
                    previous_shadow_filter,
                });
        } else if !use_profile && let Some(applied) = applied {
            if let Some(previous_ambient) = applied.previous_ambient.clone() {
                commands.entity(entity).insert(previous_ambient);
            } else {
                commands.entity(entity).remove::<AmbientLight>();
            }
            if let Some(previous_cluster_config) = applied.previous_cluster_config {
                commands.entity(entity).insert(previous_cluster_config);
            } else {
                commands.entity(entity).remove::<ClusterConfig>();
            }
            if let Some(previous_shadow_filter) = applied.previous_shadow_filter {
                commands.entity(entity).insert(previous_shadow_filter);
            } else {
                commands.entity(entity).remove::<ShadowFilteringMethod>();
            }
            commands
                .entity(entity)
                .remove::<MapS03BCameraLightingApplied>();
        }
    }
}

fn apply_map_s03b_shadow_residency(
    current_level: Res<CurrentLevel>,
    quality: Res<RenderQualityConfig>,
    mut point_lights: Query<
        (Entity, &mut PointLight, &MapS03BPointLightTuningApplied),
        With<ImportedZevyLight>,
    >,
    mut previous_selection: Local<Vec<Entity>>,
) {
    let use_profile = current_level
        .0
        .as_ref()
        .is_some_and(|level| matches!(level, LevelId::Asset(path) if is_map_s03b_asset(path)));
    if !use_profile {
        previous_selection.clear();
        return;
    }

    // Shadow residency is deliberately independent of every camera. A light
    // must never gain or lose its shadow merely because the player crossed an
    // arbitrary distance threshold. Entity order provides a deterministic cap
    // for oversized test maps; Map_S03B's default cap contains all seven lights.
    let mut selected = point_lights
        .iter_mut()
        .filter_map(|(entity, _, tuning)| tuning.previous_shadows_enabled.then_some(entity))
        .collect::<Vec<_>>();
    selected.sort_unstable_by_key(|entity| entity.index());
    selected.truncate(quality.max_shadowed_point_lights);

    for (entity, mut light, tuning) in &mut point_lights {
        light.shadows_enabled = tuning.previous_shadows_enabled && selected.contains(&entity);
    }

    if *previous_selection != selected {
        info!(
            "Map_S03B point-shadow residency fixed at {}/{} lights ({} cubemap shadow views); camera distance does not change this set",
            selected.len(),
            quality.max_shadowed_point_lights,
            selected.len() * 6,
        );
        *previous_selection = selected;
    }
}

fn sync_map_s03b_candle_visuals(
    current_level: Res<CurrentLevel>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut candle_mesh: Local<Option<Handle<Mesh>>>,
    point_lights: Query<
        (
            Entity,
            &PointLight,
            &MapS03BPointLightTuningApplied,
            Option<&MapS03BCandleVisualSpawned>,
        ),
        With<ImportedZevyLight>,
    >,
) {
    let use_profile = current_level
        .0
        .as_ref()
        .is_some_and(|level| matches!(level, LevelId::Asset(path) if is_map_s03b_asset(path)));

    for (entity, light, tuning, visual) in &point_lights {
        if use_profile && visual.is_none() {
            let mesh = candle_mesh
                .get_or_insert_with(|| meshes.add(Sphere::new(0.10).mesh().ico(2).unwrap()))
                .clone();
            let base_emissive = LinearRgba::from(light.color) * MAP_S03B_CANDLE_EMISSIVE_STRENGTH;
            let material = materials.add(StandardMaterial {
                base_color: light.color,
                emissive: base_emissive,
                emissive_exposure_weight: 0.0,
                perceptual_roughness: 1.0,
                unlit: true,
                ..default()
            });
            let base_scale = Vec3::new(0.8, 1.35, 0.8);
            let child = commands
                .spawn((
                    Name::new("MapS03BCandleGlow"),
                    Mesh3d(mesh),
                    MeshMaterial3d(material.clone()),
                    MapS03BCandleGlow {
                        material,
                        base_emissive,
                        base_scale,
                        flicker_phase: tuning.flicker_phase,
                    },
                    NotShadowCaster,
                    NotShadowReceiver,
                    Transform::from_scale(base_scale),
                    ChildOf(entity),
                ))
                .id();
            commands
                .entity(entity)
                .insert(MapS03BCandleVisualSpawned { child });
        } else if !use_profile && let Some(visual) = visual {
            commands.entity(visual.child).despawn();
            commands
                .entity(entity)
                .remove::<MapS03BCandleVisualSpawned>();
        }
    }
}

fn animate_map_s03b_candle_lights(
    current_level: Res<CurrentLevel>,
    time: Res<Time>,
    quality: Res<RenderQualityConfig>,
    mut shadow_cache_frame: ResMut<ZevyShadowCacheFrame>,
    mut point_lights: Query<
        (
            Entity,
            &mut PointLight,
            &mut Transform,
            &MapS03BPointLightTuningApplied,
            &mut CachedPointLightShadow,
        ),
        Without<MapS03BCandleGlow>,
    >,
    mut candle_glows: Query<(&mut Transform, &MapS03BCandleGlow), Without<PointLight>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let use_profile = current_level
        .0
        .as_ref()
        .is_some_and(|level| matches!(level, LevelId::Asset(path) if is_map_s03b_asset(path)));
    if !use_profile {
        return;
    }

    let seconds = time.elapsed_secs();
    let shadow_update_hz = quality.resolved_cached_point_shadow_update_hz();
    let mut shadow_update_budget = quality.max_cached_point_shadow_updates_per_frame;
    for (entity, mut light, mut transform, tuning, mut cached_shadow) in &mut point_lights {
        let intensity_multiplier = candle_flicker_multiplier(seconds, tuning.flicker_phase);
        let range_multiplier = 1.0 + (intensity_multiplier - 1.0) * 0.28;
        light.intensity = tuning.base_intensity * intensity_multiplier;

        if !quality.persistent_point_shadow_cache {
            light.range = tuning.base_range * range_multiplier;
            transform.translation =
                tuning.previous_translation + candle_light_offset(seconds, tuning.flicker_phase);
            cached_shadow.last_update_tick = None;
        } else if shadow_update_hz > 0.0 && shadow_update_budget > 0 {
            let phase_offset =
                tuning.flicker_phase.rem_euclid(core::f32::consts::TAU) / core::f32::consts::TAU;
            let update_tick = (seconds * shadow_update_hz + phase_offset).floor() as u64;
            if cached_shadow.last_update_tick != Some(update_tick) {
                light.range = tuning.base_range * range_multiplier;
                transform.translation = tuning.previous_translation
                    + candle_light_offset(seconds, tuning.flicker_phase);
                cached_shadow.last_update_tick = Some(update_tick);
                shadow_cache_frame.invalidate_point_light(entity);
                shadow_update_budget -= 1;
            }
        }
    }

    for (mut transform, glow) in &mut candle_glows {
        let intensity_multiplier = candle_flicker_multiplier(seconds, glow.flicker_phase);
        let normalized = ((intensity_multiplier - 0.55) / (1.40 - 0.55)).clamp(0.0, 1.0);
        let visual_scale = 0.86 + normalized * 0.28;
        transform.scale = glow.base_scale * visual_scale;
        if let Some(material) = materials.get_mut(&glow.material) {
            material.emissive = glow.base_emissive * intensity_multiplier;
        }
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

fn candle_light_offset(seconds: f32, phase: f32) -> Vec3 {
    let x = ((seconds * 2.1 + phase).sin() * 0.68 + (seconds * 8.3 + phase * 1.73).sin() * 0.32)
        * MAP_S03B_CANDLE_HORIZONTAL_SWAY_M;
    let z = ((seconds * 2.7 + phase * 1.21).sin() * 0.64
        + (seconds * 9.1 + phase * 2.07).sin() * 0.36)
        * MAP_S03B_CANDLE_HORIZONTAL_SWAY_M;
    let y = ((seconds * 3.4 + phase * 0.83).sin() * 0.72
        + (seconds * 11.7 + phase * 1.49).sin() * 0.28)
        * MAP_S03B_CANDLE_VERTICAL_SWAY_M;

    Vec3::new(x, y, z)
}

fn map_s03b_player_start(asset_path: &str) -> Option<Vec3> {
    is_map_s03b_asset(asset_path).then(map_s03b_player_start_bevy)
}

fn is_map_s03b_asset(asset_path: &str) -> bool {
    asset_path
        .replace('\\', "/")
        .eq_ignore_ascii_case(MAP_S03B_ASSET_PATH)
}

fn map_s03b_cluster_config(quality: RenderQualityConfig) -> ClusterConfig {
    ClusterConfig::FixedZ {
        total: quality.resolved_cluster_total(),
        z_slices: quality.resolved_cluster_z_slices(),
        z_config: ClusterZConfig {
            first_slice_depth: quality.resolved_cluster_first_slice_depth_m(),
            far_z_mode: ClusterFarZMode::Constant(quality.resolved_cluster_far_z_m()),
        },
        dynamic_resizing: true,
    }
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

    #[test]
    fn candle_light_sway_is_visible_bounded_and_smooth() {
        let samples = (0..600)
            .map(|sample| candle_light_offset(sample as f32 / 60.0, 0.75))
            .collect::<Vec<_>>();
        let maximum_abs = samples
            .iter()
            .copied()
            .map(Vec3::abs)
            .fold(Vec3::ZERO, Vec3::max);
        let maximum_step = samples
            .windows(2)
            .map(|pair| pair[0].distance(pair[1]))
            .fold(0.0, f32::max);
        let maximum_distance = samples
            .iter()
            .copied()
            .map(Vec3::length)
            .fold(0.0, f32::max);

        assert!(maximum_abs.x <= MAP_S03B_CANDLE_HORIZONTAL_SWAY_M + 0.000_01);
        assert!(maximum_abs.z <= MAP_S03B_CANDLE_HORIZONTAL_SWAY_M + 0.000_01);
        assert!(maximum_abs.y <= MAP_S03B_CANDLE_VERTICAL_SWAY_M + 0.000_01);
        assert!(maximum_distance > 0.003);
        assert!(maximum_step < 0.02);
        assert_ne!(
            candle_light_offset(1.25, 0.25),
            candle_light_offset(1.25, 2.25),
        );
    }

    #[test]
    fn candle_animation_system_accepts_disjoint_transform_queries() {
        let mut app = App::new();
        app.insert_resource(CurrentLevel(Some(LevelId::asset(MAP_S03B_ASSET_PATH))))
            .insert_resource(RenderQualityConfig::default())
            .init_resource::<ZevyShadowCacheFrame>()
            .init_resource::<Time>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Update, animate_map_s03b_candle_lights);

        let glow_material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let base_translation = Vec3::new(1.0, 2.0, 3.0);
        let point_light = app
            .world_mut()
            .spawn((
                PointLight::default(),
                Transform::from_translation(base_translation),
                MapS03BPointLightTuningApplied {
                    previous_intensity: 1.0,
                    previous_range: 1.0,
                    previous_shadows_enabled: true,
                    previous_translation: base_translation,
                    base_intensity: 100.0,
                    base_range: 6.0,
                    flicker_phase: 0.75,
                },
                CachedPointLightShadow::default(),
            ))
            .id();
        app.world_mut().spawn((
            Transform::default(),
            MapS03BCandleGlow {
                material: glow_material,
                base_emissive: LinearRgba::WHITE,
                base_scale: Vec3::ONE,
                flicker_phase: 0.75,
            },
        ));

        app.update();

        let transform = app.world().entity(point_light).get::<Transform>().unwrap();
        assert_ne!(transform.translation, base_translation);
    }
}
