use std::{
    any::type_name,
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicBool, Ordering},
};

#[cfg(feature = "render_debug")]
use std::sync::{Arc, atomic::AtomicU64};

use bevy::{
    core_pipeline::core_3d::graph::Core3d,
    pbr::{LightEntity, NotShadowCaster, Shadow, ShadowView, ViewLightEntities, graph::NodePbr},
    prelude::*,
    render::{
        RenderApp,
        diagnostic::RecordDiagnostics,
        experimental::occlusion_culling::OcclusionCulling,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_graph::{Node, NodeRunError, RenderGraph, RenderGraphContext},
        render_phase::{TrackedRenderPass, ViewBinnedRenderPhases},
        render_resource::{CommandEncoderDescriptor, RenderPassDescriptor, StoreOp},
        renderer::RenderContext,
        sync_world::MainEntity,
        view::ExtractedView,
    },
    transform::TransformSystem,
};

use crate::shadow_overlay::{
    DynamicPointShadowView, DynamicShadowCaster, DynamicShadowOverlayState,
};
use crate::{config::RenderQualityConfig, scene::ImportedZevyEntity};

#[derive(Component, Debug, Default)]
pub(crate) struct CachedPointLightShadow {
    pub(crate) last_update_seconds: Option<f32>,
}

#[derive(Clone, Default)]
struct ShadowCacheTelemetry {
    #[cfg(feature = "render_debug")]
    rendered_views: Arc<AtomicU64>,
    #[cfg(feature = "render_debug")]
    reused_views: Arc<AtomicU64>,
    #[cfg(feature = "render_debug")]
    resident_views: Arc<AtomicU64>,
    #[cfg(feature = "render_debug")]
    invalidated_lights: Arc<AtomicU64>,
    #[cfg(feature = "render_debug")]
    dynamic_views_rendered: Arc<AtomicU64>,
    #[cfg(feature = "render_debug")]
    dynamic_casters: Arc<AtomicU64>,
}

#[cfg(feature = "render_debug")]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShadowCacheTelemetrySnapshot {
    pub(crate) rendered_views: u64,
    pub(crate) reused_views: u64,
    pub(crate) resident_views: u64,
    pub(crate) invalidated_lights: u64,
    pub(crate) dynamic_views_rendered: u64,
    pub(crate) dynamic_casters: u64,
}

impl ShadowCacheTelemetry {
    fn store(&self, rendered: usize, reused: usize, resident: usize, invalidated: usize) {
        #[cfg(feature = "render_debug")]
        {
            self.rendered_views
                .store(rendered as u64, Ordering::Relaxed);
            self.reused_views.store(reused as u64, Ordering::Relaxed);
            self.resident_views
                .store(resident as u64, Ordering::Relaxed);
            self.invalidated_lights
                .store(invalidated as u64, Ordering::Relaxed);
        }
        #[cfg(not(feature = "render_debug"))]
        let _ = (rendered, reused, resident, invalidated);
    }

    #[cfg(feature = "render_debug")]
    fn snapshot(&self) -> ShadowCacheTelemetrySnapshot {
        ShadowCacheTelemetrySnapshot {
            rendered_views: self.rendered_views.load(Ordering::Relaxed),
            reused_views: self.reused_views.load(Ordering::Relaxed),
            resident_views: self.resident_views.load(Ordering::Relaxed),
            invalidated_lights: self.invalidated_lights.load(Ordering::Relaxed),
            dynamic_views_rendered: self.dynamic_views_rendered.load(Ordering::Relaxed),
            dynamic_casters: self.dynamic_casters.load(Ordering::Relaxed),
        }
    }

    fn store_dynamic(&self, rendered_views: usize, caster_count: usize) {
        #[cfg(feature = "render_debug")]
        {
            self.dynamic_views_rendered
                .store(rendered_views as u64, Ordering::Relaxed);
            self.dynamic_casters
                .store(caster_count as u64, Ordering::Relaxed);
        }
        #[cfg(not(feature = "render_debug"))]
        let _ = (rendered_views, caster_count);
    }
}

#[derive(Resource, Clone, ExtractResource)]
pub(crate) struct ZevyShadowCacheFrame {
    enabled: bool,
    warmup_frames: u8,
    point_shadow_map_size: usize,
    dynamic_overlay_enabled: bool,
    cacheable_point_lights: Vec<Entity>,
    dynamic_shadow_casters: Vec<Entity>,
    invalidated_point_lights: Vec<Entity>,
    invalidate_all: bool,
    telemetry: ShadowCacheTelemetry,
}

impl FromWorld for ZevyShadowCacheFrame {
    fn from_world(world: &mut World) -> Self {
        let quality = world
            .get_resource::<RenderQualityConfig>()
            .copied()
            .unwrap_or_default();
        Self {
            enabled: quality.persistent_point_shadow_cache,
            warmup_frames: quality.resolved_point_shadow_cache_warmup_frames(),
            point_shadow_map_size: quality.resolved_point_shadow_map_size(),
            dynamic_overlay_enabled: quality.point_light_shadows
                && quality.persistent_point_shadow_cache
                && quality.scalable_point_lighting
                && quality.dynamic_shadow_caster_overlay,
            cacheable_point_lights: Vec::new(),
            dynamic_shadow_casters: Vec::new(),
            invalidated_point_lights: Vec::new(),
            invalidate_all: true,
            telemetry: ShadowCacheTelemetry::default(),
        }
    }
}

impl ZevyShadowCacheFrame {
    pub(crate) fn invalidate_point_light(&mut self, entity: Entity) {
        if !self.invalidated_point_lights.contains(&entity) {
            self.invalidated_point_lights.push(entity);
        }
    }

    #[cfg(feature = "render_debug")]
    pub(crate) fn telemetry(&self) -> ShadowCacheTelemetrySnapshot {
        self.telemetry.snapshot()
    }

    pub(crate) fn dynamic_overlay_enabled(&self) -> bool {
        self.enabled && self.dynamic_overlay_enabled
    }

    pub(crate) fn dynamic_overlay_active(&self) -> bool {
        self.dynamic_overlay_enabled() && !self.dynamic_shadow_casters.is_empty()
    }

    pub(crate) fn point_shadow_map_size(&self) -> usize {
        self.point_shadow_map_size
    }

    pub(crate) fn dynamic_shadow_casters(&self) -> &[Entity] {
        &self.dynamic_shadow_casters
    }

    pub(crate) fn record_dynamic_overlay(&self, rendered_views: usize) {
        self.telemetry
            .store_dynamic(rendered_views, self.dynamic_shadow_casters.len());
    }
}

pub(crate) struct ShadowCachePlugin;

impl Plugin for ShadowCachePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ZevyShadowCacheFrame>()
            .add_plugins(ExtractResourcePlugin::<ZevyShadowCacheFrame>::default())
            .add_systems(PreUpdate, begin_shadow_cache_frame)
            .add_systems(
                PostUpdate,
                finalize_shadow_cache_frame.after(TransformSystem::TransformPropagate),
            );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        let node = ZevyCachedEarlyShadowPassNode::from_world(render_app.world_mut());
        let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
        let draw_3d_graph = graph
            .get_sub_graph_mut(Core3d)
            .expect("Core3d graph must exist before ShadowCachePlugin");
        let node_state = draw_3d_graph
            .get_node_state_mut(NodePbr::EarlyShadowPass)
            .expect("Bevy EarlyShadowPass must exist before ShadowCachePlugin");
        node_state.node = Box::new(node);
        node_state.type_name = type_name::<ZevyCachedEarlyShadowPassNode>();
    }
}

fn begin_shadow_cache_frame(
    quality: Res<RenderQualityConfig>,
    mut frame: ResMut<ZevyShadowCacheFrame>,
) {
    frame.enabled = quality.persistent_point_shadow_cache;
    frame.warmup_frames = quality.resolved_point_shadow_cache_warmup_frames();
    frame.point_shadow_map_size = quality.resolved_point_shadow_map_size();
    frame.dynamic_overlay_enabled = quality.point_light_shadows
        && quality.persistent_point_shadow_cache
        && quality.scalable_point_lighting
        && quality.dynamic_shadow_caster_overlay;
    frame.cacheable_point_lights.clear();
    frame.dynamic_shadow_casters.clear();
    frame.invalidated_point_lights.clear();
    frame.invalidate_all = false;
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn finalize_shadow_cache_frame(
    mut frame: ResMut<ZevyShadowCacheFrame>,
    cacheable_lights: Query<(Entity, &PointLight, &GlobalTransform), With<CachedPointLightShadow>>,
    imported_actor_changes: Query<
        Entity,
        (
            With<ImportedZevyEntity>,
            Or<(Changed<GlobalTransform>, Changed<Visibility>)>,
        ),
    >,
    changed_meshes: Query<Entity, (With<Mesh3d>, Changed<Mesh3d>, Without<NotShadowCaster>)>,
    changed_caster_transforms: Query<
        Entity,
        (
            With<Mesh3d>,
            Changed<GlobalTransform>,
            Without<NotShadowCaster>,
        ),
    >,
    shadow_casters: Query<Entity, (With<Mesh3d>, Without<NotShadowCaster>)>,
    dynamic_markers: Query<(), With<DynamicShadowCaster>>,
    imported_entities: Query<(), With<ImportedZevyEntity>>,
    parents: Query<&ChildOf>,
    mut previous_shadow_caster_count: Local<Option<usize>>,
    mut previous_light_projection: Local<HashMap<Entity, PointShadowProjectionState>>,
) {
    let mut active_cacheable_lights = HashSet::new();
    for (entity, light, global_transform) in &cacheable_lights {
        frame.cacheable_point_lights.push(entity);
        active_cacheable_lights.insert(entity);

        let projection = PointShadowProjectionState::new(light, global_transform);
        if previous_light_projection
            .insert(entity, projection)
            .is_some_and(|previous| previous != projection)
        {
            frame.invalidate_point_light(entity);
        }
    }
    previous_light_projection.retain(|entity, _| active_cacheable_lights.contains(entity));
    frame
        .cacheable_point_lights
        .sort_unstable_by_key(|entity| entity.index());

    let mut dynamic_casters = HashSet::new();
    let mut static_caster_count = 0;
    let separate_dynamic_overlay = frame.dynamic_overlay_enabled();
    for entity in &shadow_casters {
        if is_dynamic_shadow_caster(entity, &dynamic_markers, &imported_entities, &parents) {
            dynamic_casters.insert(entity);
            if !separate_dynamic_overlay {
                static_caster_count += 1;
            }
        } else {
            static_caster_count += 1;
        }
    }
    frame.dynamic_shadow_casters.extend(dynamic_casters.iter());
    frame
        .dynamic_shadow_casters
        .sort_unstable_by_key(|entity| entity.index());

    let caster_count_changed = previous_shadow_caster_count
        .replace(static_caster_count)
        .is_none_or(|previous| previous != static_caster_count);
    let static_actor_changed = imported_actor_changes.iter().any(|entity| {
        !separate_dynamic_overlay || !has_dynamic_shadow_marker(entity, &dynamic_markers, &parents)
    });
    let static_mesh_changed = changed_meshes
        .iter()
        .chain(changed_caster_transforms.iter())
        .any(|entity| !separate_dynamic_overlay || !dynamic_casters.contains(&entity));
    if caster_count_changed || static_actor_changed || static_mesh_changed {
        frame.invalidate_all = true;
    }
}

fn has_dynamic_shadow_marker(
    entity: Entity,
    dynamic_markers: &Query<(), With<DynamicShadowCaster>>,
    parents: &Query<&ChildOf>,
) -> bool {
    let mut current = Some(entity);
    for _ in 0..128 {
        let Some(entity) = current else {
            return false;
        };
        if dynamic_markers.contains(entity) {
            return true;
        }
        current = parents.get(entity).ok().map(ChildOf::parent);
    }
    false
}

pub(crate) fn is_dynamic_shadow_caster(
    entity: Entity,
    dynamic_markers: &Query<(), With<DynamicShadowCaster>>,
    imported_entities: &Query<(), With<ImportedZevyEntity>>,
    parents: &Query<&ChildOf>,
) -> bool {
    let mut current = Some(entity);
    let mut belongs_to_imported_level = false;
    for _ in 0..128 {
        let Some(entity) = current else {
            break;
        };
        if dynamic_markers.contains(entity) {
            return true;
        }
        belongs_to_imported_level |= imported_entities.contains(entity);
        current = parents.get(entity).ok().map(ChildOf::parent);
    }
    !belongs_to_imported_level
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PointShadowProjectionState {
    translation: [u32; 3],
    range: u32,
    near_z: u32,
}

impl PointShadowProjectionState {
    fn new(light: &PointLight, global_transform: &GlobalTransform) -> Self {
        let translation = global_transform.translation();
        Self {
            translation: [
                translation.x.to_bits(),
                translation.y.to_bits(),
                translation.z.to_bits(),
            ],
            range: light.range.to_bits(),
            near_z: light.shadow_map_near_z.to_bits(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct PointShadowCacheKey {
    pub(crate) light: Entity,
    pub(crate) face: usize,
}

struct ZevyCachedEarlyShadowPassNode {
    main_view_query: QueryState<&'static ViewLightEntities>,
    shadow_view_query: QueryState<(
        Entity,
        &'static ShadowView,
        &'static ExtractedView,
        Has<OcclusionCulling>,
        &'static LightEntity,
    )>,
    light_main_entity_query: QueryState<&'static MainEntity>,
    dynamic_shadow_view_query: QueryState<(
        Entity,
        &'static DynamicPointShadowView,
        &'static ExtractedView,
    )>,
    render_view_entities: HashSet<Entity>,
    point_key_by_view: HashMap<Entity, PointShadowCacheKey>,
    point_render_claims: HashMap<PointShadowCacheKey, AtomicBool>,
    dynamic_view_by_static: HashMap<Entity, Entity>,
    render_dynamic_view_entities: HashSet<Entity>,
    cache_entries: HashMap<PointShadowCacheKey, u8>,
    atlas_light_layout: Vec<Entity>,
    atlas_size: usize,
}

impl FromWorld for ZevyCachedEarlyShadowPassNode {
    fn from_world(world: &mut World) -> Self {
        Self {
            main_view_query: QueryState::new(world),
            shadow_view_query: QueryState::new(world),
            light_main_entity_query: QueryState::new(world),
            dynamic_shadow_view_query: QueryState::new(world),
            render_view_entities: HashSet::new(),
            point_key_by_view: HashMap::new(),
            point_render_claims: HashMap::new(),
            dynamic_view_by_static: HashMap::new(),
            render_dynamic_view_entities: HashSet::new(),
            cache_entries: HashMap::new(),
            atlas_light_layout: Vec::new(),
            atlas_size: 0,
        }
    }
}

impl Node for ZevyCachedEarlyShadowPassNode {
    fn update(&mut self, world: &mut World) {
        self.main_view_query.update_archetypes(world);
        self.shadow_view_query.update_archetypes(world);
        self.light_main_entity_query.update_archetypes(world);
        self.dynamic_shadow_view_query.update_archetypes(world);
        self.render_view_entities.clear();
        self.point_key_by_view.clear();
        self.point_render_claims.clear();
        self.dynamic_view_by_static.clear();
        self.render_dynamic_view_entities.clear();

        if let Some(overlay_state) = world.get_resource::<DynamicShadowOverlayState>() {
            self.render_dynamic_view_entities
                .extend(overlay_state.render_view_entities.iter().copied());
        }
        if let Some(overlay_state) = world.get_resource::<DynamicShadowOverlayState>() {
            self.dynamic_view_by_static.extend(
                overlay_state
                    .dynamic_view_by_static
                    .iter()
                    .map(|(key, value)| (*key, *value)),
            );
        }

        let Some(frame) = world.get_resource::<ZevyShadowCacheFrame>() else {
            self.render_view_entities.extend(
                self.shadow_view_query
                    .iter_manual(world)
                    .map(|(entity, ..)| entity),
            );
            return;
        };

        let cacheable_lights = frame
            .cacheable_point_lights
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let dirty_lights = frame
            .invalidated_point_lights
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let mut point_views = Vec::new();
        let mut atlas_lights = Vec::new();

        for (view_entity, _, _, occlusion_culling, light_entity) in
            self.shadow_view_query.iter_manual(world)
        {
            let LightEntity::Point {
                light_entity,
                face_index,
            } = light_entity
            else {
                self.render_view_entities.insert(view_entity);
                continue;
            };
            let Ok(main_entity) = self
                .light_main_entity_query
                .get_manual(world, *light_entity)
            else {
                self.render_view_entities.insert(view_entity);
                continue;
            };
            let main_entity = main_entity.id();
            atlas_lights.push(main_entity);
            point_views.push((
                view_entity,
                PointShadowCacheKey {
                    light: main_entity,
                    face: *face_index,
                },
                cacheable_lights.contains(&main_entity) && !occlusion_culling,
            ));
            self.point_key_by_view.insert(
                view_entity,
                PointShadowCacheKey {
                    light: main_entity,
                    face: *face_index,
                },
            );
        }

        atlas_lights.sort_unstable_by_key(|entity| entity.index());
        atlas_lights.dedup();
        if self.atlas_light_layout != atlas_lights || self.atlas_size != frame.point_shadow_map_size
        {
            self.cache_entries.clear();
            self.atlas_light_layout = atlas_lights;
            self.atlas_size = frame.point_shadow_map_size;
        }

        if !frame.enabled {
            self.cache_entries.clear();
            self.render_view_entities
                .extend(point_views.iter().map(|(entity, _, _)| *entity));
            let resident = point_views
                .iter()
                .filter(|(_, _, cacheable)| *cacheable)
                .map(|(_, key, _)| *key)
                .collect::<HashSet<_>>()
                .len();
            frame.telemetry.store(resident, 0, resident, 0);
            return;
        }

        let active_keys = point_views
            .iter()
            .filter(|(_, _, cacheable)| *cacheable)
            .map(|(_, key, _)| *key)
            .collect::<HashSet<_>>();
        self.cache_entries
            .retain(|key, _| active_keys.contains(key));

        let mut update_keys = HashSet::new();
        for key in &active_keys {
            let is_new = !self.cache_entries.contains_key(key);
            let remaining = self
                .cache_entries
                .entry(*key)
                .or_insert(frame.warmup_frames.max(1));
            if !is_new && (frame.invalidate_all || dirty_lights.contains(&key.light)) {
                *remaining = (*remaining).max(1);
            }
            if *remaining > 0 {
                update_keys.insert(*key);
                *remaining -= 1;
            }
        }

        for (view_entity, key, cacheable) in point_views {
            if !cacheable || update_keys.contains(&key) {
                self.render_view_entities.insert(view_entity);
                self.point_render_claims
                    .entry(key)
                    .or_insert_with(|| AtomicBool::new(false));
            }
        }

        frame.telemetry.store(
            update_keys.len(),
            active_keys.len().saturating_sub(update_keys.len()),
            active_keys.len(),
            dirty_lights.len() + usize::from(frame.invalidate_all),
        );
    }

    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let Some(shadow_render_phases) = world.get_resource::<ViewBinnedRenderPhases<Shadow>>()
        else {
            return Ok(());
        };

        if let Ok(view_lights) = self.main_view_query.get_manual(world, graph.view_entity()) {
            for view_light_entity in view_lights.lights.iter().copied() {
                let static_view_claimed = self
                    .point_key_by_view
                    .get(&view_light_entity)
                    .and_then(|key| self.point_render_claims.get(key))
                    .is_none_or(|claim| !claim.swap(true, Ordering::AcqRel));
                if self.render_view_entities.contains(&view_light_entity)
                    && static_view_claimed
                    && let Ok((_, view_light, extracted_light_view, _, _)) =
                        self.shadow_view_query.get_manual(world, view_light_entity)
                    && let Some(shadow_phase) =
                        shadow_render_phases.get(&extracted_light_view.retained_view_entity)
                {
                    let depth_stencil_attachment =
                        Some(view_light.depth_attachment.get_attachment(StoreOp::Store));
                    let diagnostics = render_context.diagnostic_recorder();
                    render_context.add_command_buffer_generation_task(move |render_device| {
                        let mut command_encoder =
                            render_device.create_command_encoder(&CommandEncoderDescriptor {
                                label: Some("zevy_cached_shadow_pass_command_encoder"),
                            });
                        let render_pass =
                            command_encoder.begin_render_pass(&RenderPassDescriptor {
                                label: Some(&view_light.pass_name),
                                color_attachments: &[],
                                depth_stencil_attachment,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });
                        let mut render_pass = TrackedRenderPass::new(&render_device, render_pass);
                        let pass_span =
                            diagnostics.pass_span(&mut render_pass, view_light.pass_name.clone());
                        if let Err(error) =
                            shadow_phase.render(&mut render_pass, world, view_light_entity)
                        {
                            error!(
                                "Error encountered while rendering cached shadow phase: {error:?}"
                            );
                        }
                        pass_span.end(&mut render_pass);
                        drop(render_pass);
                        command_encoder.finish()
                    });
                }

                let Some(&dynamic_view_entity) =
                    self.dynamic_view_by_static.get(&view_light_entity)
                else {
                    continue;
                };
                if !self
                    .render_dynamic_view_entities
                    .contains(&dynamic_view_entity)
                {
                    continue;
                }
                let Ok((_, dynamic_view, extracted_dynamic_view)) = self
                    .dynamic_shadow_view_query
                    .get_manual(world, dynamic_view_entity)
                else {
                    continue;
                };
                if !dynamic_view.claim_render() {
                    continue;
                }
                let Some(dynamic_phase) =
                    shadow_render_phases.get(&extracted_dynamic_view.retained_view_entity)
                else {
                    continue;
                };
                let depth_stencil_attachment =
                    Some(dynamic_view.depth_attachment.get_attachment(StoreOp::Store));
                let pass_name = format!(
                    "dynamic point shadow {:?} face {}",
                    dynamic_view.key.light, dynamic_view.key.face
                );
                let diagnostics = render_context.diagnostic_recorder();
                render_context.add_command_buffer_generation_task(move |render_device| {
                    let mut command_encoder =
                        render_device.create_command_encoder(&CommandEncoderDescriptor {
                            label: Some("zevy_dynamic_shadow_overlay_command_encoder"),
                        });
                    let render_pass = command_encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some(&pass_name),
                        color_attachments: &[],
                        depth_stencil_attachment,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    let mut render_pass = TrackedRenderPass::new(&render_device, render_pass);
                    let pass_span = diagnostics.pass_span(&mut render_pass, pass_name.clone());
                    if let Err(error) =
                        dynamic_phase.render(&mut render_pass, world, dynamic_view_entity)
                    {
                        error!(
                            "Error encountered while rendering dynamic shadow overlay: {error:?}"
                        );
                    }
                    pass_span.end(&mut render_pass);
                    drop(render_pass);
                    command_encoder.finish()
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_shadow_projection_state_tracks_only_depth_projection_inputs() {
        let mut light = PointLight::default();
        let transform =
            GlobalTransform::from(Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)));
        let baseline = PointShadowProjectionState::new(&light, &transform);

        light.intensity *= 2.0;
        light.color = Color::srgb(1.0, 0.2, 0.1);
        light.radius += 0.25;
        light.shadow_depth_bias += 0.01;
        light.shadow_normal_bias += 0.01;
        assert_eq!(
            baseline,
            PointShadowProjectionState::new(&light, &transform),
            "shading-only changes must keep cached depth valid"
        );

        light.range += 1.0;
        assert_ne!(
            baseline,
            PointShadowProjectionState::new(&light, &transform)
        );

        light.range -= 1.0;
        light.shadow_map_near_z += 0.05;
        assert_ne!(
            baseline,
            PointShadowProjectionState::new(&light, &transform)
        );

        light.shadow_map_near_z -= 0.05;
        let moved = GlobalTransform::from(Transform::from_translation(Vec3::new(1.1, 2.0, 3.0)));
        assert_ne!(baseline, PointShadowProjectionState::new(&light, &moved));
    }
}
