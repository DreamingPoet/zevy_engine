use bevy::{
    color::LinearRgba,
    ecs::system::ParamSet,
    pbr::{Clusters, PRESELECTED_CLUSTER_POINT_LIGHTS, PointLight, SimulationLightSystems},
    prelude::*,
};
use bevy_mod_xr::camera::XrCamera;

use crate::config::RenderQualityConfig;

const SUPERCLUSTER_XY: usize = 2;
const MIN_IMPORTANCE: f32 = 0.000001;

type ClusterSelection = [Option<(Entity, f32)>; PRESELECTED_CLUSTER_POINT_LIGHTS];

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct ClusterLightPreselectionStats {
    pub(crate) active: bool,
    pub(crate) views: u32,
    pub(crate) xr_views: u32,
    pub(crate) clusters: u32,
    pub(crate) superclusters: u32,
    pub(crate) nonempty_superclusters: u32,
    pub(crate) candidate_references: u64,
    pub(crate) max_candidates: u32,
}

pub(crate) struct ClusteredLightPreselectionPlugin;

impl Plugin for ClusteredLightPreselectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClusterLightPreselectionStats>()
            .add_systems(
                PostUpdate,
                preselect_cluster_point_lights
                    .after(SimulationLightSystems::AssignLightsToClusters),
            );
    }
}

#[derive(Clone)]
struct ClusterViewSnapshot {
    entity: Entity,
    xr_view: Option<u32>,
    dimensions: UVec3,
    centers_world: Vec<Vec3>,
    point_lights: Vec<Vec<Entity>>,
}

#[derive(Clone, Copy, Debug)]
struct WeightedCandidate {
    entity: Entity,
    importance: f32,
}

#[allow(clippy::type_complexity)]
fn preselect_cluster_point_lights(
    quality: Res<RenderQualityConfig>,
    mut view_queries: ParamSet<(
        Query<(
            Entity,
            &GlobalTransform,
            &Camera,
            &Clusters,
            Option<&XrCamera>,
        )>,
        Query<&mut Clusters>,
    )>,
    point_lights: Query<(&PointLight, &GlobalTransform)>,
    mut stats: ResMut<ClusterLightPreselectionStats>,
    mut logged_active: Local<bool>,
    mut logged_fallback: Local<bool>,
) {
    *stats = ClusterLightPreselectionStats::default();

    let hero_count = quality.resolved_point_light_hero_samples() as usize;
    let tail_count = quality.resolved_point_light_tail_samples() as usize;
    let supported_budget = hero_count + tail_count <= PRESELECTED_CLUSTER_POINT_LIGHTS;
    let active = quality.point_light_direct_lighting
        && quality.scalable_point_lighting
        && quality.clustered_light_preselection
        && !quality.world_space_light_reservoir
        && !quality.temporal_point_light_sampling
        && supported_budget;
    if !active {
        if quality.clustered_light_preselection
            && quality.world_space_light_reservoir
            && !*logged_fallback
        {
            warn!(
                "Aggressive 2x2 cluster preselection is ignored because the world-space reservoir path takes precedence"
            );
            *logged_fallback = true;
        } else if quality.clustered_light_preselection
            && (!supported_budget || quality.temporal_point_light_sampling)
            && !*logged_fallback
        {
            warn!(
                "Cluster light preselection requires Hero+Tail <= {} and world-stable sampling; using the scalar fragment reference path",
                PRESELECTED_CLUSTER_POINT_LIGHTS
            );
            *logged_fallback = true;
        }
        return;
    }

    let snapshots = {
        let views = view_queries.p0();
        views
            .iter()
            .filter_map(|(entity, transform, camera, clusters, xr_camera)| {
                if !camera.is_active || clusters.is_empty() {
                    return None;
                }
                let mut centers_world = Vec::with_capacity(clusters.len());
                let mut clustered_point_lights = Vec::with_capacity(clusters.len());
                for cluster_index in 0..clusters.len() {
                    let center_view = clusters
                        .cluster_aabb(camera, cluster_index)
                        .map(|aabb| Vec3::from(aabb.center))
                        .unwrap_or(Vec3::ZERO);
                    centers_world.push(transform.compute_matrix().transform_point3(center_view));
                    clustered_point_lights
                        .push(clusters.point_light_entities(cluster_index).to_vec());
                }
                Some(ClusterViewSnapshot {
                    entity,
                    xr_view: xr_camera.map(|camera| camera.0),
                    dimensions: clusters.dimensions(),
                    centers_world,
                    point_lights: clustered_point_lights,
                })
            })
            .collect::<Vec<_>>()
    };

    if snapshots.is_empty() {
        return;
    }

    let mut assignments = Vec::<(Entity, Vec<ClusterSelection>)>::new();
    let mut xr_snapshot_indices = snapshots
        .iter()
        .enumerate()
        .filter_map(|(index, snapshot)| snapshot.xr_view.map(|view| (view, index)))
        .collect::<Vec<_>>();
    xr_snapshot_indices.sort_unstable_by_key(|(view, _)| *view);

    if xr_snapshot_indices.len() >= 2 {
        let xr_snapshots = xr_snapshot_indices
            .iter()
            .map(|(_, index)| &snapshots[*index])
            .collect::<Vec<_>>();
        let common_dimensions = xr_snapshots[0].dimensions;
        if xr_snapshots
            .iter()
            .all(|snapshot| snapshot.dimensions == common_dimensions)
        {
            let selections = build_supercluster_selections(
                &xr_snapshots,
                hero_count,
                tail_count,
                &point_lights,
                &mut stats,
            );
            for snapshot in xr_snapshots {
                assignments.push((snapshot.entity, selections.clone()));
            }
        } else {
            if !*logged_fallback {
                warn!(
                    "XR cluster grids have different dimensions; selecting each view independently for this frame"
                );
                *logged_fallback = true;
            }
            for snapshot in xr_snapshots {
                let selections = build_supercluster_selections(
                    &[snapshot],
                    hero_count,
                    tail_count,
                    &point_lights,
                    &mut stats,
                );
                assignments.push((snapshot.entity, selections));
            }
        }
    } else {
        for (_, index) in &xr_snapshot_indices {
            let snapshot = &snapshots[*index];
            let selections = build_supercluster_selections(
                &[snapshot],
                hero_count,
                tail_count,
                &point_lights,
                &mut stats,
            );
            assignments.push((snapshot.entity, selections));
        }
    }

    for snapshot in snapshots
        .iter()
        .filter(|snapshot| snapshot.xr_view.is_none())
    {
        let selections = build_supercluster_selections(
            &[snapshot],
            hero_count,
            tail_count,
            &point_lights,
            &mut stats,
        );
        assignments.push((snapshot.entity, selections));
    }

    {
        let mut clusters_query = view_queries.p1();
        for (view_entity, selections) in assignments {
            let Ok(mut clusters) = clusters_query.get_mut(view_entity) else {
                continue;
            };
            for (cluster_index, selection) in selections.into_iter().enumerate() {
                clusters.set_preselected_point_lights(cluster_index, selection);
            }
        }
    }

    stats.active = true;
    stats.views = snapshots.len() as u32;
    stats.xr_views = xr_snapshot_indices.len() as u32;
    stats.clusters = snapshots
        .iter()
        .map(|snapshot| snapshot.point_lights.len() as u32)
        .sum();

    if !*logged_active {
        info!(
            "Cyclopean cluster light preselection active: {} XR views, 2x2 superclusters, {} Hero + {} Tail, four fixed GPU IDs per cluster",
            stats.xr_views, hero_count, tail_count
        );
        *logged_active = true;
    }
}

fn build_supercluster_selections(
    views: &[&ClusterViewSnapshot],
    hero_count: usize,
    tail_count: usize,
    point_lights: &Query<(&PointLight, &GlobalTransform)>,
    stats: &mut ClusterLightPreselectionStats,
) -> Vec<ClusterSelection> {
    let dimensions = views[0].dimensions;
    let cluster_count = views[0].point_lights.len();
    let mut selections = vec![[None; PRESELECTED_CLUSTER_POINT_LIGHTS]; cluster_count];
    let x_count = dimensions.x as usize;
    let y_count = dimensions.y as usize;
    let z_count = dimensions.z as usize;

    for y_start in (0..y_count).step_by(SUPERCLUSTER_XY) {
        for x_start in (0..x_count).step_by(SUPERCLUSTER_XY) {
            for z in 0..z_count {
                let mut candidates = Vec::<Entity>::new();
                let mut sample_points = Vec::<Vec3>::new();

                for view in views {
                    for y in y_start..(y_start + SUPERCLUSTER_XY).min(y_count) {
                        for x in x_start..(x_start + SUPERCLUSTER_XY).min(x_count) {
                            let cluster_index = cluster_index(dimensions, x, y, z);
                            sample_points.push(view.centers_world[cluster_index]);
                            for entity in &view.point_lights[cluster_index] {
                                if !candidates.contains(entity) {
                                    candidates.push(*entity);
                                }
                            }
                        }
                    }
                }

                stats.superclusters += 1;
                if !candidates.is_empty() {
                    stats.nonempty_superclusters += 1;
                }
                stats.candidate_references += candidates.len() as u64;
                stats.max_candidates = stats.max_candidates.max(candidates.len() as u32);

                let anchor = if sample_points.is_empty() {
                    Vec3::ZERO
                } else {
                    sample_points.iter().copied().sum::<Vec3>() / sample_points.len() as f32
                };
                let weighted_candidates = candidates
                    .into_iter()
                    .filter_map(|entity| {
                        let (light, transform) = point_lights.get(entity).ok()?;
                        let importance = sample_points
                            .iter()
                            .map(|point| point_light_importance(light, transform, *point))
                            .fold(MIN_IMPORTANCE, f32::max);
                        Some(WeightedCandidate { entity, importance })
                    })
                    .collect::<Vec<_>>();
                let selection = select_weighted_candidates(
                    weighted_candidates,
                    hero_count,
                    tail_count,
                    stable_world_jitter(anchor),
                );

                for y in y_start..(y_start + SUPERCLUSTER_XY).min(y_count) {
                    for x in x_start..(x_start + SUPERCLUSTER_XY).min(x_count) {
                        selections[cluster_index(dimensions, x, y, z)] = selection;
                    }
                }
            }
        }
    }

    selections
}

fn cluster_index(dimensions: UVec3, x: usize, y: usize, z: usize) -> usize {
    ((y * dimensions.x as usize + x) * dimensions.z as usize) + z
}

fn point_light_importance(
    light: &PointLight,
    transform: &GlobalTransform,
    world_position: Vec3,
) -> f32 {
    let light_to_point = transform.translation() - world_position;
    let distance_squared = light_to_point.length_squared();
    let inverse_square_range = 1.0 / (light.range * light.range).max(0.0001);
    let range_factor = distance_squared * inverse_square_range;
    let smooth_factor = (1.0 - range_factor * range_factor).clamp(0.0, 1.0);
    let attenuation = smooth_factor * smooth_factor / distance_squared.max(0.0001);
    let color = LinearRgba::from(light.color);
    let luminance =
        (color.red * 0.2126 + color.green * 0.7152 + color.blue * 0.0722) * light.intensity;
    (luminance * attenuation).max(MIN_IMPORTANCE)
}

fn select_weighted_candidates(
    mut candidates: Vec<WeightedCandidate>,
    hero_count: usize,
    tail_count: usize,
    jitter: f32,
) -> ClusterSelection {
    candidates.sort_unstable_by(|left, right| {
        right
            .importance
            .total_cmp(&left.importance)
            .then_with(|| left.entity.to_bits().cmp(&right.entity.to_bits()))
    });

    let mut selected = [None; PRESELECTED_CLUSTER_POINT_LIGHTS];
    if candidates.is_empty() {
        return selected;
    }

    let full_budget = (hero_count + tail_count).min(PRESELECTED_CLUSTER_POINT_LIGHTS);
    if candidates.len() <= full_budget {
        for (slot, candidate) in candidates.into_iter().enumerate() {
            selected[slot] = Some((candidate.entity, 1.0));
        }
        return selected;
    }

    let resolved_hero_count = hero_count.min(candidates.len());
    for (slot, candidate) in candidates.iter().take(resolved_hero_count).enumerate() {
        selected[slot] = Some((candidate.entity, 1.0));
    }

    let tail = &candidates[resolved_hero_count..];
    let resolved_tail_count = tail_count
        .min(PRESELECTED_CLUSTER_POINT_LIGHTS - resolved_hero_count)
        .min(tail.len());
    if resolved_tail_count == 0 {
        return selected;
    }
    if tail.len() <= resolved_tail_count {
        for (tail_slot, candidate) in tail.iter().enumerate() {
            selected[resolved_hero_count + tail_slot] = Some((candidate.entity, 1.0));
        }
        return selected;
    }

    let importance_sum = tail
        .iter()
        .map(|candidate| candidate.importance)
        .sum::<f32>();
    if importance_sum <= 0.0 || !importance_sum.is_finite() {
        return selected;
    }

    for sample_index in 0..resolved_tail_count {
        let target = (sample_index as f32 + jitter.clamp(0.0, 0.999999))
            / resolved_tail_count as f32
            * importance_sum;
        let mut accumulated = 0.0;
        let mut sampled = tail.last().copied();
        for candidate in tail {
            accumulated += candidate.importance;
            if target <= accumulated {
                sampled = Some(*candidate);
                break;
            }
        }
        if let Some(candidate) = sampled {
            let probability = candidate.importance / importance_sum;
            let estimator_weight =
                1.0 / (probability * resolved_tail_count as f32).max(MIN_IMPORTANCE);
            selected[resolved_hero_count + sample_index] =
                Some((candidate.entity, estimator_weight));
        }
    }

    selected
}

fn stable_world_jitter(world_position: Vec3) -> f32 {
    let cell = (world_position * 2.0).floor().as_ivec3();
    let mut seed = (cell.x as u32).wrapping_mul(0x8da6_b343);
    seed ^= (cell.y as u32).wrapping_mul(0xd816_3841);
    seed ^= (cell.z as u32).wrapping_mul(0xcb1a_b31f);
    let hash = zevy_hash_u32(seed);
    (hash & 0x00ff_ffff) as f32 / 16_777_216.0
}

fn zevy_hash_u32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(index: u32) -> Entity {
        Entity::from_raw(index)
    }

    #[test]
    fn exact_small_cluster_keeps_every_light_once() {
        let selected = select_weighted_candidates(
            vec![
                WeightedCandidate {
                    entity: entity(1),
                    importance: 1.0,
                },
                WeightedCandidate {
                    entity: entity(2),
                    importance: 3.0,
                },
                WeightedCandidate {
                    entity: entity(3),
                    importance: 2.0,
                },
            ],
            2,
            2,
            0.25,
        );
        assert_eq!(selected[0], Some((entity(2), 1.0)));
        assert_eq!(selected[1], Some((entity(3), 1.0)));
        assert_eq!(selected[2], Some((entity(1), 1.0)));
        assert_eq!(selected[3], None);
    }

    #[test]
    fn hero_set_is_deterministic_and_tail_weights_are_finite() {
        let candidates = (1..=8)
            .map(|index| WeightedCandidate {
                entity: entity(index),
                importance: index as f32,
            })
            .collect::<Vec<_>>();
        let selected_a = select_weighted_candidates(candidates.clone(), 2, 2, 0.42);
        let selected_b = select_weighted_candidates(candidates, 2, 2, 0.42);
        assert_eq!(selected_a, selected_b);
        assert_eq!(selected_a[0], Some((entity(8), 1.0)));
        assert_eq!(selected_a[1], Some((entity(7), 1.0)));
        for sample in selected_a.into_iter().flatten() {
            assert!(sample.1.is_finite() && sample.1 > 0.0);
        }
    }
}
