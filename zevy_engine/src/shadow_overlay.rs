use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicBool, Ordering},
};

use bevy::{
    core_pipeline::core_3d::CORE_3D_DEPTH_FORMAT,
    pbr::{
        DrawPrepass, ExtractedPointLight, GlobalClusterableObjectMeta, LightEntity,
        RenderCubemapVisibleEntities, Shadow, ShadowBatchSetKey, ShadowBinKey, ShadowView,
        SpecializedShadowMaterialPipelineCache, ViewShadowBindings, prepare_lights, queue_shadows,
        specialize_shadows,
    },
    prelude::*,
    render::{
        RenderApp, RenderSet,
        batching::gpu_preprocessing::{GpuPreprocessingMode, GpuPreprocessingSupport},
        mesh::allocator::MeshAllocator,
        render_phase::{BinnedRenderPhaseType, DrawFunctions, ViewBinnedRenderPhases},
        render_resource::{
            Extent3d, TextureAspect, TextureDescriptor, TextureDimension, TextureUsages,
            TextureViewDescriptor, TextureViewDimension,
        },
        sync_world::MainEntity,
        texture::{DepthAttachment, TextureCache},
        view::{ExtractedView, NoIndirectDrawing, RetainedViewEntity},
    },
};

use crate::shadow_cache::{PointShadowCacheKey, ZevyShadowCacheFrame};

/// Marks a mesh, or an actor root containing meshes, as a moving shadow
/// caster. Imported level geometry is static by default; meshes outside an
/// imported level hierarchy are classified as dynamic automatically.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DynamicShadowCaster;

#[derive(Component)]
pub(crate) struct DynamicPointShadowView {
    pub(crate) static_retained_view: RetainedViewEntity,
    pub(crate) light_entity: Entity,
    pub(crate) face_index: usize,
    pub(crate) key: PointShadowCacheKey,
    pub(crate) depth_attachment: DepthAttachment,
    rendered_this_frame: AtomicBool,
}

impl DynamicPointShadowView {
    pub(crate) fn claim_render(&self) -> bool {
        !self.rendered_this_frame.swap(true, Ordering::AcqRel)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DynamicAtlasSignature {
    face_size: usize,
    point_light_count: usize,
    cube_capacity: usize,
    dynamic_overlay_active: bool,
    layout: Vec<(PointShadowCacheKey, usize)>,
}

#[derive(Resource, Default)]
pub(crate) struct DynamicShadowOverlayState {
    dynamic_views: HashMap<PointShadowCacheKey, Entity>,
    atlas_signature: Option<DynamicAtlasSignature>,
    previous_active_keys: HashSet<PointShadowCacheKey>,
    clear_all_views: bool,
    pub(crate) render_view_entities: HashSet<Entity>,
    pub(crate) dynamic_view_by_static: HashMap<Entity, Entity>,
    pub(crate) enabled: bool,
    last_logged_counts: Option<(usize, usize)>,
}

pub(crate) struct DynamicShadowOverlayPlugin;

impl Plugin for DynamicShadowOverlayPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<DynamicShadowOverlayState>()
            .add_systems(
                bevy::render::Render,
                prepare_dynamic_point_shadow_overlay
                    .in_set(RenderSet::ManageViews)
                    .after(prepare_lights)
                    .before(specialize_shadows::<StandardMaterial>),
            )
            .add_systems(
                bevy::render::Render,
                mask_dynamic_casters_from_static_shadow_phase
                    .in_set(RenderSet::QueueMeshes)
                    .before(queue_shadows::<StandardMaterial>),
            )
            .add_systems(
                bevy::render::Render,
                queue_dynamic_shadow_overlay
                    .in_set(RenderSet::QueueMeshes)
                    .after(queue_shadows::<StandardMaterial>)
                    .after(mask_dynamic_casters_from_static_shadow_phase),
            );
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_dynamic_point_shadow_overlay(
    mut commands: Commands,
    frame: Res<ZevyShadowCacheFrame>,
    render_device: Res<bevy::render::renderer::RenderDevice>,
    mut texture_cache: ResMut<TextureCache>,
    clusterable_meta: Res<GlobalClusterableObjectMeta>,
    light_main_entities: Query<&MainEntity>,
    mut view_shadow_bindings: Query<&mut ViewShadowBindings>,
    mut static_shadow_views: Query<(Entity, &mut ShadowView, &ExtractedView, &LightEntity)>,
    mut state: ResMut<DynamicShadowOverlayState>,
) {
    state.render_view_entities.clear();
    state.dynamic_view_by_static.clear();
    state.enabled = frame.dynamic_overlay_enabled();
    if !state.enabled {
        remove_all_dynamic_views(&mut commands, &mut state);
        frame.record_dynamic_overlay(0);
        return;
    }

    struct PointViewRecord {
        static_view_entity: Entity,
        static_retained_view: RetainedViewEntity,
        light_entity: Entity,
        key: PointShadowCacheKey,
        face_index: usize,
        cube_index: usize,
        clip_from_view: Mat4,
        world_from_view: GlobalTransform,
        clip_from_world: Option<Mat4>,
        hdr: bool,
        viewport: UVec4,
        color_grading: bevy::render::view::ColorGrading,
    }

    let mut records = Vec::new();
    let mut point_light_count = 0;
    for (static_view_entity, _, extracted_view, light_entity) in &mut static_shadow_views {
        let LightEntity::Point {
            light_entity,
            face_index,
        } = light_entity
        else {
            continue;
        };
        let Some(&cube_index) = clusterable_meta.entity_to_index.get(light_entity) else {
            continue;
        };
        let Ok(main_entity) = light_main_entities.get(*light_entity) else {
            continue;
        };
        point_light_count = point_light_count.max(cube_index + 1);
        records.push(PointViewRecord {
            static_view_entity,
            static_retained_view: extracted_view.retained_view_entity,
            light_entity: *light_entity,
            key: PointShadowCacheKey {
                light: main_entity.id(),
                face: *face_index,
            },
            face_index: *face_index,
            cube_index,
            clip_from_view: extracted_view.clip_from_view,
            world_from_view: extracted_view.world_from_view,
            clip_from_world: extracted_view.clip_from_world,
            hdr: extracted_view.hdr,
            viewport: extracted_view.viewport,
            color_grading: extracted_view.color_grading.clone(),
        });
    }

    if records.is_empty() || point_light_count == 0 {
        remove_all_dynamic_views(&mut commands, &mut state);
        frame.record_dynamic_overlay(0);
        return;
    }

    let dynamic_overlay_active = frame.dynamic_overlay_active();
    let cube_capacity = point_shadow_cube_capacity(point_light_count, dynamic_overlay_active);
    let required_array_layers = cube_capacity * 6;
    assert!(
        required_array_layers <= render_device.limits().max_texture_array_layers as usize,
        "Zevy dynamic point-shadow overlay needs {required_array_layers} texture-array layers, but this device supports only {}. Set RenderQualityConfig.max_shadowed_point_lights to at most {}.",
        render_device.limits().max_texture_array_layers,
        render_device
            .limits()
            .max_texture_array_layers
            .saturating_sub(6)
            / 12,
    );

    let face_size = frame.point_shadow_map_size();
    let atlas = texture_cache.get(
        &render_device,
        TextureDescriptor {
            label: Some("zevy_static_dynamic_point_shadow_atlas"),
            size: Extent3d {
                width: face_size as u32,
                height: face_size as u32,
                depth_or_array_layers: required_array_layers as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: CORE_3D_DEPTH_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
    );
    let full_cube_array_view = atlas.texture.create_view(&TextureViewDescriptor {
        label: Some("zevy_static_dynamic_point_shadow_cube_array_view"),
        dimension: Some(TextureViewDimension::CubeArray),
        aspect: TextureAspect::DepthOnly,
        ..default()
    });

    for mut bindings in &mut view_shadow_bindings {
        bindings.point_light_depth_texture = atlas.texture.clone();
        bindings.point_light_depth_texture_view = full_cube_array_view.clone();
    }

    let mut layout = records
        .iter()
        .map(|record| (record.key, record.cube_index))
        .collect::<Vec<_>>();
    layout.sort_unstable();
    layout.dedup();
    let signature = DynamicAtlasSignature {
        face_size,
        point_light_count,
        cube_capacity,
        dynamic_overlay_active,
        layout,
    };
    if state.atlas_signature.as_ref() != Some(&signature) {
        state.atlas_signature = Some(signature);
        state.clear_all_views = dynamic_overlay_active;
        state.previous_active_keys.clear();
    }

    let live_keys = if dynamic_overlay_active {
        records
            .iter()
            .map(|record| record.key)
            .collect::<HashSet<_>>()
    } else {
        HashSet::new()
    };
    let stale_keys = state
        .dynamic_views
        .keys()
        .filter(|key| !live_keys.contains(key))
        .copied()
        .collect::<Vec<_>>();
    for key in stale_keys {
        if let Some(entity) = state.dynamic_views.remove(&key) {
            commands.entity(entity).despawn();
        }
        state.previous_active_keys.remove(&key);
    }

    let mut configured_dynamic_views = HashMap::new();
    for record in records {
        let static_layer = record.cube_index * 6 + record.face_index;
        let static_face_view = atlas.texture.create_view(&single_depth_layer_view(
            "zevy_static_point_shadow_face",
            static_layer,
        ));

        if let Ok((_, mut shadow_view, _, _)) =
            static_shadow_views.get_mut(record.static_view_entity)
        {
            shadow_view.depth_attachment = DepthAttachment::new(static_face_view, Some(0.0));
        }

        if !dynamic_overlay_active {
            continue;
        }

        let dynamic_layer = (point_light_count + record.cube_index) * 6 + record.face_index;
        let dynamic_face_view = atlas.texture.create_view(&single_depth_layer_view(
            "zevy_dynamic_point_shadow_face",
            dynamic_layer,
        ));

        if let Some(&dynamic_view_entity) = configured_dynamic_views.get(&record.key) {
            state
                .dynamic_view_by_static
                .insert(record.static_view_entity, dynamic_view_entity);
            continue;
        }

        let retained_view_entity = RetainedViewEntity {
            main_entity: record.static_retained_view.main_entity,
            auxiliary_entity: record.static_retained_view.auxiliary_entity,
            subview_index: record.static_retained_view.subview_index + 6,
        };
        let dynamic_view = DynamicPointShadowView {
            static_retained_view: record.static_retained_view,
            light_entity: record.light_entity,
            face_index: record.face_index,
            key: record.key,
            depth_attachment: DepthAttachment::new(dynamic_face_view, Some(0.0)),
            rendered_this_frame: AtomicBool::new(false),
        };
        let extracted_view = ExtractedView {
            retained_view_entity,
            clip_from_view: record.clip_from_view,
            world_from_view: record.world_from_view,
            clip_from_world: record.clip_from_world,
            hdr: record.hdr,
            viewport: record.viewport,
            color_grading: record.color_grading,
        };

        let dynamic_view_entity = if let Some(&entity) = state.dynamic_views.get(&record.key) {
            commands
                .entity(entity)
                .insert((dynamic_view, extracted_view, NoIndirectDrawing));
            entity
        } else {
            let entity = commands
                .spawn((dynamic_view, extracted_view, NoIndirectDrawing))
                .id();
            state.dynamic_views.insert(record.key, entity);
            entity
        };
        configured_dynamic_views.insert(record.key, dynamic_view_entity);
        state
            .dynamic_view_by_static
            .insert(record.static_view_entity, dynamic_view_entity);
    }
}

fn single_depth_layer_view(label: &'static str, layer: usize) -> TextureViewDescriptor<'static> {
    TextureViewDescriptor {
        label: Some(label),
        dimension: Some(TextureViewDimension::D2),
        aspect: TextureAspect::DepthOnly,
        base_array_layer: layer as u32,
        array_layer_count: Some(1),
        ..default()
    }
}

fn point_shadow_cube_capacity(point_light_count: usize, dynamic_overlay_active: bool) -> usize {
    if dynamic_overlay_active {
        point_light_count * 2 + 1
    } else {
        point_light_count + point_light_count % 2
    }
}

fn remove_all_dynamic_views(commands: &mut Commands, state: &mut DynamicShadowOverlayState) {
    for (_, entity) in state.dynamic_views.drain() {
        commands.entity(entity).despawn();
    }
    state.atlas_signature = None;
    state.previous_active_keys.clear();
    state.render_view_entities.clear();
    state.dynamic_view_by_static.clear();
    state.clear_all_views = false;
}

fn mask_dynamic_casters_from_static_shadow_phase(
    frame: Res<ZevyShadowCacheFrame>,
    mut render_mesh_instances: ResMut<bevy::pbr::RenderMeshInstances>,
) {
    if !frame.dynamic_overlay_active() {
        return;
    }
    set_dynamic_shadow_caster_flags(
        &mut render_mesh_instances,
        frame.dynamic_shadow_casters(),
        false,
    );
}

#[allow(clippy::too_many_arguments)]
fn queue_dynamic_shadow_overlay(
    frame: Res<ZevyShadowCacheFrame>,
    mut state: ResMut<DynamicShadowOverlayState>,
    shadow_draw_functions: Res<DrawFunctions<Shadow>>,
    mut render_mesh_instances: ResMut<bevy::pbr::RenderMeshInstances>,
    mut shadow_render_phases: ResMut<ViewBinnedRenderPhases<Shadow>>,
    gpu_preprocessing_support: Res<GpuPreprocessingSupport>,
    mesh_allocator: Res<MeshAllocator>,
    dynamic_views: Query<(Entity, &DynamicPointShadowView, &ExtractedView)>,
    point_light_entities: Query<&RenderCubemapVisibleEntities, With<ExtractedPointLight>>,
    specialized_pipeline_cache: Res<SpecializedShadowMaterialPipelineCache<StandardMaterial>>,
) {
    set_dynamic_shadow_caster_flags(
        &mut render_mesh_instances,
        frame.dynamic_shadow_casters(),
        true,
    );
    state.render_view_entities.clear();
    if !frame.dynamic_overlay_active() || !state.enabled {
        frame.record_dynamic_overlay(0);
        return;
    }

    let dynamic_casters = frame
        .dynamic_shadow_casters()
        .iter()
        .copied()
        .map(MainEntity::from)
        .collect::<HashSet<_>>();
    let draw_shadow_mesh = shadow_draw_functions
        .read()
        .id::<DrawPrepass<StandardMaterial>>();
    let mut current_active_keys = HashSet::new();
    let mut dynamic_entity_by_key = HashMap::new();

    for (dynamic_view_entity, dynamic_view, extracted_view) in &dynamic_views {
        dynamic_entity_by_key.insert(dynamic_view.key, dynamic_view_entity);
        shadow_render_phases.prepare_for_new_frame(
            extracted_view.retained_view_entity,
            GpuPreprocessingMode::PreprocessingOnly,
        );
        let Some(shadow_phase) = shadow_render_phases.get_mut(&extracted_view.retained_view_entity)
        else {
            continue;
        };
        let Ok(visible_faces) = point_light_entities.get(dynamic_view.light_entity) else {
            continue;
        };
        let visible_entities = visible_faces.get(dynamic_view.face_index);
        let visible_dynamic_entities = visible_entities
            .iter()
            .copied()
            .filter(|(_, main_entity)| dynamic_casters.contains(main_entity))
            .collect::<Vec<_>>();
        if !visible_dynamic_entities.is_empty() {
            current_active_keys.insert(dynamic_view.key);
        }

        let Some(view_pipeline_cache) =
            specialized_pipeline_cache.get(&dynamic_view.static_retained_view)
        else {
            continue;
        };
        for (entity, main_entity) in visible_dynamic_entities {
            let Some((current_change_tick, pipeline_id)) = view_pipeline_cache.get(&main_entity)
            else {
                continue;
            };
            if shadow_phase.validate_cached_entity(main_entity, *current_change_tick) {
                continue;
            }
            let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(main_entity)
            else {
                continue;
            };
            let (vertex_slab, index_slab) = mesh_allocator.mesh_slabs(&mesh_instance.mesh_asset_id);
            shadow_phase.add(
                ShadowBatchSetKey {
                    pipeline: *pipeline_id,
                    draw_function: draw_shadow_mesh,
                    material_bind_group_index: Some(mesh_instance.material_bindings_index.group.0),
                    vertex_slab: vertex_slab.unwrap_or_default(),
                    index_slab,
                },
                ShadowBinKey {
                    asset_id: mesh_instance.mesh_asset_id.into(),
                },
                (entity, main_entity),
                mesh_instance.current_uniform_index,
                BinnedRenderPhaseType::mesh(
                    mesh_instance.should_batch(),
                    &gpu_preprocessing_support,
                ),
                *current_change_tick,
            );
        }
    }

    let render_keys = overlay_keys_to_render(
        &current_active_keys,
        &state.previous_active_keys,
        state.clear_all_views,
        dynamic_entity_by_key.keys().copied(),
    );
    state.clear_all_views = false;
    for key in render_keys {
        if let Some(entity) = dynamic_entity_by_key.get(&key) {
            state.render_view_entities.insert(*entity);
        }
    }
    state.previous_active_keys = current_active_keys;
    frame.record_dynamic_overlay(state.render_view_entities.len());
    let counts = (
        frame.dynamic_shadow_casters().len(),
        state.render_view_entities.len(),
    );
    if state.last_logged_counts != Some(counts) {
        debug!(
            "Dynamic point-shadow overlay: {} casters, {} cubemap faces redrawn this frame",
            counts.0, counts.1,
        );
        state.last_logged_counts = Some(counts);
    }
}

fn overlay_keys_to_render<T: Copy + Eq + std::hash::Hash>(
    current: &HashSet<T>,
    previous: &HashSet<T>,
    clear_all: bool,
    all_keys: impl IntoIterator<Item = T>,
) -> HashSet<T> {
    let mut render_keys = current.union(previous).copied().collect::<HashSet<_>>();
    if clear_all {
        render_keys.extend(all_keys);
    }
    render_keys
}

fn set_dynamic_shadow_caster_flags(
    render_mesh_instances: &mut bevy::pbr::RenderMeshInstances,
    dynamic_casters: &[Entity],
    enabled: bool,
) {
    match render_mesh_instances {
        bevy::pbr::RenderMeshInstances::CpuBuilding(instances) => {
            for entity in dynamic_casters {
                if let Some(instance) = instances.get_mut(&MainEntity::from(*entity)) {
                    instance
                        .flags
                        .set(bevy::pbr::RenderMeshInstanceFlags::SHADOW_CASTER, enabled);
                }
            }
        }
        bevy::pbr::RenderMeshInstances::GpuBuilding(instances) => {
            for entity in dynamic_casters {
                if let Some(instance) = instances.get_mut(&MainEntity::from(*entity)) {
                    instance
                        .flags
                        .set(bevy::pbr::RenderMeshInstanceFlags::SHADOW_CASTER, enabled);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{overlay_keys_to_render, point_shadow_cube_capacity};

    #[test]
    fn dual_atlas_uses_matching_static_and_dynamic_cube_indices() {
        let point_light_count = 16;
        let cube_index = 9;
        let face_index = 4;
        let static_layer = cube_index * 6 + face_index;
        let dynamic_layer = (point_light_count + cube_index) * 6 + face_index;
        assert_eq!(static_layer, 58);
        assert_eq!(dynamic_layer, 154);
        assert_eq!(dynamic_layer - static_layer, point_light_count * 6);
    }

    #[test]
    fn atlas_parity_disables_the_second_sample_when_no_dynamic_caster_exists() {
        assert_eq!(point_shadow_cube_capacity(16, false), 16);
        assert_eq!(point_shadow_cube_capacity(15, false), 16);
        assert_eq!(point_shadow_cube_capacity(16, true), 33);
        assert_eq!(point_shadow_cube_capacity(15, true), 31);
    }

    #[test]
    fn previous_dynamic_faces_are_cleared_after_a_caster_leaves() {
        let current = HashSet::from([2_u32]);
        let previous = HashSet::from([1_u32]);
        let render = overlay_keys_to_render(&current, &previous, false, []);
        assert_eq!(render, HashSet::from([1, 2]));
    }

    #[test]
    fn a_new_dual_atlas_clears_every_dynamic_face_once() {
        let render =
            overlay_keys_to_render(&HashSet::<u32>::new(), &HashSet::new(), true, [0, 1, 2, 3]);
        assert_eq!(render, HashSet::from([0, 1, 2, 3]));
    }
}
