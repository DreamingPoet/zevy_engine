use std::collections::HashSet;

use bevy::{
    pbr::{PointLightShadowMapJitter, PointLightShadowMapTransition},
    prelude::*,
    transform::TransformSystem,
};

use crate::{
    config::RenderQualityConfig,
    scene::{ImportedZevyEntity, ImportedZevyLight},
    shadow_cache::{CachedPointLightShadow, ShadowCacheSet, ZevyShadowCacheFrame},
    shadow_overlay::DynamicShadowCaster,
};

/// The concrete shadow path selected for a PointLight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightShadowMotionClass {
    Static,
    BoundedMicroMotion,
    SlowMoving,
    FullyDynamic,
}

impl LightShadowMotionClass {
    fn dynamic_rank(self) -> u8 {
        match self {
            Self::Static => 0,
            Self::BoundedMicroMotion => 1,
            Self::SlowMoving => 2,
            Self::FullyDynamic => 3,
        }
    }
}

/// Automatic selection, or a manually fixed shadow motion class.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LightShadowMotionMode {
    #[default]
    Automatic,
    Static,
    BoundedMicroMotion,
    SlowMoving,
    FullyDynamic,
}

impl LightShadowMotionMode {
    fn fixed_class(self) -> Option<LightShadowMotionClass> {
        match self {
            Self::Automatic => None,
            Self::Static => Some(LightShadowMotionClass::Static),
            Self::BoundedMicroMotion => Some(LightShadowMotionClass::BoundedMicroMotion),
            Self::SlowMoving => Some(LightShadowMotionClass::SlowMoving),
            Self::FullyDynamic => Some(LightShadowMotionClass::FullyDynamic),
        }
    }
}

/// Per-light thresholds for automatic motion classification.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightShadowAutomaticThresholds {
    /// World-space speed below which the real projection is stationary.
    pub stationary_speed_mps: f32,
    /// Range change rate below which the projection is stationary.
    pub stationary_range_rate_mps: f32,
    /// Largest world-space speed that remains in the cache-on-dirty path.
    pub slow_speed_mps: f32,
    /// Largest range change rate that remains in the cache-on-dirty path.
    pub slow_range_rate_mps: f32,
    /// Maximum virtual-origin magnitude accepted as bounded micro motion.
    pub max_micro_motion_m: f32,
    /// Time a lower-cost candidate must remain valid before downgrading.
    pub settle_seconds: f32,
    /// Exponential velocity smoothing response in Hz.
    pub smoothing_hz: f32,
}

impl Default for LightShadowAutomaticThresholds {
    fn default() -> Self {
        Self {
            stationary_speed_mps: 0.002,
            stationary_range_rate_mps: 0.002,
            slow_speed_mps: 0.35,
            slow_range_rate_mps: 0.50,
            // The current packed virtual-origin representation supports about
            // +/-31.75 mm on each axis. Stay below that edge by default.
            max_micro_motion_m: 0.030,
            settle_seconds: 0.75,
            smoothing_hz: 12.0,
        }
    }
}

/// Selects the shadow update path for one local light.
///
/// Manual modes are fixed. Automatic mode upgrades immediately when motion
/// becomes more demanding and downgrades only after the configured settle
/// interval. Bounded micro motion requires a producer to write
/// `PointLightShadowMapJitter`; real Transform motion is never silently
/// reinterpreted as a virtual offset. Persistent cached classes are currently
/// implemented for PointLight. SpotLight is accepted as a real one-frustum
/// FullyDynamic path; lower-cost SpotLight classes are promoted until a spot
/// shadow cache exists.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct LightShadowMotionPolicy {
    pub mode: LightShadowMotionMode,
    pub automatic: LightShadowAutomaticThresholds,
}

impl Default for LightShadowMotionPolicy {
    fn default() -> Self {
        Self::automatic()
    }
}

impl LightShadowMotionPolicy {
    pub const fn automatic() -> Self {
        Self {
            mode: LightShadowMotionMode::Automatic,
            automatic: LightShadowAutomaticThresholds {
                stationary_speed_mps: 0.002,
                stationary_range_rate_mps: 0.002,
                slow_speed_mps: 0.35,
                slow_range_rate_mps: 0.50,
                max_micro_motion_m: 0.030,
                settle_seconds: 0.75,
                smoothing_hz: 12.0,
            },
        }
    }

    pub const fn fixed(class: LightShadowMotionClass) -> Self {
        let mode = match class {
            LightShadowMotionClass::Static => LightShadowMotionMode::Static,
            LightShadowMotionClass::BoundedMicroMotion => LightShadowMotionMode::BoundedMicroMotion,
            LightShadowMotionClass::SlowMoving => LightShadowMotionMode::SlowMoving,
            LightShadowMotionClass::FullyDynamic => LightShadowMotionMode::FullyDynamic,
        };
        Self {
            mode,
            automatic: Self::automatic().automatic,
        }
    }
}

/// Runtime result and motion telemetry shared by both eyes.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ResolvedLightShadowMotion {
    pub class: LightShadowMotionClass,
    pub linear_speed_mps: f32,
    pub range_rate_mps: f32,
    pub virtual_offset_m: f32,
    pub lower_cost_candidate_seconds: f32,
}

#[derive(Component, Clone, Copy, Debug)]
struct LightShadowMotionState {
    mode: LightShadowMotionMode,
    last_translation: Vec3,
    last_range: f32,
    smoothed_linear_speed_mps: f32,
    smoothed_range_rate_mps: f32,
    class: LightShadowMotionClass,
    lower_cost_candidate: LightShadowMotionClass,
    lower_cost_candidate_seconds: f32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ShadowCasterMotionClass {
    Static,
    #[default]
    DynamicOverlay,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ShadowCasterMotionMode {
    #[default]
    Automatic,
    Static,
    DynamicOverlay,
}

impl ShadowCasterMotionMode {
    fn fixed_class(self) -> Option<ShadowCasterMotionClass> {
        match self {
            Self::Automatic => None,
            Self::Static => Some(ShadowCasterMotionClass::Static),
            Self::DynamicOverlay => Some(ShadowCasterMotionClass::DynamicOverlay),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowCasterAutomaticThresholds {
    pub translation_epsilon_m: f32,
    pub rotation_epsilon_radians: f32,
    pub scale_epsilon: f32,
    pub settle_seconds: f32,
}

impl Default for ShadowCasterAutomaticThresholds {
    fn default() -> Self {
        Self {
            translation_epsilon_m: 0.000_5,
            rotation_epsilon_radians: 0.001,
            scale_epsilon: 0.000_5,
            settle_seconds: 0.50,
        }
    }
}

/// Selects whether a mesh or Actor root belongs to the static shadow layer or
/// the dynamic-caster overlay.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ShadowCasterMotionPolicy {
    pub mode: ShadowCasterMotionMode,
    pub automatic: ShadowCasterAutomaticThresholds,
}

impl Default for ShadowCasterMotionPolicy {
    fn default() -> Self {
        Self::automatic()
    }
}

impl ShadowCasterMotionPolicy {
    pub const fn automatic() -> Self {
        Self {
            mode: ShadowCasterMotionMode::Automatic,
            automatic: ShadowCasterAutomaticThresholds {
                translation_epsilon_m: 0.000_5,
                rotation_epsilon_radians: 0.001,
                scale_epsilon: 0.000_5,
                settle_seconds: 0.50,
            },
        }
    }

    pub const fn fixed(class: ShadowCasterMotionClass) -> Self {
        Self {
            mode: match class {
                ShadowCasterMotionClass::Static => ShadowCasterMotionMode::Static,
                ShadowCasterMotionClass::DynamicOverlay => ShadowCasterMotionMode::DynamicOverlay,
            },
            automatic: Self::automatic().automatic,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ResolvedShadowCasterMotion {
    pub class: ShadowCasterMotionClass,
    pub translation_delta_m: f32,
    pub rotation_delta_radians: f32,
    pub scale_delta: f32,
    pub stationary_seconds: f32,
}

#[derive(Component, Clone, Copy, Debug)]
struct ShadowCasterMotionState {
    mode: ShadowCasterMotionMode,
    last_translation: Vec3,
    last_rotation: Quat,
    last_scale: Vec3,
    class: ShadowCasterMotionClass,
    stationary_seconds: f32,
}

#[derive(Component)]
struct PolicyManagedPointShadowCache;

#[derive(Component)]
struct PolicyManagedPointShadowJitter;

#[derive(Component)]
struct PolicyManagedPointShadowTransition;

#[derive(Component, Clone, Copy, Debug)]
struct SlowMovingPointShadowState {
    current_position: Vec3,
    previous_position: Vec3,
    current_range: f32,
    current_near_z: f32,
    last_snapshot_seconds: f32,
    transition_start_seconds: f32,
    transition_slot: Option<u32>,
}

#[derive(Resource, Debug, Default)]
struct SlowMovingShadowSlotPool {
    owners: Vec<Option<Entity>>,
}

impl SlowMovingShadowSlotPool {
    fn configure(&mut self, capacity: usize, live_entities: &HashSet<Entity>) {
        self.owners.resize(capacity, None);
        for owner in &mut self.owners {
            if owner.is_some_and(|entity| !live_entities.contains(&entity)) {
                *owner = None;
            }
        }
    }

    fn claim(&mut self, entity: Entity) -> Option<u32> {
        if let Some((slot, _)) = self
            .owners
            .iter()
            .enumerate()
            .find(|(_, owner)| **owner == Some(entity))
        {
            return Some(slot as u32);
        }
        let (slot, owner) = self
            .owners
            .iter_mut()
            .enumerate()
            .find(|(_, owner)| owner.is_none())?;
        *owner = Some(entity);
        Some(slot as u32)
    }

    fn release(&mut self, entity: Entity) {
        for owner in &mut self.owners {
            if *owner == Some(entity) {
                *owner = None;
            }
        }
    }

    fn capacity(&self) -> usize {
        self.owners.len()
    }
}

#[derive(Component)]
struct PolicyManagedDynamicShadowCaster;

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct ShadowMotionPolicyTelemetry {
    pub light_static: usize,
    pub light_micro_motion: usize,
    pub light_slow_moving: usize,
    pub light_fully_dynamic: usize,
    pub caster_static: usize,
    pub caster_dynamic_overlay: usize,
    pub transitions_this_frame: usize,
    pub slow_shadow_transitions_active: usize,
    pub slow_shadow_transition_starts: usize,
    pub slow_shadow_transition_waiting: usize,
    pub slow_shadow_transition_slot_capacity: usize,
    pub slow_shadow_max_stale_distance_m: f32,
}

#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ShadowMotionPolicySet;

pub(crate) struct ShadowMotionPolicyPlugin;

impl Plugin for ShadowMotionPolicyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShadowMotionPolicyTelemetry>()
            .init_resource::<RenderQualityConfig>()
            .init_resource::<ZevyShadowCacheFrame>()
            .init_resource::<SlowMovingShadowSlotPool>()
            .configure_sets(
                PostUpdate,
                ShadowMotionPolicySet
                    .after(TransformSystem::TransformPropagate)
                    .before(ShadowCacheSet::Finalize),
            )
            .add_systems(
                PostUpdate,
                (
                    begin_policy_telemetry,
                    resolve_light_shadow_motion,
                    resolve_spot_light_shadow_motion,
                    resolve_shadow_caster_motion,
                    ApplyDeferred,
                    update_slow_moving_point_shadow_snapshots,
                    cleanup_removed_motion_policies,
                    ApplyDeferred,
                )
                    .chain()
                    .in_set(ShadowMotionPolicySet),
            );
    }
}

fn begin_policy_telemetry(mut telemetry: ResMut<ShadowMotionPolicyTelemetry>) {
    *telemetry = ShadowMotionPolicyTelemetry::default();
}

#[allow(clippy::type_complexity)]
fn resolve_light_shadow_motion(
    time: Res<Time>,
    mut commands: Commands,
    mut telemetry: ResMut<ShadowMotionPolicyTelemetry>,
    lights: Query<(
        Entity,
        Option<&Name>,
        &PointLight,
        &GlobalTransform,
        &LightShadowMotionPolicy,
        Option<&PointLightShadowMapJitter>,
        Option<&LightShadowMotionState>,
        Option<&ResolvedLightShadowMotion>,
        Has<ImportedZevyLight>,
        Has<CachedPointLightShadow>,
        Has<PolicyManagedPointShadowCache>,
        Has<PolicyManagedPointShadowJitter>,
    )>,
) {
    let dt = policy_delta_seconds(&time);
    for (
        entity,
        name,
        light,
        global_transform,
        policy,
        jitter,
        previous_state,
        previous_resolved,
        imported,
        has_cache,
        managed_cache,
        managed_jitter,
    ) in &lights
    {
        let translation = global_transform.translation();
        let virtual_offset_m = jitter.map_or(0.0, |jitter| jitter.local_offset.length());
        let (state, resolved) = next_light_motion_state(
            previous_state.copied(),
            *policy,
            translation,
            light.range,
            virtual_offset_m,
            imported,
            dt,
        );

        if previous_resolved.is_some_and(|previous| previous.class != resolved.class) {
            telemetry.transitions_this_frame += 1;
            debug!(
                "Light shadow policy '{}' transitioned {:?} -> {:?} ({:.3} m/s, range {:.3} m/s, virtual {:.4} m)",
                name.map(Name::as_str).unwrap_or("unnamed"),
                previous_resolved.unwrap().class,
                resolved.class,
                resolved.linear_speed_mps,
                resolved.range_rate_mps,
                resolved.virtual_offset_m,
            );
        }
        match resolved.class {
            LightShadowMotionClass::Static => telemetry.light_static += 1,
            LightShadowMotionClass::BoundedMicroMotion => telemetry.light_micro_motion += 1,
            LightShadowMotionClass::SlowMoving => telemetry.light_slow_moving += 1,
            LightShadowMotionClass::FullyDynamic => telemetry.light_fully_dynamic += 1,
        }

        let wants_cache = resolved.class != LightShadowMotionClass::FullyDynamic;
        let wants_jitter = resolved.class == LightShadowMotionClass::BoundedMicroMotion;
        let mut entity_commands = commands.entity(entity);
        entity_commands.insert((state, resolved));

        if wants_cache {
            if !has_cache {
                entity_commands.insert(CachedPointLightShadow::default());
            }
            if !managed_cache {
                entity_commands.insert(PolicyManagedPointShadowCache);
            }
        } else {
            entity_commands.remove::<(CachedPointLightShadow, PolicyManagedPointShadowCache)>();
        }

        if wants_jitter {
            if jitter.is_none() {
                entity_commands.insert(PointLightShadowMapJitter::default());
            }
            if !managed_jitter {
                entity_commands.insert(PolicyManagedPointShadowJitter);
            }
        } else {
            entity_commands.remove::<(PointLightShadowMapJitter, PolicyManagedPointShadowJitter)>();
        }
    }
}

#[allow(clippy::type_complexity)]
fn resolve_spot_light_shadow_motion(
    time: Res<Time>,
    mut commands: Commands,
    mut telemetry: ResMut<ShadowMotionPolicyTelemetry>,
    lights: Query<
        (
            Entity,
            Option<&Name>,
            &SpotLight,
            &GlobalTransform,
            &LightShadowMotionPolicy,
            Option<&LightShadowMotionState>,
            Option<&ResolvedLightShadowMotion>,
            Has<ImportedZevyLight>,
        ),
        Without<PointLight>,
    >,
) {
    let dt = policy_delta_seconds(&time);
    for (
        entity,
        name,
        light,
        global_transform,
        policy,
        previous_state,
        previous_resolved,
        imported,
    ) in &lights
    {
        let (mut state, mut resolved) = next_light_motion_state(
            previous_state.copied(),
            *policy,
            global_transform.translation(),
            light.range,
            0.0,
            imported,
            dt,
        );

        // Bevy renders a shadowed SpotLight through one live frustum. Zevy's
        // persistent cache currently stores only PointLight cubemap faces, so
        // never attach point-cache/jitter components or report a cache class
        // that the renderer cannot honor.
        if resolved.class != LightShadowMotionClass::FullyDynamic && previous_resolved.is_none() {
            warn!(
                "SpotLight shadow policy '{}' requested {:?}; Zevy P1 has no cached spot-shadow path, so it is promoted to FullyDynamic",
                name.map(Name::as_str).unwrap_or("unnamed"),
                resolved.class,
            );
        }
        state.class = LightShadowMotionClass::FullyDynamic;
        state.lower_cost_candidate = LightShadowMotionClass::FullyDynamic;
        state.lower_cost_candidate_seconds = 0.0;
        resolved.class = LightShadowMotionClass::FullyDynamic;
        resolved.lower_cost_candidate_seconds = 0.0;

        if previous_resolved.is_some_and(|previous| previous.class != resolved.class) {
            telemetry.transitions_this_frame += 1;
            debug!(
                "SpotLight shadow policy '{}' transitioned {:?} -> FullyDynamic",
                name.map(Name::as_str).unwrap_or("unnamed"),
                previous_resolved.unwrap().class,
            );
        }
        telemetry.light_fully_dynamic += 1;
        commands.entity(entity).insert((state, resolved)).remove::<(
            CachedPointLightShadow,
            PolicyManagedPointShadowCache,
            PointLightShadowMapJitter,
            PolicyManagedPointShadowJitter,
            PointLightShadowMapTransition,
            PolicyManagedPointShadowTransition,
            SlowMovingPointShadowState,
        )>();
    }
}

#[derive(Clone, Copy, Debug)]
struct SlowMovingShadowWork {
    entity: Entity,
    position: Vec3,
    range: f32,
    near_z: f32,
    state: SlowMovingPointShadowState,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_slow_moving_point_shadow_snapshots(
    time: Res<Time>,
    quality: Res<RenderQualityConfig>,
    mut commands: Commands,
    mut shadow_cache_frame: ResMut<ZevyShadowCacheFrame>,
    mut slot_pool: ResMut<SlowMovingShadowSlotPool>,
    mut telemetry: ResMut<ShadowMotionPolicyTelemetry>,
    lights: Query<(
        Entity,
        &PointLight,
        &GlobalTransform,
        &ResolvedLightShadowMotion,
        Option<&SlowMovingPointShadowState>,
        Has<PolicyManagedPointShadowTransition>,
    )>,
) {
    let now = time.elapsed_secs();
    let crossfade_enabled = quality.slow_moving_shadow_crossfade_enabled();
    let requested_slots = if crossfade_enabled {
        quality.resolved_slow_moving_shadow_transition_slots()
    } else {
        0
    };
    let configured_slots = requested_slots.min(
        shadow_cache_frame
            .effective_transition_slots()
            .unwrap_or(requested_slots),
    );
    let live_entities = lights.iter().map(|(entity, ..)| entity).collect();
    slot_pool.configure(configured_slots, &live_entities);
    telemetry.slow_shadow_transition_slot_capacity = slot_pool.capacity();

    let transition_seconds = quality.resolved_slow_moving_shadow_crossfade_seconds();
    let snapshot_hz = quality.resolved_slow_moving_shadow_snapshot_hz();
    let snapshot_interval = if snapshot_hz > 0.0 {
        Some(1.0 / snapshot_hz)
    } else {
        None
    };
    let distance_threshold = quality.resolved_slow_moving_shadow_snapshot_distance_m();
    let mut work = Vec::new();

    for (entity, light, transform, resolved, previous_state, managed_transition) in &lights {
        if resolved.class != LightShadowMotionClass::SlowMoving || !light.shadows_enabled {
            if previous_state.is_some() || managed_transition {
                slot_pool.release(entity);
                commands.entity(entity).remove::<(
                    SlowMovingPointShadowState,
                    PointLightShadowMapTransition,
                    PolicyManagedPointShadowTransition,
                )>();
                // A cached class entered or left the snapshot-managed path.
                // Re-align its resident cubemap with the real projection once.
                shadow_cache_frame.invalidate_point_light(entity);
            }
            continue;
        }

        let position = transform.translation();
        let mut state = previous_state
            .copied()
            .unwrap_or(SlowMovingPointShadowState {
                current_position: position,
                previous_position: position,
                current_range: light.range,
                current_near_z: light.shadow_map_near_z,
                last_snapshot_seconds: now,
                transition_start_seconds: now,
                transition_slot: None,
            });

        if previous_state.is_none() {
            shadow_cache_frame.invalidate_point_light(entity);
        }
        if state
            .transition_slot
            .is_some_and(|slot| slot as usize >= slot_pool.capacity())
        {
            slot_pool.release(entity);
            state.transition_slot = None;
            state.previous_position = state.current_position;
        }

        if state.transition_slot.is_some()
            && (transition_seconds <= 0.0
                || now - state.transition_start_seconds >= transition_seconds)
        {
            slot_pool.release(entity);
            state.transition_slot = None;
            state.previous_position = state.current_position;
        }

        // A near-plane change alters depth encoding, so old and new maps are
        // not numerically blend-compatible. Perform one real redraw and settle
        // immediately; transform/range motion continues through keyframes.
        if light.shadow_map_near_z.to_bits() != state.current_near_z.to_bits() {
            slot_pool.release(entity);
            state.current_position = position;
            state.previous_position = position;
            state.current_range = light.range;
            state.current_near_z = light.shadow_map_near_z;
            state.last_snapshot_seconds = now;
            state.transition_start_seconds = now;
            state.transition_slot = None;
            shadow_cache_frame.invalidate_point_light(entity);
        }

        work.push(SlowMovingShadowWork {
            entity,
            position,
            range: light.range,
            near_z: light.shadow_map_near_z,
            state,
        });
    }

    let mut candidates = work
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if item.state.transition_slot.is_some() {
                return None;
            }
            let position_delta = item.position.distance(item.state.current_position);
            let range_delta = (item.range - item.state.current_range).abs();
            let elapsed = (now - item.state.last_snapshot_seconds).max(0.0);
            shadow_snapshot_is_due(
                position_delta,
                range_delta,
                elapsed,
                snapshot_interval,
                distance_threshold,
            )
            .then_some((index, position_delta.max(range_delta)))
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by(|(left_index, left_stale), (right_index, right_stale)| {
        right_stale.total_cmp(left_stale).then_with(|| {
            work[*left_index]
                .entity
                .index()
                .cmp(&work[*right_index].entity.index())
        })
    });

    for (index, _) in candidates {
        let item = &mut work[index];
        if crossfade_enabled {
            let Some(slot) = slot_pool.claim(item.entity) else {
                telemetry.slow_shadow_transition_waiting += 1;
                continue;
            };
            item.state.previous_position = item.state.current_position;
            item.state.current_position = item.position;
            item.state.current_range = item.range;
            item.state.current_near_z = item.near_z;
            item.state.last_snapshot_seconds = now;
            item.state.transition_start_seconds = now;
            item.state.transition_slot = Some(slot);
            shadow_cache_frame.request_transition_copy(item.entity, slot);
            telemetry.slow_shadow_transition_starts += 1;
        } else {
            item.state.current_position = item.position;
            item.state.previous_position = item.position;
            item.state.current_range = item.range;
            item.state.current_near_z = item.near_z;
            item.state.last_snapshot_seconds = now;
            item.state.transition_start_seconds = now;
            item.state.transition_slot = None;
        }
        shadow_cache_frame.invalidate_point_light(item.entity);
    }

    for item in work {
        let blend = item.state.transition_slot.map_or(1.0, |_| {
            if transition_seconds > 0.0 {
                ((now - item.state.transition_start_seconds) / transition_seconds).clamp(0.0, 1.0)
            } else {
                1.0
            }
        });
        telemetry.slow_shadow_transitions_active +=
            usize::from(item.state.transition_slot.is_some());
        telemetry.slow_shadow_max_stale_distance_m = telemetry
            .slow_shadow_max_stale_distance_m
            .max(item.position.distance(item.state.current_position));
        commands.entity(item.entity).insert((
            item.state,
            PointLightShadowMapTransition {
                current_position: item.state.current_position,
                previous_position: item.state.previous_position,
                blend,
                transition_slot: item.state.transition_slot,
            },
            PolicyManagedPointShadowTransition,
        ));
    }
}

fn shadow_snapshot_is_due(
    position_delta_m: f32,
    range_delta_m: f32,
    elapsed_seconds: f32,
    snapshot_interval_seconds: Option<f32>,
    distance_threshold_m: f32,
) -> bool {
    let moved = position_delta_m > 0.000_01 || range_delta_m > 0.000_01;
    moved
        && (position_delta_m >= distance_threshold_m
            || range_delta_m >= distance_threshold_m
            || snapshot_interval_seconds
                .is_some_and(|interval| elapsed_seconds >= interval.max(0.0)))
}

fn next_light_motion_state(
    previous: Option<LightShadowMotionState>,
    policy: LightShadowMotionPolicy,
    translation: Vec3,
    range: f32,
    virtual_offset_m: f32,
    imported: bool,
    dt: f32,
) -> (LightShadowMotionState, ResolvedLightShadowMotion) {
    let initial_class = policy.mode.fixed_class().unwrap_or(if imported {
        LightShadowMotionClass::Static
    } else {
        LightShadowMotionClass::FullyDynamic
    });
    let reset_for_policy_change = previous.is_none_or(|state| state.mode != policy.mode);
    let mut state = if reset_for_policy_change {
        LightShadowMotionState {
            mode: policy.mode,
            last_translation: translation,
            last_range: range,
            smoothed_linear_speed_mps: 0.0,
            smoothed_range_rate_mps: 0.0,
            class: initial_class,
            lower_cost_candidate: initial_class,
            lower_cost_candidate_seconds: 0.0,
        }
    } else {
        previous.unwrap()
    };

    let instantaneous_speed = translation.distance(state.last_translation) / dt;
    let instantaneous_range_rate = (range - state.last_range).abs() / dt;
    let smoothing = 1.0 - (-policy.automatic.smoothing_hz.max(0.0) * dt).exp();
    state.smoothed_linear_speed_mps = state
        .smoothed_linear_speed_mps
        .lerp(instantaneous_speed, smoothing);
    state.smoothed_range_rate_mps = state
        .smoothed_range_rate_mps
        .lerp(instantaneous_range_rate, smoothing);
    state.last_translation = translation;
    state.last_range = range;

    let selected_class = if reset_for_policy_change {
        initial_class
    } else if let Some(fixed) = policy.mode.fixed_class() {
        state.lower_cost_candidate = fixed;
        state.lower_cost_candidate_seconds = 0.0;
        fixed
    } else {
        let thresholds = policy.automatic;
        let projection_stationary = state.smoothed_linear_speed_mps
            <= thresholds.stationary_speed_mps.max(0.0)
            && state.smoothed_range_rate_mps <= thresholds.stationary_range_rate_mps.max(0.0);
        let raw_class = if projection_stationary
            && virtual_offset_m > 0.000_01
            && virtual_offset_m <= thresholds.max_micro_motion_m.max(0.0)
        {
            LightShadowMotionClass::BoundedMicroMotion
        } else if projection_stationary && virtual_offset_m <= 0.000_01 {
            LightShadowMotionClass::Static
        } else if state.smoothed_linear_speed_mps <= thresholds.slow_speed_mps.max(0.0)
            && state.smoothed_range_rate_mps <= thresholds.slow_range_rate_mps.max(0.0)
            && virtual_offset_m <= 0.000_01
        {
            LightShadowMotionClass::SlowMoving
        } else {
            LightShadowMotionClass::FullyDynamic
        };

        if raw_class.dynamic_rank() > state.class.dynamic_rank() {
            state.lower_cost_candidate = raw_class;
            state.lower_cost_candidate_seconds = 0.0;
            raw_class
        } else if raw_class == state.class {
            state.lower_cost_candidate = raw_class;
            state.lower_cost_candidate_seconds = 0.0;
            state.class
        } else {
            if state.lower_cost_candidate == raw_class {
                state.lower_cost_candidate_seconds += dt;
            } else {
                state.lower_cost_candidate = raw_class;
                state.lower_cost_candidate_seconds = dt;
            }
            if state.lower_cost_candidate_seconds >= thresholds.settle_seconds.max(0.0) {
                state.lower_cost_candidate_seconds = 0.0;
                raw_class
            } else {
                state.class
            }
        }
    };
    state.class = selected_class;

    (
        state,
        ResolvedLightShadowMotion {
            class: selected_class,
            linear_speed_mps: state.smoothed_linear_speed_mps,
            range_rate_mps: state.smoothed_range_rate_mps,
            virtual_offset_m,
            lower_cost_candidate_seconds: state.lower_cost_candidate_seconds,
        },
    )
}

#[allow(clippy::type_complexity)]
fn resolve_shadow_caster_motion(
    time: Res<Time>,
    mut commands: Commands,
    mut telemetry: ResMut<ShadowMotionPolicyTelemetry>,
    casters: Query<(
        Entity,
        Option<&Name>,
        &GlobalTransform,
        &ShadowCasterMotionPolicy,
        Option<&ShadowCasterMotionState>,
        Option<&ResolvedShadowCasterMotion>,
        Has<ImportedZevyEntity>,
        Has<DynamicShadowCaster>,
        Has<PolicyManagedDynamicShadowCaster>,
    )>,
) {
    let dt = policy_delta_seconds(&time);
    for (
        entity,
        name,
        global_transform,
        policy,
        previous_state,
        previous_resolved,
        imported,
        has_dynamic_marker,
        managed_marker,
    ) in &casters
    {
        let (scale, rotation, translation) = global_transform.to_scale_rotation_translation();
        let (state, resolved) = next_caster_motion_state(
            previous_state.copied(),
            *policy,
            translation,
            rotation,
            scale,
            imported,
            dt,
        );
        if previous_resolved.is_some_and(|previous| previous.class != resolved.class) {
            telemetry.transitions_this_frame += 1;
            debug!(
                "Shadow caster policy '{}' transitioned {:?} -> {:?} (translation {:.4} m, rotation {:.4} rad, scale {:.4})",
                name.map(Name::as_str).unwrap_or("unnamed"),
                previous_resolved.unwrap().class,
                resolved.class,
                resolved.translation_delta_m,
                resolved.rotation_delta_radians,
                resolved.scale_delta,
            );
        }
        match resolved.class {
            ShadowCasterMotionClass::Static => telemetry.caster_static += 1,
            ShadowCasterMotionClass::DynamicOverlay => telemetry.caster_dynamic_overlay += 1,
        }

        let mut entity_commands = commands.entity(entity);
        entity_commands.insert((state, resolved));
        match resolved.class {
            ShadowCasterMotionClass::DynamicOverlay => {
                if !has_dynamic_marker {
                    entity_commands.insert(DynamicShadowCaster);
                }
                if !managed_marker {
                    entity_commands.insert(PolicyManagedDynamicShadowCaster);
                }
            }
            ShadowCasterMotionClass::Static => {
                entity_commands.remove::<(DynamicShadowCaster, PolicyManagedDynamicShadowCaster)>();
            }
        }
    }
}

fn next_caster_motion_state(
    previous: Option<ShadowCasterMotionState>,
    policy: ShadowCasterMotionPolicy,
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
    imported: bool,
    dt: f32,
) -> (ShadowCasterMotionState, ResolvedShadowCasterMotion) {
    let initial_class = policy.mode.fixed_class().unwrap_or(if imported {
        ShadowCasterMotionClass::Static
    } else {
        ShadowCasterMotionClass::DynamicOverlay
    });
    let reset_for_policy_change = previous.is_none_or(|state| state.mode != policy.mode);
    let mut state = if reset_for_policy_change {
        ShadowCasterMotionState {
            mode: policy.mode,
            last_translation: translation,
            last_rotation: rotation,
            last_scale: scale,
            class: initial_class,
            stationary_seconds: 0.0,
        }
    } else {
        previous.unwrap()
    };

    let translation_delta_m = translation.distance(state.last_translation);
    let rotation_delta_radians = quaternion_angle_between(rotation, state.last_rotation);
    let scale_delta = (scale - state.last_scale).abs().max_element();
    state.last_translation = translation;
    state.last_rotation = rotation;
    state.last_scale = scale;

    let selected_class = if reset_for_policy_change {
        initial_class
    } else if let Some(fixed) = policy.mode.fixed_class() {
        state.stationary_seconds = 0.0;
        fixed
    } else {
        let thresholds = policy.automatic;
        let moving = translation_delta_m > thresholds.translation_epsilon_m.max(0.0)
            || rotation_delta_radians > thresholds.rotation_epsilon_radians.max(0.0)
            || scale_delta > thresholds.scale_epsilon.max(0.0);
        if moving {
            state.stationary_seconds = 0.0;
            ShadowCasterMotionClass::DynamicOverlay
        } else if state.class == ShadowCasterMotionClass::DynamicOverlay {
            state.stationary_seconds += dt;
            if state.stationary_seconds >= thresholds.settle_seconds.max(0.0) {
                state.stationary_seconds = 0.0;
                ShadowCasterMotionClass::Static
            } else {
                ShadowCasterMotionClass::DynamicOverlay
            }
        } else {
            state.stationary_seconds = 0.0;
            ShadowCasterMotionClass::Static
        }
    };
    state.class = selected_class;

    (
        state,
        ResolvedShadowCasterMotion {
            class: selected_class,
            translation_delta_m,
            rotation_delta_radians,
            scale_delta,
            stationary_seconds: state.stationary_seconds,
        },
    )
}

fn quaternion_angle_between(left: Quat, right: Quat) -> f32 {
    2.0 * left.dot(right).abs().clamp(0.0, 1.0).acos()
}

fn policy_delta_seconds(time: &Time) -> f32 {
    let delta = time.delta_secs();
    if delta.is_finite() && delta > 0.0 {
        delta.clamp(1.0 / 1_000.0, 0.25)
    } else {
        1.0 / 60.0
    }
}

fn cleanup_removed_motion_policies(
    mut commands: Commands,
    mut slot_pool: ResMut<SlowMovingShadowSlotPool>,
    orphaned_light_cache: Query<
        Entity,
        (
            With<PolicyManagedPointShadowCache>,
            Without<LightShadowMotionPolicy>,
        ),
    >,
    orphaned_light_jitter: Query<
        Entity,
        (
            With<PolicyManagedPointShadowJitter>,
            Without<LightShadowMotionPolicy>,
        ),
    >,
    orphaned_light_transition: Query<
        Entity,
        (
            With<PolicyManagedPointShadowTransition>,
            Without<LightShadowMotionPolicy>,
        ),
    >,
    orphaned_light_state: Query<
        Entity,
        (
            Or<(
                With<LightShadowMotionState>,
                With<ResolvedLightShadowMotion>,
                With<SlowMovingPointShadowState>,
            )>,
            Without<LightShadowMotionPolicy>,
        ),
    >,
    orphaned_caster_marker: Query<
        Entity,
        (
            With<PolicyManagedDynamicShadowCaster>,
            Without<ShadowCasterMotionPolicy>,
        ),
    >,
    orphaned_caster_state: Query<
        Entity,
        (
            Or<(
                With<ShadowCasterMotionState>,
                With<ResolvedShadowCasterMotion>,
            )>,
            Without<ShadowCasterMotionPolicy>,
        ),
    >,
) {
    for entity in &orphaned_light_cache {
        commands
            .entity(entity)
            .remove::<(CachedPointLightShadow, PolicyManagedPointShadowCache)>();
    }
    for entity in &orphaned_light_jitter {
        commands
            .entity(entity)
            .remove::<(PointLightShadowMapJitter, PolicyManagedPointShadowJitter)>();
    }
    for entity in &orphaned_light_transition {
        slot_pool.release(entity);
        commands.entity(entity).remove::<(
            PointLightShadowMapTransition,
            PolicyManagedPointShadowTransition,
            SlowMovingPointShadowState,
        )>();
    }
    for entity in &orphaned_light_state {
        commands.entity(entity).remove::<(
            LightShadowMotionState,
            ResolvedLightShadowMotion,
            SlowMovingPointShadowState,
        )>();
    }
    for entity in &orphaned_caster_marker {
        commands
            .entity(entity)
            .remove::<(DynamicShadowCaster, PolicyManagedDynamicShadowCaster)>();
    }
    for entity in &orphaned_caster_state {
        commands
            .entity(entity)
            .remove::<(ShadowCasterMotionState, ResolvedShadowCasterMotion)>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_light_classes_route_to_the_requested_path() {
        let mut app = policy_test_app();
        let entity = app
            .world_mut()
            .spawn((
                PointLight::default(),
                Transform::default(),
                GlobalTransform::default(),
                LightShadowMotionPolicy::fixed(LightShadowMotionClass::Static),
            ))
            .id();

        app.update();
        assert_light_route(&app, entity, LightShadowMotionClass::Static, true, false);

        app.world_mut()
            .entity_mut(entity)
            .insert(LightShadowMotionPolicy::fixed(
                LightShadowMotionClass::BoundedMicroMotion,
            ));
        app.update();
        assert_light_route(
            &app,
            entity,
            LightShadowMotionClass::BoundedMicroMotion,
            true,
            true,
        );

        app.world_mut()
            .entity_mut(entity)
            .insert(LightShadowMotionPolicy::fixed(
                LightShadowMotionClass::SlowMoving,
            ));
        app.update();
        assert_light_route(
            &app,
            entity,
            LightShadowMotionClass::SlowMoving,
            true,
            false,
        );

        app.world_mut()
            .entity_mut(entity)
            .insert(LightShadowMotionPolicy::fixed(
                LightShadowMotionClass::FullyDynamic,
            ));
        app.update();
        assert_light_route(
            &app,
            entity,
            LightShadowMotionClass::FullyDynamic,
            false,
            false,
        );
    }

    #[test]
    fn automatic_light_upgrades_immediately_and_downgrades_after_hysteresis() {
        let mut app = policy_test_app();
        let entity = app
            .world_mut()
            .spawn((
                PointLight::default(),
                Transform::default(),
                GlobalTransform::default(),
                LightShadowMotionPolicy {
                    automatic: LightShadowAutomaticThresholds {
                        settle_seconds: 0.10,
                        smoothing_hz: 100.0,
                        ..default()
                    },
                    ..default()
                },
            ))
            .id();

        advance_and_update(&mut app, 0.05);
        assert_eq!(
            app.world()
                .entity(entity)
                .get::<ResolvedLightShadowMotion>()
                .unwrap()
                .class,
            LightShadowMotionClass::FullyDynamic
        );
        advance_and_update(&mut app, 0.11);
        assert_eq!(
            app.world()
                .entity(entity)
                .get::<ResolvedLightShadowMotion>()
                .unwrap()
                .class,
            LightShadowMotionClass::Static
        );

        app.world_mut()
            .entity_mut(entity)
            .insert(GlobalTransform::from_translation(Vec3::X));
        advance_and_update(&mut app, 0.016);
        assert_eq!(
            app.world()
                .entity(entity)
                .get::<ResolvedLightShadowMotion>()
                .unwrap()
                .class,
            LightShadowMotionClass::FullyDynamic
        );
    }

    #[test]
    fn automatic_caster_moves_between_overlay_and_static_layers() {
        let mut app = policy_test_app();
        let entity = app
            .world_mut()
            .spawn((
                Transform::default(),
                GlobalTransform::default(),
                ShadowCasterMotionPolicy {
                    automatic: ShadowCasterAutomaticThresholds {
                        settle_seconds: 0.10,
                        ..default()
                    },
                    ..default()
                },
            ))
            .id();

        advance_and_update(&mut app, 0.05);
        assert!(app.world().entity(entity).contains::<DynamicShadowCaster>());
        advance_and_update(&mut app, 0.11);
        assert!(!app.world().entity(entity).contains::<DynamicShadowCaster>());

        app.world_mut()
            .entity_mut(entity)
            .insert(GlobalTransform::from_translation(Vec3::X));
        advance_and_update(&mut app, 0.016);
        assert!(app.world().entity(entity).contains::<DynamicShadowCaster>());
        assert_eq!(
            app.world()
                .entity(entity)
                .get::<ResolvedShadowCasterMotion>()
                .unwrap()
                .class,
            ShadowCasterMotionClass::DynamicOverlay
        );
    }

    #[test]
    fn imported_automatic_entities_begin_static_but_react_to_motion() {
        let mut app = policy_test_app();
        let entity = app
            .world_mut()
            .spawn((
                Transform::default(),
                GlobalTransform::default(),
                ImportedZevyEntity {
                    id: "fixture".to_owned(),
                    asset_id: None,
                },
                ShadowCasterMotionPolicy::automatic(),
            ))
            .id();
        app.update();
        assert!(!app.world().entity(entity).contains::<DynamicShadowCaster>());

        app.world_mut()
            .entity_mut(entity)
            .insert(GlobalTransform::from_translation(Vec3::Y));
        advance_and_update(&mut app, 0.016);
        assert!(app.world().entity(entity).contains::<DynamicShadowCaster>());
    }

    #[test]
    fn manual_caster_policy_is_authoritative_and_runtime_mutable() {
        let mut app = policy_test_app();
        let entity = app
            .world_mut()
            .spawn((
                Transform::default(),
                GlobalTransform::default(),
                DynamicShadowCaster,
                ShadowCasterMotionPolicy::fixed(ShadowCasterMotionClass::Static),
            ))
            .id();

        app.update();
        assert!(!app.world().entity(entity).contains::<DynamicShadowCaster>());
        app.world_mut()
            .entity_mut(entity)
            .insert(ShadowCasterMotionPolicy::fixed(
                ShadowCasterMotionClass::DynamicOverlay,
            ));
        app.update();
        assert!(app.world().entity(entity).contains::<DynamicShadowCaster>());
    }

    #[test]
    fn removing_policies_removes_their_managed_outputs() {
        let mut app = policy_test_app();
        let light = app
            .world_mut()
            .spawn((
                PointLight::default(),
                Transform::default(),
                GlobalTransform::default(),
                LightShadowMotionPolicy::fixed(LightShadowMotionClass::BoundedMicroMotion),
            ))
            .id();
        let caster = app
            .world_mut()
            .spawn((
                Transform::default(),
                GlobalTransform::default(),
                ShadowCasterMotionPolicy::fixed(ShadowCasterMotionClass::DynamicOverlay),
            ))
            .id();
        app.update();

        app.world_mut()
            .entity_mut(light)
            .remove::<LightShadowMotionPolicy>();
        app.world_mut()
            .entity_mut(caster)
            .remove::<ShadowCasterMotionPolicy>();
        app.update();

        let light = app.world().entity(light);
        assert!(!light.contains::<CachedPointLightShadow>());
        assert!(!light.contains::<PointLightShadowMapJitter>());
        assert!(!light.contains::<ResolvedLightShadowMotion>());
        let caster = app.world().entity(caster);
        assert!(!caster.contains::<DynamicShadowCaster>());
        assert!(!caster.contains::<ResolvedShadowCasterMotion>());
    }

    #[test]
    fn fully_dynamic_spot_light_resolves_without_point_shadow_cache() {
        let mut app = policy_test_app();
        let entity = app
            .world_mut()
            .spawn((
                SpotLight {
                    shadows_enabled: true,
                    ..default()
                },
                Transform::default(),
                GlobalTransform::default(),
                CachedPointLightShadow::default(),
                PointLightShadowMapJitter::default(),
                LightShadowMotionPolicy::fixed(LightShadowMotionClass::FullyDynamic),
            ))
            .id();

        app.update();

        let entity = app.world().entity(entity);
        assert_eq!(
            entity.get::<ResolvedLightShadowMotion>().unwrap().class,
            LightShadowMotionClass::FullyDynamic
        );
        assert!(!entity.contains::<CachedPointLightShadow>());
        assert!(!entity.contains::<PointLightShadowMapJitter>());
        assert_eq!(
            app.world()
                .resource::<ShadowMotionPolicyTelemetry>()
                .light_fully_dynamic,
            1
        );
    }

    #[test]
    fn slow_moving_light_starts_and_finishes_a_sparse_snapshot_transition() {
        let mut app = policy_test_app();
        let entity = app
            .world_mut()
            .spawn((
                PointLight {
                    shadows_enabled: true,
                    ..default()
                },
                Transform::default(),
                GlobalTransform::default(),
                LightShadowMotionPolicy::fixed(LightShadowMotionClass::SlowMoving),
            ))
            .id();

        app.update();
        let settled = app
            .world()
            .entity(entity)
            .get::<PointLightShadowMapTransition>()
            .copied()
            .unwrap();
        assert_eq!(settled.transition_slot, None);
        assert_eq!(settled.current_position, Vec3::ZERO);

        app.world_mut()
            .entity_mut(entity)
            .insert(GlobalTransform::from_translation(Vec3::new(0.05, 0.0, 0.0)));
        advance_and_update(&mut app, 0.016);
        let transition = app
            .world()
            .entity(entity)
            .get::<PointLightShadowMapTransition>()
            .copied()
            .unwrap();
        assert_eq!(transition.transition_slot, Some(0));
        assert_eq!(transition.previous_position, Vec3::ZERO);
        assert_eq!(transition.current_position, Vec3::new(0.05, 0.0, 0.0));
        assert_eq!(transition.blend, 0.0);
        assert_eq!(
            app.world()
                .resource::<ZevyShadowCacheFrame>()
                .transition_copy_requests(),
            &[crate::shadow_cache::PointShadowTransitionCopyRequest {
                light: entity,
                slot: 0,
            }]
        );

        advance_and_update(&mut app, 0.13);
        let settled = app
            .world()
            .entity(entity)
            .get::<PointLightShadowMapTransition>()
            .copied()
            .unwrap();
        assert_eq!(settled.transition_slot, None);
        assert_eq!(settled.blend, 1.0);
    }

    #[test]
    fn sparse_slot_contention_prefers_the_most_stale_light() {
        let mut app = policy_test_app();
        app.world_mut()
            .resource::<ZevyShadowCacheFrame>()
            .record_effective_transition_slots(1);
        let first = app
            .world_mut()
            .spawn((
                PointLight {
                    shadows_enabled: true,
                    ..default()
                },
                Transform::default(),
                GlobalTransform::default(),
                LightShadowMotionPolicy::fixed(LightShadowMotionClass::SlowMoving),
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                PointLight {
                    shadows_enabled: true,
                    ..default()
                },
                Transform::default(),
                GlobalTransform::default(),
                LightShadowMotionPolicy::fixed(LightShadowMotionClass::SlowMoving),
            ))
            .id();
        app.update();

        app.world_mut()
            .entity_mut(first)
            .insert(GlobalTransform::from_translation(Vec3::X * 0.05));
        app.world_mut()
            .entity_mut(second)
            .insert(GlobalTransform::from_translation(Vec3::X * 0.08));
        advance_and_update(&mut app, 0.016);

        assert_eq!(
            app.world()
                .entity(first)
                .get::<PointLightShadowMapTransition>()
                .unwrap()
                .transition_slot,
            None
        );
        assert_eq!(
            app.world()
                .entity(second)
                .get::<PointLightShadowMapTransition>()
                .unwrap()
                .transition_slot,
            Some(0)
        );
        let telemetry = app.world().resource::<ShadowMotionPolicyTelemetry>();
        assert_eq!(telemetry.slow_shadow_transition_waiting, 1);
        assert_eq!(telemetry.slow_shadow_transitions_active, 1);
        assert_eq!(telemetry.slow_shadow_transition_slot_capacity, 1);
    }

    #[test]
    fn snapshot_due_combines_time_and_projection_error() {
        assert!(!shadow_snapshot_is_due(0.01, 0.0, 0.05, Some(0.125), 0.04,));
        assert!(shadow_snapshot_is_due(0.04, 0.0, 0.05, Some(0.125), 0.04,));
        assert!(shadow_snapshot_is_due(0.01, 0.0, 0.125, Some(0.125), 0.04,));
        assert!(!shadow_snapshot_is_due(0.0, 0.0, 1.0, Some(0.125), 0.04,));
    }

    fn policy_test_app() -> App {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_plugins(ShadowMotionPolicyPlugin);
        app
    }

    fn advance_and_update(app: &mut App, seconds: f32) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(seconds));
        app.update();
    }

    fn assert_light_route(
        app: &App,
        entity: Entity,
        class: LightShadowMotionClass,
        cached: bool,
        jittered: bool,
    ) {
        let entity = app.world().entity(entity);
        assert_eq!(
            entity.get::<ResolvedLightShadowMotion>().unwrap().class,
            class
        );
        assert_eq!(entity.contains::<CachedPointLightShadow>(), cached);
        assert_eq!(entity.contains::<PointLightShadowMapJitter>(), jittered);
    }
}
