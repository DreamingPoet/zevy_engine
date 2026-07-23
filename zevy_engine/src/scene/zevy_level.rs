use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
};

use bevy::{
    asset::{AssetApp, AssetLoader, LoadContext, LoadState, io::Reader},
    gltf::GltfAssetLabel,
    prelude::*,
    reflect::TypePath,
};
use serde::Deserialize;
use thiserror::Error;

use crate::shadow_motion_policy::{
    LightShadowMotionClass, LightShadowMotionPolicy, ShadowCasterMotionPolicy,
};

use super::LevelEntity;

pub const ZEVY_LEVEL_SCHEMA_VERSION: u32 = 2;
const ZEVY_LEVEL_MIN_SCHEMA_VERSION: u32 = 1;

#[derive(Asset, TypePath, Debug)]
pub struct ZevyLevelAsset {
    pub schema_version: u32,
    pub level_name: String,
    pub scene_path: Option<String>,
    pub scene_index: usize,
    #[dependency]
    pub scenes: Vec<Handle<Scene>>,
    pub assets: Vec<ZevyLevelSceneAsset>,
    pub entities: Vec<ZevyLevelEntityDefinition>,
    pub source: ZevyLevelSource,
    pub content: ZevyLevelContentSummary,
    pub export: ZevyLevelExportMetadata,
}

impl ZevyLevelAsset {
    pub fn is_composed(&self) -> bool {
        self.schema_version >= 2
    }

    pub fn monolithic_scene(&self) -> Option<&Handle<Scene>> {
        (!self.is_composed()).then(|| self.scenes.first()).flatten()
    }

    pub fn scene_for_asset(&self, asset_id: &str) -> Option<&Handle<Scene>> {
        let asset = self.assets.iter().find(|asset| asset.id == asset_id)?;
        self.scenes.get(asset.scene_handle_index)
    }
}

#[derive(Clone, Debug)]
pub struct ZevyLevelSceneAsset {
    pub id: String,
    pub name: String,
    pub scene_path: String,
    pub scene_index: usize,
    scene_handle_index: usize,
}

#[derive(Clone, Debug)]
pub struct ZevyLevelEntityDefinition {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub asset: Option<String>,
    pub transform: ZevyLevelTransform,
    pub visible: bool,
    pub lights: Vec<ZevyLightDefinition>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ZevyLightKind {
    Directional,
    Point,
    Spot,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ZevyLightDefinition {
    pub component_name: String,
    pub gltf_name: String,
    pub kind: ZevyLightKind,
    #[serde(default)]
    pub bevy: ZevyBevyLightParameters,
    #[serde(default)]
    pub unreal: ZevyUnrealLightParameters,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ZevyBevyLightParameters {
    #[serde(default = "white_color")]
    pub color_srgb: [f32; 3],
    #[serde(default)]
    pub intensity: f32,
    #[serde(default)]
    pub intensity_unit: String,
    #[serde(default)]
    pub attenuation_model: String,
    #[serde(default = "default_visible")]
    pub enabled: bool,
    #[serde(default)]
    pub shadows_enabled: bool,
    #[serde(default = "default_light_range")]
    pub range_m: f32,
    #[serde(default)]
    pub radius_m: f32,
    #[serde(default)]
    pub inner_angle_radians: f32,
    #[serde(default = "default_spot_outer_angle")]
    pub outer_angle_radians: f32,
}

impl Default for ZevyBevyLightParameters {
    fn default() -> Self {
        Self {
            color_srgb: white_color(),
            intensity: 0.0,
            intensity_unit: String::new(),
            attenuation_model: String::new(),
            enabled: true,
            shadows_enabled: false,
            range_m: default_light_range(),
            radius_m: 0.0,
            inner_angle_radians: 0.0,
            outer_angle_radians: default_spot_outer_angle(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ZevyUnrealLightParameters {
    #[serde(default)]
    pub intensity: f32,
    #[serde(default)]
    pub intensity_units: String,
    #[serde(default = "white_color")]
    pub light_color_srgb: [f32; 3],
    #[serde(default)]
    pub use_temperature: bool,
    #[serde(default)]
    pub temperature_kelvin: f32,
    #[serde(default = "default_visible")]
    pub affects_world: bool,
    #[serde(default)]
    pub casts_shadows: bool,
    #[serde(default)]
    pub mobility: String,
    #[serde(default)]
    pub shadow_bias: f32,
    #[serde(default)]
    pub shadow_slope_bias: f32,
    #[serde(default)]
    pub attenuation_radius_m: f32,
    #[serde(default)]
    pub inverse_exposure_blend: f32,
    #[serde(default)]
    pub attenuation_model: String,
    #[serde(default)]
    pub falloff_exponent: f32,
    #[serde(default)]
    pub source_radius_m: f32,
    #[serde(default)]
    pub soft_source_radius_m: f32,
    #[serde(default)]
    pub source_length_m: f32,
    #[serde(default)]
    pub inner_cone_angle_degrees: f32,
    #[serde(default)]
    pub outer_cone_angle_degrees: f32,
}

impl ZevyUnrealLightParameters {
    /// Returns whether Unreal authored this light as fully static.
    ///
    /// Older manifests may omit mobility, so only an explicit `static` value
    /// opts into static-only runtime behavior.
    pub fn is_static_mobility(&self) -> bool {
        self.mobility.trim().eq_ignore_ascii_case("static")
    }
}

impl Default for ZevyUnrealLightParameters {
    fn default() -> Self {
        Self {
            intensity: 0.0,
            intensity_units: String::new(),
            light_color_srgb: white_color(),
            use_temperature: false,
            temperature_kelvin: 0.0,
            affects_world: true,
            casts_shadows: false,
            mobility: String::new(),
            shadow_bias: 0.0,
            shadow_slope_bias: 0.0,
            attenuation_radius_m: 0.0,
            inverse_exposure_blend: 0.0,
            attenuation_model: String::new(),
            falloff_exponent: 0.0,
            source_radius_m: 0.0,
            soft_source_radius_m: 0.0,
            source_length_m: 0.0,
            inner_cone_angle_degrees: 0.0,
            outer_cone_angle_degrees: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ZevyLevelTransform {
    #[serde(default)]
    pub translation: [f32; 3],
    #[serde(default = "identity_rotation")]
    pub rotation: [f32; 4],
    #[serde(default = "unit_scale")]
    pub scale: [f32; 3],
}

impl Default for ZevyLevelTransform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: identity_rotation(),
            scale: unit_scale(),
        }
    }
}

impl ZevyLevelTransform {
    pub fn to_bevy_transform(self) -> Transform {
        let rotation = Quat::from_array(self.rotation).normalize();
        Transform {
            translation: Vec3::from_array(self.translation),
            rotation,
            scale: Vec3::from_array(self.scale),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ZevyLevelSource {
    #[serde(default)]
    pub unreal_engine_version: String,
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub map_package: String,
    #[serde(default)]
    pub exported_at_utc: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ZevyLevelContentSummary {
    #[serde(default)]
    pub static_mesh_actors: u32,
    #[serde(default)]
    pub static_mesh_components: u32,
    #[serde(default)]
    pub unique_materials: u32,
    #[serde(default)]
    pub directional_lights: u32,
    #[serde(default)]
    pub point_lights: u32,
    #[serde(default)]
    pub spot_lights: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ZevyLevelExportMetadata {
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub coordinate_system: String,
    #[serde(default)]
    pub unit_scale: f32,
    #[serde(default)]
    pub texture_format: String,
    #[serde(default)]
    pub material_bake_mode: String,
    #[serde(default)]
    pub normal_maps_adjusted: bool,
    #[serde(default)]
    pub lights_exported: bool,
}

#[derive(Debug, Deserialize)]
struct ZevyLevelManifest {
    schema_version: u32,
    level_name: String,
    #[serde(default)]
    scene: Option<String>,
    #[serde(default)]
    scene_index: usize,
    #[serde(default)]
    assets: Vec<ZevyLevelManifestAsset>,
    #[serde(default)]
    entities: Vec<ZevyLevelManifestEntity>,
    #[serde(default)]
    source: ZevyLevelSource,
    #[serde(default)]
    content: ZevyLevelContentSummary,
    #[serde(default)]
    export: ZevyLevelExportMetadata,
}

#[derive(Clone, Debug, Deserialize)]
struct ZevyLevelManifestAsset {
    id: String,
    #[serde(default)]
    name: String,
    scene: String,
    #[serde(default)]
    scene_index: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct ZevyLevelManifestEntity {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    asset: Option<String>,
    #[serde(default)]
    transform: ZevyLevelTransform,
    #[serde(default = "default_visible")]
    visible: bool,
    #[serde(default)]
    lights: Vec<ZevyLightDefinition>,
}

#[derive(Default)]
pub struct ZevyLevelAssetLoader;

pub struct ZevyLevelPlugin;

impl Plugin for ZevyLevelPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<ZevyLevelAsset>()
            .init_asset_loader::<ZevyLevelAssetLoader>()
            .add_systems(
                Update,
                (resolve_pending_levels, apply_pending_light_overrides).chain(),
            );
    }
}

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ZevyLevelAssetLoaderError {
    #[error("could not read Zevy Level manifest: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not parse Zevy Level manifest JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "unsupported Zevy Level schema version {found}; this engine supports versions {supported_min} through {supported_max}"
    )]
    UnsupportedSchema {
        found: u32,
        supported_min: u32,
        supported_max: u32,
    },
    #[error("Zevy Level name must not be empty")]
    EmptyLevelName,
    #[error("invalid Zevy Level manifest: {0}")]
    InvalidManifest(String),
    #[error("invalid scene path '{0}': paths must be relative and remain inside the Level folder")]
    InvalidScenePath(String),
    #[error("unsupported scene file '{0}': expected a .glb or .gltf file")]
    UnsupportedSceneFormat(String),
}

impl AssetLoader for ZevyLevelAssetLoader {
    type Asset = ZevyLevelAsset;
    type Settings = ();
    type Error = ZevyLevelAssetLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;

        let manifest = parse_manifest(&bytes)?;
        let mut scenes = Vec::new();
        let mut assets = Vec::new();

        if manifest.schema_version == 1 {
            let scene_path = manifest.scene.as_deref().ok_or_else(|| {
                ZevyLevelAssetLoaderError::InvalidManifest(
                    "schema v1 requires a non-empty 'scene' field".to_owned(),
                )
            })?;
            let resolved_scene_path = resolve_scene_path(load_context.path(), scene_path)?;
            scenes.push(
                load_context.load(
                    GltfAssetLabel::Scene(manifest.scene_index).from_asset(resolved_scene_path),
                ),
            );
        } else {
            for manifest_asset in &manifest.assets {
                let resolved_scene_path =
                    resolve_scene_path(load_context.path(), &manifest_asset.scene)?;
                let scene_handle_index = scenes.len();
                scenes.push(
                    load_context.load(
                        GltfAssetLabel::Scene(manifest_asset.scene_index)
                            .from_asset(resolved_scene_path),
                    ),
                );
                assets.push(ZevyLevelSceneAsset {
                    id: manifest_asset.id.clone(),
                    name: manifest_asset.name.clone(),
                    scene_path: manifest_asset.scene.clone(),
                    scene_index: manifest_asset.scene_index,
                    scene_handle_index,
                });
            }
        }

        let entities = manifest
            .entities
            .into_iter()
            .map(|entity| ZevyLevelEntityDefinition {
                id: entity.id,
                name: entity.name,
                parent: entity.parent,
                asset: entity.asset,
                transform: entity.transform,
                visible: entity.visible,
                lights: entity.lights,
            })
            .collect();

        Ok(ZevyLevelAsset {
            schema_version: manifest.schema_version,
            level_name: manifest.level_name,
            scene_path: manifest.scene,
            scene_index: manifest.scene_index,
            scenes,
            assets,
            entities,
            source: manifest.source,
            content: manifest.content,
            export: manifest.export,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["zevy-level.json"]
    }
}

#[derive(Component, Debug)]
pub(super) struct PendingZevyLevel {
    handle: Handle<ZevyLevelAsset>,
    asset_path: String,
    failure_logged: bool,
}

#[derive(Component, Clone, Debug)]
#[allow(dead_code)]
pub struct ImportedZevyLevel {
    pub asset_path: String,
    pub level_name: String,
}

#[derive(Component, Clone, Debug)]
#[allow(dead_code)]
pub struct ImportedZevyEntity {
    pub id: String,
    pub asset_id: Option<String>,
}

#[derive(Component, Clone, Debug)]
pub struct ImportedZevyLight {
    pub source: ZevyLightDefinition,
}

#[derive(Component, Debug)]
struct PendingZevyLightOverrides {
    remaining: Vec<ZevyLightDefinition>,
    waited_frames: u16,
}

pub fn spawn_zevy_level(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_path: &str,
) -> (Entity, Handle<ZevyLevelAsset>) {
    let normalized_path = asset_path.replace('\\', "/");
    let handle = asset_server.load::<ZevyLevelAsset>(normalized_path.clone());

    let entity = commands
        .spawn((
            Name::new(format!("PendingZevyLevel:{normalized_path}")),
            LevelEntity,
            PendingZevyLevel {
                handle: handle.clone(),
                asset_path: normalized_path,
                failure_logged: false,
            },
        ))
        .id();
    (entity, handle)
}

pub(super) fn resolve_pending_levels(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    level_assets: Res<Assets<ZevyLevelAsset>>,
    mut pending_levels: Query<(Entity, &mut PendingZevyLevel)>,
) {
    for (entity, mut pending) in &mut pending_levels {
        if let Some(level) = level_assets.get(&pending.handle) {
            info!(
                "Loaded Zevy Level '{}' from '{}' (schema {}, composed {}, assets {}, entities {}, UE '{}', project '{}', map '{}', exported '{}', static mesh actors/components {}/{}, materials {}, lights directional/point/spot {}/{}/{}, format '{}', coordinates '{}', unit scale {}, texture '{}', material bake '{}', adjusted normal maps {}, exported lights {})",
                level.level_name,
                pending.asset_path,
                level.schema_version,
                level.is_composed(),
                level.assets.len(),
                level.entities.len(),
                level.source.unreal_engine_version,
                level.source.project_name,
                level.source.map_package,
                level.source.exported_at_utc,
                level.content.static_mesh_actors,
                level.content.static_mesh_components,
                level.content.unique_materials,
                level.content.directional_lights,
                level.content.point_lights,
                level.content.spot_lights,
                level.export.format,
                level.export.coordinate_system,
                level.export.unit_scale,
                level.export.texture_format,
                level.export.material_bake_mode,
                level.export.normal_maps_adjusted,
                level.export.lights_exported,
            );

            commands.entity(entity).insert((
                Name::new(level.level_name.clone()),
                Transform::IDENTITY,
                Visibility::Inherited,
                ImportedZevyLevel {
                    asset_path: pending.asset_path.clone(),
                    level_name: level.level_name.clone(),
                },
            ));

            if let Some(scene) = level.monolithic_scene() {
                commands.entity(entity).insert(SceneRoot(scene.clone()));
            } else {
                spawn_composed_level(&mut commands, entity, level);
            }

            commands.entity(entity).remove::<PendingZevyLevel>();
            continue;
        }

        if pending.failure_logged {
            continue;
        }

        if let LoadState::Failed(error) = asset_server.load_state(pending.handle.id()) {
            error!(
                "Failed to load Zevy Level '{}': {error}",
                pending.asset_path
            );
            pending.failure_logged = true;
        }
    }
}

fn spawn_composed_level(commands: &mut Commands, root: Entity, level: &ZevyLevelAsset) {
    let mut spawned_entities = HashMap::with_capacity(level.entities.len());

    for definition in &level.entities {
        let visibility = if definition.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let spawned = commands
            .spawn((
                Name::new(definition.name.clone()),
                definition.transform.to_bevy_transform(),
                visibility,
                ImportedZevyEntity {
                    id: definition.id.clone(),
                    asset_id: definition.asset.clone(),
                },
                ShadowCasterMotionPolicy::automatic(),
            ))
            .id();

        if let Some(asset_id) = definition.asset.as_deref() {
            let scene = level
                .scene_for_asset(asset_id)
                .expect("manifest asset references were validated while loading");
            commands.entity(spawned).insert(SceneRoot(scene.clone()));
        }

        if !definition.lights.is_empty() {
            commands.entity(spawned).insert(PendingZevyLightOverrides {
                remaining: definition.lights.clone(),
                waited_frames: 0,
            });
        }

        spawned_entities.insert(definition.id.clone(), spawned);
    }

    for definition in &level.entities {
        let child = spawned_entities[&definition.id];
        let parent = definition
            .parent
            .as_ref()
            .map(|parent_id| spawned_entities[parent_id])
            .unwrap_or(root);
        commands.entity(child).insert(ChildOf(parent));
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_pending_light_overrides(
    mut commands: Commands,
    children: Query<&Children>,
    names: Query<&Name>,
    mut pending_actors: Query<(Entity, &mut PendingZevyLightOverrides)>,
    mut point_lights: Query<&mut PointLight>,
    mut spot_lights: Query<&mut SpotLight>,
    mut directional_lights: Query<&mut DirectionalLight>,
) {
    for (actor_entity, mut pending) in &mut pending_actors {
        let descendants = children.iter_descendants(actor_entity).collect::<Vec<_>>();
        let mut unresolved = Vec::new();

        for definition in pending.remaining.drain(..) {
            let mut matched_entity = None;
            for descendant in descendants.iter().copied() {
                if names.get(descendant).map(Name::as_str).ok()
                    != Some(definition.gltf_name.as_str())
                {
                    continue;
                }

                let applied = match definition.kind {
                    ZevyLightKind::Point => point_lights.get_mut(descendant).map(|mut light| {
                        light.color = Color::srgb(
                            definition.bevy.color_srgb[0],
                            definition.bevy.color_srgb[1],
                            definition.bevy.color_srgb[2],
                        );
                        light.intensity = definition.bevy.intensity;
                        light.range = definition.bevy.range_m;
                        light.radius = definition.bevy.radius_m;
                        light.shadows_enabled = definition.bevy.shadows_enabled;
                    }),
                    ZevyLightKind::Spot => spot_lights.get_mut(descendant).map(|mut light| {
                        light.color = Color::srgb(
                            definition.bevy.color_srgb[0],
                            definition.bevy.color_srgb[1],
                            definition.bevy.color_srgb[2],
                        );
                        light.intensity = definition.bevy.intensity;
                        light.range = definition.bevy.range_m;
                        light.radius = definition.bevy.radius_m;
                        light.shadows_enabled = definition.bevy.shadows_enabled;
                        light.inner_angle = definition.bevy.inner_angle_radians;
                        light.outer_angle = definition.bevy.outer_angle_radians;
                    }),
                    ZevyLightKind::Directional => {
                        directional_lights.get_mut(descendant).map(|mut light| {
                            light.color = Color::srgb(
                                definition.bevy.color_srgb[0],
                                definition.bevy.color_srgb[1],
                                definition.bevy.color_srgb[2],
                            );
                            light.illuminance = definition.bevy.intensity;
                            light.shadows_enabled = definition.bevy.shadows_enabled;
                        })
                    }
                };

                if applied.is_ok() {
                    matched_entity = Some(descendant);
                    break;
                }
            }

            if let Some(light_entity) = matched_entity {
                let mut light_commands = commands.entity(light_entity);
                light_commands.insert((
                    ImportedZevyLight {
                        source: definition.clone(),
                    },
                    if definition.bevy.enabled {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                ));
                if definition.kind == ZevyLightKind::Point {
                    let policy = if definition.unreal.is_static_mobility() {
                        LightShadowMotionPolicy::fixed(LightShadowMotionClass::Static)
                    } else {
                        LightShadowMotionPolicy::automatic()
                    };
                    light_commands.insert(policy);
                }
                if definition.unreal.attenuation_model == "custom_exponent" {
                    warn!(
                        "UE light '{}' uses custom falloff exponent {}; Bevy uses its standard inverse-square cut-off, while the UE parameters remain available in ImportedZevyLight",
                        definition.component_name, definition.unreal.falloff_exponent,
                    );
                }
            } else {
                unresolved.push(definition);
            }
        }

        pending.remaining = unresolved;
        if pending.remaining.is_empty() {
            commands
                .entity(actor_entity)
                .remove::<PendingZevyLightOverrides>();
            continue;
        }

        pending.waited_frames = pending.waited_frames.saturating_add(1);
        if pending.waited_frames >= 600 {
            let missing = pending
                .remaining
                .iter()
                .map(|light| format!("{} ({:?})", light.gltf_name, light.kind))
                .collect::<Vec<_>>()
                .join(", ");
            error!(
                "Could not find imported glTF light nodes below Actor {:?}: {}",
                actor_entity, missing
            );
            commands
                .entity(actor_entity)
                .remove::<PendingZevyLightOverrides>();
        }
    }
}

fn parse_manifest(bytes: &[u8]) -> Result<ZevyLevelManifest, ZevyLevelAssetLoaderError> {
    let manifest = serde_json::from_slice::<ZevyLevelManifest>(bytes)?;

    if !(ZEVY_LEVEL_MIN_SCHEMA_VERSION..=ZEVY_LEVEL_SCHEMA_VERSION)
        .contains(&manifest.schema_version)
    {
        return Err(ZevyLevelAssetLoaderError::UnsupportedSchema {
            found: manifest.schema_version,
            supported_min: ZEVY_LEVEL_MIN_SCHEMA_VERSION,
            supported_max: ZEVY_LEVEL_SCHEMA_VERSION,
        });
    }

    if manifest.level_name.trim().is_empty() {
        return Err(ZevyLevelAssetLoaderError::EmptyLevelName);
    }

    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &ZevyLevelManifest) -> Result<(), ZevyLevelAssetLoaderError> {
    if manifest.schema_version == 1 {
        let scene = manifest.scene.as_deref().unwrap_or_default();
        if scene.trim().is_empty() {
            return Err(ZevyLevelAssetLoaderError::InvalidManifest(
                "schema v1 requires a non-empty 'scene' field".to_owned(),
            ));
        }
        return Ok(());
    }

    let mut asset_ids = HashSet::with_capacity(manifest.assets.len());
    for asset in &manifest.assets {
        if asset.id.trim().is_empty() {
            return Err(ZevyLevelAssetLoaderError::InvalidManifest(
                "asset id must not be empty".to_owned(),
            ));
        }
        if asset.scene.trim().is_empty() {
            return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
                "asset '{}' has an empty scene path",
                asset.id
            )));
        }
        if !asset_ids.insert(asset.id.as_str()) {
            return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
                "duplicate asset id '{}'",
                asset.id
            )));
        }
    }

    let mut entity_ids = HashSet::with_capacity(manifest.entities.len());
    let mut parents = HashMap::with_capacity(manifest.entities.len());
    for entity in &manifest.entities {
        if entity.id.trim().is_empty() {
            return Err(ZevyLevelAssetLoaderError::InvalidManifest(
                "entity id must not be empty".to_owned(),
            ));
        }
        if !entity_ids.insert(entity.id.as_str()) {
            return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
                "duplicate entity id '{}'",
                entity.id
            )));
        }
        if let Some(asset_id) = entity.asset.as_deref()
            && !asset_ids.contains(asset_id)
        {
            return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
                "entity '{}' references unknown asset '{}'",
                entity.id, asset_id
            )));
        }
        validate_transform(&entity.id, entity.transform)?;
        for light in &entity.lights {
            validate_light(&entity.id, light)?;
        }
        parents.insert(entity.id.as_str(), entity.parent.as_deref());
    }

    for entity in &manifest.entities {
        if let Some(parent_id) = entity.parent.as_deref()
            && !entity_ids.contains(parent_id)
        {
            return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
                "entity '{}' references unknown parent '{}'",
                entity.id, parent_id
            )));
        }
    }

    for start in entity_ids.iter().copied() {
        let mut chain = HashSet::new();
        let mut current = Some(start);
        while let Some(entity_id) = current {
            if !chain.insert(entity_id) {
                return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
                    "actor hierarchy contains a cycle involving entity '{}'",
                    entity_id
                )));
            }
            current = parents.get(entity_id).copied().flatten();
        }
    }

    Ok(())
}

fn validate_transform(
    entity_id: &str,
    transform: ZevyLevelTransform,
) -> Result<(), ZevyLevelAssetLoaderError> {
    let values_are_finite = transform
        .translation
        .into_iter()
        .chain(transform.rotation)
        .chain(transform.scale)
        .all(f32::is_finite);
    if !values_are_finite {
        return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
            "entity '{}' transform contains a non-finite number",
            entity_id
        )));
    }

    let rotation_length_squared = transform
        .rotation
        .into_iter()
        .map(|value| value * value)
        .sum::<f32>();
    if rotation_length_squared <= f32::EPSILON {
        return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
            "entity '{}' rotation quaternion must not be zero",
            entity_id
        )));
    }

    Ok(())
}

fn validate_light(
    entity_id: &str,
    light: &ZevyLightDefinition,
) -> Result<(), ZevyLevelAssetLoaderError> {
    if light.component_name.trim().is_empty() || light.gltf_name.trim().is_empty() {
        return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
            "entity '{}' contains a light with an empty component_name or gltf_name",
            entity_id
        )));
    }

    let bevy_values = [
        light.bevy.color_srgb[0],
        light.bevy.color_srgb[1],
        light.bevy.color_srgb[2],
        light.bevy.intensity,
        light.bevy.range_m,
        light.bevy.radius_m,
        light.bevy.inner_angle_radians,
        light.bevy.outer_angle_radians,
    ];
    if !bevy_values.into_iter().all(f32::is_finite) {
        return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
            "entity '{}' light '{}' contains a non-finite Bevy value",
            entity_id, light.component_name
        )));
    }
    if !light
        .bevy
        .color_srgb
        .into_iter()
        .all(|channel| (0.0..=1.0).contains(&channel))
        || light.bevy.intensity < 0.0
        || light.bevy.range_m < 0.0
        || light.bevy.radius_m < 0.0
    {
        return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
            "entity '{}' light '{}' has an invalid Bevy color, intensity, range, or radius",
            entity_id, light.component_name
        )));
    }

    let expected_unit = match light.kind {
        ZevyLightKind::Directional => "lux",
        ZevyLightKind::Point | ZevyLightKind::Spot => "lumens",
    };
    if light.bevy.intensity_unit != expected_unit {
        return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
            "entity '{}' light '{}' uses Bevy intensity unit '{}'; expected '{}'",
            entity_id, light.component_name, light.bevy.intensity_unit, expected_unit
        )));
    }

    if light.kind == ZevyLightKind::Spot
        && (light.bevy.inner_angle_radians < 0.0
            || light.bevy.outer_angle_radians <= 0.0
            || light.bevy.inner_angle_radians > light.bevy.outer_angle_radians
            || light.bevy.outer_angle_radians >= std::f32::consts::FRAC_PI_2)
    {
        return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
            "entity '{}' spot light '{}' has invalid cone angles",
            entity_id, light.component_name
        )));
    }

    let unreal_values = [
        light.unreal.intensity,
        light.unreal.light_color_srgb[0],
        light.unreal.light_color_srgb[1],
        light.unreal.light_color_srgb[2],
        light.unreal.temperature_kelvin,
        light.unreal.shadow_bias,
        light.unreal.shadow_slope_bias,
        light.unreal.attenuation_radius_m,
        light.unreal.inverse_exposure_blend,
        light.unreal.falloff_exponent,
        light.unreal.source_radius_m,
        light.unreal.soft_source_radius_m,
        light.unreal.source_length_m,
        light.unreal.inner_cone_angle_degrees,
        light.unreal.outer_cone_angle_degrees,
    ];
    if !unreal_values.into_iter().all(f32::is_finite)
        || !light
            .unreal
            .light_color_srgb
            .into_iter()
            .all(|channel| (0.0..=1.0).contains(&channel))
        || light.unreal.intensity < 0.0
        || light.unreal.attenuation_radius_m < 0.0
        || light.unreal.source_radius_m < 0.0
        || light.unreal.soft_source_radius_m < 0.0
        || light.unreal.source_length_m < 0.0
        || !(0.0..=1.0).contains(&light.unreal.inverse_exposure_blend)
        || (light.unreal.use_temperature && light.unreal.temperature_kelvin <= 0.0)
    {
        return Err(ZevyLevelAssetLoaderError::InvalidManifest(format!(
            "entity '{}' light '{}' has invalid retained Unreal parameters",
            entity_id, light.component_name
        )));
    }

    Ok(())
}

fn resolve_scene_path(
    manifest_path: &Path,
    scene: &str,
) -> Result<PathBuf, ZevyLevelAssetLoaderError> {
    let scene_path = Path::new(scene);
    let stays_in_level_folder = !scene_path.as_os_str().is_empty()
        && !scene_path.is_absolute()
        && scene_path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));

    if !stays_in_level_folder {
        return Err(ZevyLevelAssetLoaderError::InvalidScenePath(
            scene.to_owned(),
        ));
    }

    let extension = scene_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("glb") && !extension.eq_ignore_ascii_case("gltf") {
        return Err(ZevyLevelAssetLoaderError::UnsupportedSceneFormat(
            scene.to_owned(),
        ));
    }

    Ok(manifest_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(scene_path))
}

const fn identity_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

const fn unit_scale() -> [f32; 3] {
    [1.0; 3]
}

const fn default_visible() -> bool {
    true
}

const fn white_color() -> [f32; 3] {
    [1.0; 3]
}

const fn default_light_range() -> f32 {
    20.0
}

const fn default_spot_outer_angle() -> f32 {
    std::f32::consts::FRAC_PI_4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v1_manifest_and_defaults_scene_index() {
        let manifest = parse_manifest(
            br#"{
                "schema_version": 1,
                "level_name": "Demo",
                "scene": "Demo.glb"
            }"#,
        )
        .unwrap();

        assert_eq!(manifest.level_name, "Demo");
        assert_eq!(manifest.scene_index, 0);
        assert_eq!(manifest.scene.as_deref(), Some("Demo.glb"));
        assert_eq!(
            resolve_scene_path(
                Path::new("levels/Demo/Demo.zevy-level.json"),
                manifest.scene.as_deref().unwrap()
            )
            .unwrap(),
            PathBuf::from("levels/Demo/Demo.glb")
        );
    }

    #[test]
    fn parses_v2_actor_hierarchy_and_editable_local_transform() {
        let manifest = parse_manifest(
            br#"{
                "schema_version": 2,
                "level_name": "Demo",
                "assets": [
                    { "id": "crate", "scene": "assets/crate/crate.gltf" }
                ],
                "entities": [
                    {
                        "id": "parent",
                        "name": "Parent",
                        "transform": { "translation": [1.0, 2.0, 3.0] },
                        "lights": [{
                            "component_name": "PointLightComponent0",
                            "gltf_name": "Parent",
                            "kind": "point",
                            "bevy": {
                                "color_srgb": [1.0, 0.5, 0.25],
                                "intensity": 1200.0,
                                "intensity_unit": "lumens",
                                "attenuation_model": "inverse_square_cutoff",
                                "range_m": 8.0,
                                "radius_m": 0.1,
                                "shadows_enabled": true
                            },
                            "unreal": {
                                "intensity": 1200.0,
                                "intensity_units": "lumens",
                                "attenuation_radius_m": 8.0,
                                "attenuation_model": "inverse_square",
                                "falloff_exponent": 8.0
                            }
                        }]
                    },
                    {
                        "id": "child",
                        "name": "Child",
                        "parent": "parent",
                        "asset": "crate",
                        "transform": {
                            "rotation": [0.0, 0.70710677, 0.0, 0.70710677],
                            "scale": [2.0, 2.0, 2.0]
                        }
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(manifest.assets.len(), 1);
        assert_eq!(manifest.entities.len(), 2);
        assert_eq!(manifest.entities[1].parent.as_deref(), Some("parent"));
        assert_eq!(manifest.entities[1].asset.as_deref(), Some("crate"));
        assert_eq!(manifest.entities[0].transform.translation, [1.0, 2.0, 3.0]);
        assert_eq!(manifest.entities[0].lights.len(), 1);
        assert_eq!(manifest.entities[0].lights[0].kind, ZevyLightKind::Point);
        assert_eq!(manifest.entities[0].lights[0].bevy.intensity, 1200.0);
        assert_eq!(manifest.entities[0].lights[0].bevy.range_m, 8.0);
    }

    #[test]
    fn static_light_mobility_requires_an_explicit_static_value() {
        for mobility in ["static", " Static ", "STATIC"] {
            let parameters = ZevyUnrealLightParameters {
                mobility: mobility.to_owned(),
                ..default()
            };
            assert!(parameters.is_static_mobility());
        }

        for mobility in ["", "movable", "stationary"] {
            let parameters = ZevyUnrealLightParameters {
                mobility: mobility.to_owned(),
                ..default()
            };
            assert!(!parameters.is_static_mobility());
        }
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let error = parse_manifest(
            br#"{
                "schema_version": 3,
                "level_name": "Demo"
            }"#,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ZevyLevelAssetLoaderError::UnsupportedSchema {
                found: 3,
                supported_min: 1,
                supported_max: 2
            }
        ));
    }

    #[test]
    fn rejects_unknown_parent_and_hierarchy_cycle() {
        let unknown_parent = parse_manifest(
            br#"{
                "schema_version": 2,
                "level_name": "Demo",
                "entities": [{ "id": "child", "parent": "missing" }]
            }"#,
        )
        .unwrap_err();
        assert!(matches!(
            unknown_parent,
            ZevyLevelAssetLoaderError::InvalidManifest(_)
        ));

        let cycle = parse_manifest(
            br#"{
                "schema_version": 2,
                "level_name": "Demo",
                "entities": [
                    { "id": "a", "parent": "b" },
                    { "id": "b", "parent": "a" }
                ]
            }"#,
        )
        .unwrap_err();
        assert!(matches!(
            cycle,
            ZevyLevelAssetLoaderError::InvalidManifest(_)
        ));
    }

    #[test]
    fn rejects_scene_path_that_escapes_level_folder() {
        let error = resolve_scene_path(
            Path::new("levels/Demo/Demo.zevy-level.json"),
            "../Shared.glb",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ZevyLevelAssetLoaderError::InvalidScenePath(_)
        ));
    }

    #[test]
    fn rejects_non_gltf_scene_file() {
        let error = resolve_scene_path(Path::new("levels/Demo/Demo.zevy-level.json"), "Demo.fbx")
            .unwrap_err();

        assert!(matches!(
            error,
            ZevyLevelAssetLoaderError::UnsupportedSceneFormat(_)
        ));
    }

    #[test]
    fn rejects_invalid_reusable_light_parameters() {
        let error = parse_manifest(
            br#"{
                "schema_version": 2,
                "level_name": "Demo",
                "entities": [{
                    "id": "light",
                    "lights": [{
                        "component_name": "PointLightComponent0",
                        "gltf_name": "PointLight",
                        "kind": "point",
                        "bevy": {
                            "color_srgb": [1.0, 1.2, 1.0],
                            "intensity": 1000.0,
                            "intensity_unit": "candelas"
                        }
                    }]
                }]
            }"#,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ZevyLevelAssetLoaderError::InvalidManifest(_)
        ));
    }
}
