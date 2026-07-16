mod desktop_player;
mod levels;
mod zevy_level;

use std::env;

use bevy::{prelude::*, render::primitives::Aabb};

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
                    desktop_player::update_desktop_level_player
                        .after(EngineInputSet::Collect)
                        .after(frame_asset_level_camera),
                    levels::move_xr_level_player.after(EngineInputSet::Collect),
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

#[derive(Component, Default)]
struct AssetLevelCamera {
    last_mesh_count: usize,
    stable_frames: u8,
    framed: bool,
}

#[derive(Component)]
pub(crate) struct ImportedLevelCameraFramed;

#[derive(Component)]
struct AssetLevelFallbackLight;

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
                    commands.entity(camera).insert((
                        AssetLevelCamera::default(),
                        AmbientLight {
                            color: Color::WHITE,
                            brightness: 500.0,
                            affects_lightmapped_meshes: true,
                        },
                    ));
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
        let direction = Vec3::new(-1.0, 0.7, 1.0).normalize();

        let fov = match projection.as_ref() {
            Projection::Perspective(perspective) => perspective.fov,
            _ => std::f32::consts::FRAC_PI_4,
        };
        let distance = radius / (fov * 0.5).tan();
        *camera_transform =
            Transform::from_translation(center + direction * distance).looking_at(center, Vec3::Y);

        if let Projection::Perspective(perspective) = projection.as_mut() {
            perspective.near = (radius * 0.001).clamp(0.05, 1.0);
            perspective.far = (distance + radius * 4.0).max(1000.0);
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
            "Framed imported Level camera: meshes={} center={:?} size={:?} distance={:.2} imported_lights={}",
            mesh_count,
            center,
            maximum - minimum,
            distance,
            has_imported_lights,
        );
    }
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
