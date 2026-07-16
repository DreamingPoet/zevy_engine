use std::{
    collections::{HashMap, HashSet},
    env,
    time::Instant,
};

use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    ecs::system::SystemParam,
    prelude::*,
    scene::SceneInstanceReady,
    window::{ExitCondition, WindowPlugin},
};
use zevy_engine::{
    ImportedZevyEntity, ImportedZevyLevel, ImportedZevyLight, ZevyLevelAsset, ZevyLevelPlugin,
    ZevyLightKind, spawn_zevy_level,
};

const VALIDATION_TIMEOUT_SECONDS: u64 = 300;

#[derive(Resource)]
struct ValidationState {
    asset_path: String,
    handle: Option<Handle<ZevyLevelAsset>>,
    root: Option<Entity>,
    started_at: Instant,
    finished: bool,
    metadata_logged: bool,
    expected_scene_instances: usize,
    ready_scene_roots: HashSet<Entity>,
    settle_frames: u8,
}

#[derive(Component)]
struct ValidationSceneRoot;

#[derive(SystemParam)]
struct ValidationLightQueries<'w, 's> {
    directional: Query<'w, 's, &'static DirectionalLight>,
    point: Query<'w, 's, &'static PointLight>,
    spot: Query<'w, 's, &'static SpotLight>,
    imported: Query<'w, 's, &'static ImportedZevyLight>,
}

fn main() -> AppExit {
    let Some(asset_path) = env::args().nth(1) else {
        eprintln!("usage: cargo run --bin validate_zevy_level -- <path relative to assets>");
        return AppExit::error();
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            close_when_requested: false,
        }))
        .add_plugins(ZevyLevelPlugin)
        .insert_resource(ValidationState {
            asset_path,
            handle: None,
            root: None,
            started_at: Instant::now(),
            finished: false,
            metadata_logged: false,
            expected_scene_instances: 0,
            ready_scene_roots: HashSet::new(),
            settle_frames: 0,
        })
        .add_systems(Startup, begin_validation)
        .add_systems(Update, (poll_validation, inspect_when_ready).chain())
        .add_observer(track_ready_scene)
        .run()
}

fn begin_validation(
    mut commands: Commands,
    mut state: ResMut<ValidationState>,
    asset_server: Res<AssetServer>,
) {
    println!("Validating Zevy Level asset: {}", state.asset_path);
    let (root, handle) = spawn_zevy_level(&mut commands, &asset_server, &state.asset_path);
    commands.entity(root).insert(ValidationSceneRoot);
    state.root = Some(root);
    state.handle = Some(handle);
}

fn poll_validation(
    mut state: ResMut<ValidationState>,
    asset_server: Res<AssetServer>,
    level_assets: Res<Assets<ZevyLevelAsset>>,
    mut exit: EventWriter<AppExit>,
) {
    if state.finished {
        return;
    }

    let Some(handle) = state.handle.as_ref() else {
        return;
    };

    if let LoadState::Failed(error) = asset_server.load_state(handle.id()) {
        eprintln!("Zevy Level manifest failed to load: {error}");
        state.finished = true;
        exit.write(AppExit::error());
        return;
    }

    if let RecursiveDependencyLoadState::Failed(error) =
        asset_server.recursive_dependency_load_state(handle.id())
    {
        eprintln!("Zevy Level dependency failed to load: {error}");
        state.finished = true;
        exit.write(AppExit::error());
        return;
    }

    if !state.metadata_logged {
        let Some(level) = level_assets.get(handle) else {
            return;
        };

        state.expected_scene_instances = if level.is_composed() {
            level
                .entities
                .iter()
                .filter(|entity| entity.asset.is_some())
                .count()
        } else {
            usize::from(level.monolithic_scene().is_some())
        };
        state.metadata_logged = true;

        println!(
            "Zevy Level assets loaded: '{}' schema={} composed={} source_scene='{}' assets={} entities={} expected_scene_instances={} meshes={}/{} materials={} lights={}/{}/{} UE='{}' map='{}'",
            level.level_name,
            level.schema_version,
            level.is_composed(),
            level.scene_path.as_deref().unwrap_or("<composed>"),
            level.assets.len(),
            level.entities.len(),
            state.expected_scene_instances,
            level.content.static_mesh_actors,
            level.content.static_mesh_components,
            level.content.unique_materials,
            level.content.directional_lights,
            level.content.point_lights,
            level.content.spot_lights,
            level.source.unreal_engine_version,
            level.source.map_package,
        );
    }

    if state.started_at.elapsed().as_secs() >= VALIDATION_TIMEOUT_SECONDS {
        eprintln!(
            "Timed out after {VALIDATION_TIMEOUT_SECONDS}s while loading Zevy Level '{}' ({}/{} scene instances ready)",
            state.asset_path,
            state.ready_scene_roots.len(),
            state.expected_scene_instances,
        );
        state.finished = true;
        exit.write(AppExit::error());
    }
}

fn track_ready_scene(trigger: Trigger<SceneInstanceReady>, mut state: ResMut<ValidationState>) {
    if !state.finished {
        state.ready_scene_roots.insert(trigger.target());
    }
}

#[allow(clippy::too_many_arguments)]
fn inspect_when_ready(
    mut state: ResMut<ValidationState>,
    imported_level_roots: Query<(), With<ImportedZevyLevel>>,
    children: Query<&Children>,
    child_of: Query<&ChildOf>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
    visibility: Query<&Visibility>,
    imported_entities: Query<&ImportedZevyEntity>,
    scene_roots: Query<(), With<SceneRoot>>,
    meshes: Query<(), With<Mesh3d>>,
    mesh_materials: Query<&MeshMaterial3d<StandardMaterial>>,
    light_queries: ValidationLightQueries,
    level_assets: Res<Assets<ZevyLevelAsset>>,
    mut exit: EventWriter<AppExit>,
) {
    if state.finished || !state.metadata_logged {
        return;
    }

    let (Some(root), Some(handle)) = (state.root, state.handle.clone()) else {
        return;
    };
    if imported_level_roots.get(root).is_err()
        || state.ready_scene_roots.len() < state.expected_scene_instances
    {
        return;
    }

    state.settle_frames += 1;
    if state.settle_frames < 4 {
        return;
    }

    let Some(level) = level_assets.get(&handle) else {
        return;
    };
    let descendants: Vec<Entity> = children.iter_descendants(root).collect();
    let descendant_set = descendants.iter().copied().collect::<HashSet<_>>();

    let mesh_count = descendants
        .iter()
        .filter(|entity| meshes.get(**entity).is_ok())
        .count();
    let directional_light_count = descendants
        .iter()
        .filter(|entity| light_queries.directional.get(**entity).is_ok())
        .count();
    let point_light_count = descendants
        .iter()
        .filter(|entity| light_queries.point.get(**entity).is_ok())
        .count();
    let spot_light_count = descendants
        .iter()
        .filter(|entity| light_queries.spot.get(**entity).is_ok())
        .count();
    let imported_light_count = descendants
        .iter()
        .filter(|entity| light_queries.imported.get(**entity).is_ok())
        .count();
    let expected_imported_light_count = level
        .entities
        .iter()
        .map(|entity| entity.lights.len())
        .sum::<usize>();
    let unique_material_count = descendants
        .iter()
        .filter_map(|entity| mesh_materials.get(*entity).ok())
        .map(|material| material.id())
        .collect::<HashSet<_>>()
        .len();
    let scene_root_count = descendants
        .iter()
        .filter(|entity| scene_roots.get(**entity).is_ok())
        .count()
        + usize::from(scene_roots.get(root).is_ok());

    let max_depth = descendants
        .iter()
        .map(|entity| hierarchy_depth(*entity, root, &child_of))
        .max()
        .unwrap_or_default();

    let hierarchy_valid = if level.is_composed() {
        validate_composed_hierarchy(
            root,
            level,
            &descendant_set,
            &child_of,
            &names,
            &transforms,
            &visibility,
            &imported_entities,
        )
    } else {
        let entity_names = descendants
            .iter()
            .filter_map(|entity| names.get(*entity).ok())
            .map(|name| name.as_str().to_owned())
            .collect::<HashSet<_>>();
        [
            "ZevyFixtureParentCube",
            "ZevyFixtureChildSphere",
            "ZevyFixtureGrandchildCylinder",
        ]
        .iter()
        .all(|name| entity_names.contains(*name))
    };

    let geometry_valid = if level.is_composed() {
        (level.content.static_mesh_components == 0 || mesh_count > 0)
            && (level.content.unique_materials == 0 || unique_material_count > 0)
            && (level.content.directional_lights == 0 || directional_light_count > 0)
            && (level.content.point_lights == 0 || point_light_count > 0)
            && (level.content.spot_lights == 0 || spot_light_count > 0)
    } else {
        mesh_count == level.content.static_mesh_components as usize
            && unique_material_count == level.content.unique_materials as usize
            && directional_light_count == level.content.directional_lights as usize
            && point_light_count == level.content.point_lights as usize
            && spot_light_count == level.content.spot_lights as usize
            && max_depth >= 4
    };
    let scene_instances_valid = scene_root_count == state.expected_scene_instances;
    let light_parameters_valid = imported_light_count == expected_imported_light_count
        && descendants.iter().copied().all(|entity| {
            let Ok(imported) = light_queries.imported.get(entity) else {
                return true;
            };
            validate_light_parameters(
                entity,
                imported,
                &visibility,
                &light_queries.directional,
                &light_queries.point,
                &light_queries.spot,
            )
        });
    let valid =
        hierarchy_valid && geometry_valid && scene_instances_valid && light_parameters_valid;

    if valid {
        println!(
            "Zevy Level scene instantiated: entities={} manifest_entities={} scene_instances={}/{} meshes={} materials={} lights={}/{}/{} reusable_light_parameters={}/{} max_hierarchy_depth={} hierarchy_and_local_transforms=ok",
            descendants.len(),
            level.entities.len(),
            scene_root_count,
            state.expected_scene_instances,
            mesh_count,
            unique_material_count,
            directional_light_count,
            point_light_count,
            spot_light_count,
            imported_light_count,
            expected_imported_light_count,
            max_depth,
        );
        state.finished = true;
        exit.write(AppExit::Success);
    } else {
        eprintln!(
            "Zevy Level scene validation failed: descendants={} manifest_entities={} scene_instances={}/{} meshes={} source_mesh_components={} materials={} source_materials={} lights={}/{}/{} source_lights={}/{}/{} reusable_light_parameters={}/{} max_hierarchy_depth={} hierarchy_valid={} geometry_valid={} light_parameters_valid={}",
            descendants.len(),
            level.entities.len(),
            scene_root_count,
            state.expected_scene_instances,
            mesh_count,
            level.content.static_mesh_components,
            unique_material_count,
            level.content.unique_materials,
            directional_light_count,
            point_light_count,
            spot_light_count,
            level.content.directional_lights,
            level.content.point_lights,
            level.content.spot_lights,
            imported_light_count,
            expected_imported_light_count,
            max_depth,
            hierarchy_valid,
            geometry_valid,
            light_parameters_valid,
        );
        state.finished = true;
        exit.write(AppExit::error());
    }
}

fn validate_light_parameters(
    entity: Entity,
    imported: &ImportedZevyLight,
    visibility: &Query<&Visibility>,
    directional_lights: &Query<&DirectionalLight>,
    point_lights: &Query<&PointLight>,
    spot_lights: &Query<&SpotLight>,
) -> bool {
    let expected_visibility = if imported.source.bevy.enabled {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if visibility.get(entity).ok() != Some(&expected_visibility) {
        return false;
    }

    match imported.source.kind {
        ZevyLightKind::Point => point_lights.get(entity).is_ok_and(|light| {
            color_approximately_equal(light.color, imported.source.bevy.color_srgb)
                && float_approximately_equal(light.intensity, imported.source.bevy.intensity)
                && float_approximately_equal(light.range, imported.source.bevy.range_m)
                && float_approximately_equal(light.radius, imported.source.bevy.radius_m)
                && light.shadows_enabled == imported.source.bevy.shadows_enabled
        }),
        ZevyLightKind::Spot => spot_lights.get(entity).is_ok_and(|light| {
            color_approximately_equal(light.color, imported.source.bevy.color_srgb)
                && float_approximately_equal(light.intensity, imported.source.bevy.intensity)
                && float_approximately_equal(light.range, imported.source.bevy.range_m)
                && float_approximately_equal(light.radius, imported.source.bevy.radius_m)
                && float_approximately_equal(
                    light.inner_angle,
                    imported.source.bevy.inner_angle_radians,
                )
                && float_approximately_equal(
                    light.outer_angle,
                    imported.source.bevy.outer_angle_radians,
                )
                && light.shadows_enabled == imported.source.bevy.shadows_enabled
        }),
        ZevyLightKind::Directional => directional_lights.get(entity).is_ok_and(|light| {
            color_approximately_equal(light.color, imported.source.bevy.color_srgb)
                && float_approximately_equal(light.illuminance, imported.source.bevy.intensity)
                && light.shadows_enabled == imported.source.bevy.shadows_enabled
        }),
    }
}

fn color_approximately_equal(actual: Color, expected: [f32; 3]) -> bool {
    let actual = actual.to_srgba();
    float_approximately_equal(actual.red, expected[0])
        && float_approximately_equal(actual.green, expected[1])
        && float_approximately_equal(actual.blue, expected[2])
}

fn float_approximately_equal(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() <= expected.abs().max(1.0) * 0.00001
}

#[allow(clippy::too_many_arguments)]
fn validate_composed_hierarchy(
    root: Entity,
    level: &ZevyLevelAsset,
    descendants: &HashSet<Entity>,
    child_of: &Query<&ChildOf>,
    names: &Query<&Name>,
    transforms: &Query<&Transform>,
    visibility: &Query<&Visibility>,
    imported_entities: &Query<&ImportedZevyEntity>,
) -> bool {
    let mut entity_by_id = HashMap::new();
    for entity in descendants.iter().copied() {
        if let Ok(imported) = imported_entities.get(entity) {
            entity_by_id.insert(imported.id.as_str(), entity);
        }
    }
    if entity_by_id.len() != level.entities.len() {
        return false;
    }

    for definition in &level.entities {
        let Some(&entity) = entity_by_id.get(definition.id.as_str()) else {
            return false;
        };
        let expected_parent = definition
            .parent
            .as_deref()
            .and_then(|parent_id| entity_by_id.get(parent_id).copied())
            .unwrap_or(root);
        if child_of.get(entity).map(ChildOf::parent).ok() != Some(expected_parent) {
            return false;
        }
        if names.get(entity).map(Name::as_str).ok() != Some(definition.name.as_str()) {
            return false;
        }

        let Ok(actual_transform) = transforms.get(entity) else {
            return false;
        };
        let expected_transform = definition.transform.to_bevy_transform();
        if !transform_approximately_equal(actual_transform, &expected_transform) {
            return false;
        }

        let expected_visibility = if definition.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if visibility.get(entity).ok() != Some(&expected_visibility) {
            return false;
        }
    }

    true
}

fn transform_approximately_equal(actual: &Transform, expected: &Transform) -> bool {
    actual.translation.abs_diff_eq(expected.translation, 0.0001)
        && actual.scale.abs_diff_eq(expected.scale, 0.0001)
        && actual.rotation.dot(expected.rotation).abs() >= 0.99999
}

fn hierarchy_depth(entity: Entity, root: Entity, child_of: &Query<&ChildOf>) -> usize {
    let mut current = entity;
    let mut depth = 1;
    while current != root && depth < 512 {
        let Ok(parent) = child_of.get(current) else {
            break;
        };
        current = parent.parent();
        depth += 1;
    }
    depth
}
