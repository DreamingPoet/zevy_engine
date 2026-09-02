use bevy::{
    core_pipeline::prepass::{DeferredPrepass, DepthPrepass},
    prelude::*,
    render::view::Msaa,
};

/// Main opaque local-lighting architecture.
///
/// `DeferredReference` is deliberately a full-resolution correctness and cost
/// baseline. It does not claim reduced-rate shading; it establishes the
/// G-buffer path needed by Zevy's later low-resolution lighting and
/// edge-aware reconstruction experiments.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LocalLightingPipeline {
    #[default]
    Forward,
    DeferredReference,
}

impl LocalLightingPipeline {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Forward => "forward reference",
            Self::DeferredReference => "full-resolution deferred reference",
        }
    }

    pub(crate) fn uses_deferred_prepass(self) -> bool {
        self == Self::DeferredReference
    }

    #[cfg(any(test, all(target_os = "android", feature = "render_debug")))]
    pub(crate) fn from_debug_label(label: &str) -> Option<Self> {
        match label.trim().to_ascii_lowercase().as_str() {
            "forward" | "forward-reference" => Some(Self::Forward),
            "deferred" | "deferred-reference" => Some(Self::DeferredReference),
            _ => None,
        }
    }
}

/// Central rendering-quality settings for desktop and XR.
///
/// Edit the values in [`Default`] and rebuild the application to compare
/// quality/performance configurations. XR swapchain resolution is selected
/// when the OpenXR session is created, so changing `xr_render_scale` requires
/// restarting the application.
#[derive(Resource, Clone, Copy, Debug)]
pub struct RenderQualityConfig {
    /// Scale applied to the OpenXR runtime's recommended per-eye resolution.
    /// `1.0` uses the full recommended resolution; `0.8` renders 64% as many
    /// pixels per eye.
    pub xr_render_scale: f32,
    /// Selects the main local-lighting architecture. Keep `Forward` as the
    /// product reference while the deferred/reconstruction path is measured.
    pub local_lighting_pipeline: LocalLightingPipeline,
    /// Supported values are 1 (off), 2, 4, and 8 samples.
    /// Full-resolution deferred forces this to 1 because Bevy's G-buffer path
    /// is single-sampled; the effective value is reported explicitly.
    pub msaa_samples: u32,
    /// Width and height of each of the six point-light shadow cubemap faces.
    /// Bevy's default is 1024; 128 is the current multi-light VR test tier.
    pub point_shadow_map_size: usize,
    /// Optional stable cap on imported Map_S03B PointLights with resident
    /// cubemap shadows. `0` means all lights that had shadows enabled in the
    /// exported level. A positive value is an explicit performance-test cap.
    /// Selection never depends on camera distance.
    pub max_shadowed_point_lights: usize,
    /// Maximum number of clustered-forward froxels for a view.
    pub cluster_total: u32,
    /// Number of logarithmic depth slices in the clustered-forward grid.
    pub cluster_z_slices: u32,
    /// Far plane of the first clustered-forward depth slice, in meters.
    pub cluster_first_slice_depth_m: f32,
    /// Constant clustered-lighting far distance, in meters. This is separate
    /// from each light's physical illumination range.
    pub cluster_far_z_m: f32,
    /// Fixed A/B switch for PointLight direct shading. With Zevy's scalable
    /// shader this is compiled out, so `false` measures the geometry/post and
    /// shadow-submission floor without paying the per-fragment PointLight loop.
    /// It is never changed from camera distance or independently per eye.
    pub point_light_direct_lighting: bool,
    /// Fixed A/B and quality-tier switch for imported PointLight shadows.
    /// `false` disables shadow residency for the Map_S03B test profile without
    /// changing physical light ranges or making shadows camera dependent.
    pub point_light_shadows: bool,
    /// Replaces Bevy's unbounded per-cluster PointLight BRDF loop with Zevy's
    /// fixed-budget Hero + importance-sampled tail path.
    pub scalable_point_lighting: bool,
    /// Aggressive fixed O(1) A/B path that moves Hero/Tail selection to a
    /// Cyclopean 2x2 CPU supercluster. It is disabled by default because moving
    /// head tests exposed screen-block brightness discontinuities. Keep it as
    /// a reversible experiment, not as the product-quality fallback.
    pub clustered_light_preselection: bool,
    /// Product-quality scalable path for clusters above the exact-light budget.
    /// A single walk over the real per-fragment cluster simultaneously finds
    /// deterministic Hero lights and two world-anchored weighted reservoirs.
    /// This removes the second O(N) scan without tying approximation boundaries
    /// to screen-space superclusters. It takes precedence over the aggressive
    /// preselection switch when both are enabled.
    pub world_space_light_reservoir: bool,
    /// Real cluster lists at or below this count are always evaluated exactly.
    /// Map_S03B defaults to eight so its verified local-light overlap never
    /// exposes raw stochastic shadow samples. Higher-density overflow can still
    /// use the experimental world-space reservoir.
    pub point_light_exact_threshold: u32,
    /// Number of highest-contribution PointLights evaluated deterministically
    /// at each shading point, independent of camera-to-light distance.
    pub point_light_hero_samples: u32,
    /// Maximum number of remaining PointLights whose full shadowed BRDF is
    /// importance sampled per shading point. `0` is an explicit A/B/quality
    /// tier that evaluates only deterministic Hero lights.
    pub point_light_tail_samples: u32,
    /// Allows the stochastic tail selection to rotate over time. Disabled by
    /// default for VR so lighting and shadows remain anchored in world space.
    pub temporal_point_light_sampling: bool,
    /// Number of frames for which a stochastic light selection remains stable.
    /// Used only when `temporal_point_light_sampling` is enabled.
    pub light_sample_period_frames: u32,
    /// Keeps point-light shadow atlas layers across frames and redraws only
    /// invalidated lights. Non-cacheable lights retain Bevy's normal behavior.
    pub persistent_point_shadow_cache: bool,
    /// Splits the PointLight cube-array into cached static depth and a separate
    /// dynamic-caster overlay. The PBR shader combines both visibility terms.
    pub dynamic_shadow_caster_overlay: bool,
    /// Keeps candle PointLight transforms and ranges fixed while applying a
    /// sub-centimeter virtual offset only during cached cubemap lookup. This
    /// makes projection motion continuous without invalidating six static
    /// shadow faces per light. Disable for the real-redraw A/B reference path.
    pub continuous_point_shadow_proxy: bool,
    /// Multiplier for the Map_S03B candle proxy's authored 5 mm sway. This is
    /// clamped so the virtual origin remains inside the packed GPU range.
    pub point_shadow_proxy_sway_scale: f32,
    /// Number of full redraw frames used when an atlas layout first appears,
    /// allowing asynchronously-created shadow pipelines and meshes to settle.
    pub point_shadow_cache_warmup_frames: u8,
    /// Target update frequency for cached candle-light projection movement.
    /// Intensity, color, and emissive animation still update every frame.
    pub cached_point_shadow_update_hz: f32,
    /// Hard cap on cached PointLights invalidated in one frame. Due lights are
    /// scheduled oldest-first so an overloaded frame budget cannot starve a
    /// subset of lights. Each point-light update redraws six cubemap faces.
    pub max_cached_point_shadow_updates_per_frame: usize,
    /// Enables sparse two-snapshot reconstruction for PointLights resolved as
    /// `SlowMoving`. The static cubemap updates at keyframes while the shader
    /// cross-fades old and new shadow visibility in a shared XR timeline.
    pub slow_moving_shadow_crossfade: bool,
    /// Maximum number of old PointLight cubemaps kept concurrently. This is a
    /// sparse pool, not a third cubemap allocated for every shadowed light.
    pub slow_moving_shadow_transition_slots: usize,
    /// Maximum cadence at which a slow-moving light may create a new static
    /// shadow snapshot. Zero disables time-driven updates; distance can still
    /// trigger one.
    pub slow_moving_shadow_snapshot_hz: f32,
    /// World-space displacement that forces a new slow-moving shadow snapshot
    /// even before the cadence interval expires.
    pub slow_moving_shadow_snapshot_distance_m: f32,
    /// Time used to blend old and new static shadow visibility.
    pub slow_moving_shadow_crossfade_seconds: f32,
}

impl Default for RenderQualityConfig {
    fn default() -> Self {
        Self {
            xr_render_scale: 0.8,
            local_lighting_pipeline: LocalLightingPipeline::Forward,
            msaa_samples: 2,
            point_shadow_map_size: 128,
            max_shadowed_point_lights: 0,
            cluster_total: 4096,
            cluster_z_slices: 24,
            cluster_first_slice_depth_m: 4.0,
            cluster_far_z_m: 128.0,
            point_light_direct_lighting: true,
            point_light_shadows: true,
            scalable_point_lighting: true,
            clustered_light_preselection: false,
            world_space_light_reservoir: true,
            point_light_exact_threshold: 18,
            point_light_hero_samples: 2,
            point_light_tail_samples: 2,
            temporal_point_light_sampling: false,
            light_sample_period_frames: 4,
            persistent_point_shadow_cache: true,
            dynamic_shadow_caster_overlay: true,
            continuous_point_shadow_proxy: true,
            point_shadow_proxy_sway_scale: 1.0,
            point_shadow_cache_warmup_frames: 3,
            cached_point_shadow_update_hz: 8.0,
            max_cached_point_shadow_updates_per_frame: 8,
            slow_moving_shadow_crossfade: true,
            slow_moving_shadow_transition_slots: 4,
            slow_moving_shadow_snapshot_hz: 8.0,
            slow_moving_shadow_snapshot_distance_m: 0.04,
            slow_moving_shadow_crossfade_seconds: 0.12,
        }
    }
}

impl RenderQualityConfig {
    pub(crate) fn resolved_xr_render_scale(self) -> f32 {
        if self.xr_render_scale.is_finite() {
            self.xr_render_scale.clamp(0.25, 2.0)
        } else {
            1.0
        }
    }

    fn resolved_requested_msaa(self) -> Msaa {
        match self.msaa_samples {
            1 => Msaa::Off,
            2 => Msaa::Sample2,
            4 => Msaa::Sample4,
            8 => Msaa::Sample8,
            _ => Msaa::Sample2,
        }
    }

    pub(crate) fn resolved_msaa(self) -> Msaa {
        if self.local_lighting_pipeline.uses_deferred_prepass() {
            Msaa::Off
        } else {
            self.resolved_requested_msaa()
        }
    }

    pub(crate) fn resolved_point_shadow_map_size(self) -> usize {
        self.point_shadow_map_size
            .clamp(64, 2048)
            .next_power_of_two()
    }

    pub(crate) fn resolved_point_shadow_resident_count(self, enabled_count: usize) -> usize {
        if !self.point_light_shadows {
            0
        } else if self.max_shadowed_point_lights == 0 {
            enabled_count
        } else {
            enabled_count.min(self.max_shadowed_point_lights)
        }
    }

    pub(crate) fn point_light_ab_profile_label(self) -> &'static str {
        match (self.point_light_direct_lighting, self.point_light_shadows) {
            (true, true) => "FULL: direct + shadows",
            (true, false) => "DIRECT ONLY: shadows off",
            (false, true) => "SHADOW SUBMISSION ONLY: direct shader off",
            (false, false) => "GEOMETRY / POST FLOOR",
        }
    }

    pub(crate) fn resolved_cluster_total(self) -> u32 {
        self.cluster_total.clamp(1, 8192)
    }

    pub(crate) fn resolved_cluster_z_slices(self) -> u32 {
        self.cluster_z_slices
            .clamp(1, self.resolved_cluster_total())
    }

    pub(crate) fn resolved_cluster_first_slice_depth_m(self) -> f32 {
        if self.cluster_first_slice_depth_m.is_finite() {
            self.cluster_first_slice_depth_m.clamp(0.1, 32.0)
        } else {
            4.0
        }
    }

    pub(crate) fn resolved_cluster_far_z_m(self) -> f32 {
        if self.cluster_far_z_m.is_finite() {
            self.cluster_far_z_m
                .max(self.resolved_cluster_first_slice_depth_m() + 1.0)
                .min(2_000.0)
        } else {
            128.0
        }
    }

    pub(crate) fn resolved_point_light_tail_samples(self) -> u32 {
        self.point_light_tail_samples.min(8)
    }

    pub(crate) fn resolved_point_light_hero_samples(self) -> u32 {
        self.point_light_hero_samples.clamp(1, 2)
    }

    pub(crate) fn resolved_point_light_exact_threshold(self) -> u32 {
        let fixed_sample_budget = self
            .resolved_point_light_hero_samples()
            .saturating_add(self.resolved_point_light_tail_samples());
        self.point_light_exact_threshold
            .max(fixed_sample_budget)
            .min(64)
    }

    pub(crate) fn point_light_selection_mode_label(self) -> &'static str {
        if !self.scalable_point_lighting {
            "Bevy deterministic"
        } else if self.world_space_light_reservoir {
            "single-scan world reservoir"
        } else if self.clustered_light_preselection {
            "aggressive 2x2 supercluster"
        } else {
            "scalar Hero/Tail reference"
        }
    }

    pub(crate) fn resolved_light_sample_period_frames(self) -> u32 {
        self.light_sample_period_frames.clamp(1, 120)
    }

    pub(crate) fn resolved_point_shadow_cache_warmup_frames(self) -> u8 {
        self.point_shadow_cache_warmup_frames.clamp(1, 30)
    }

    pub(crate) fn continuous_point_shadow_proxy_enabled(self) -> bool {
        self.point_light_shadows
            && self.scalable_point_lighting
            && self.persistent_point_shadow_cache
            && self.continuous_point_shadow_proxy
    }

    pub(crate) fn resolved_point_shadow_proxy_sway_scale(self) -> f32 {
        if self.point_shadow_proxy_sway_scale.is_finite() {
            self.point_shadow_proxy_sway_scale.clamp(0.0, 4.0)
        } else {
            1.0
        }
    }

    pub(crate) fn resolved_cached_point_shadow_update_hz(self) -> f32 {
        if self.cached_point_shadow_update_hz.is_finite() {
            self.cached_point_shadow_update_hz.clamp(0.0, 30.0)
        } else {
            8.0
        }
    }

    pub(crate) fn slow_moving_shadow_crossfade_enabled(self) -> bool {
        self.point_light_shadows
            && self.scalable_point_lighting
            && self.persistent_point_shadow_cache
            && self.slow_moving_shadow_crossfade
            && self.resolved_slow_moving_shadow_transition_slots() > 0
            && self.resolved_slow_moving_shadow_crossfade_seconds() > 0.0
    }

    pub(crate) fn resolved_slow_moving_shadow_transition_slots(self) -> usize {
        self.slow_moving_shadow_transition_slots.min(16)
    }

    pub(crate) fn resolved_slow_moving_shadow_snapshot_hz(self) -> f32 {
        if self.slow_moving_shadow_snapshot_hz.is_finite() {
            self.slow_moving_shadow_snapshot_hz.clamp(0.0, 60.0)
        } else {
            8.0
        }
    }

    pub(crate) fn resolved_slow_moving_shadow_snapshot_distance_m(self) -> f32 {
        if self.slow_moving_shadow_snapshot_distance_m.is_finite() {
            self.slow_moving_shadow_snapshot_distance_m
                .clamp(0.001, 2.0)
        } else {
            0.04
        }
    }

    pub(crate) fn resolved_slow_moving_shadow_crossfade_seconds(self) -> f32 {
        if self.slow_moving_shadow_crossfade_seconds.is_finite() {
            self.slow_moving_shadow_crossfade_seconds.clamp(0.0, 2.0)
        } else {
            0.12
        }
    }
}

/// Tracks only the prepass components inserted by Zevy so switching the
/// experiment back to Forward can restore a camera's prior state instead of
/// deleting components owned by another renderer feature.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct ZevyDeferredLightingCamera {
    had_depth_prepass: bool,
    had_deferred_prepass: bool,
}

pub(crate) fn apply_render_quality_to_cameras(
    config: Res<RenderQualityConfig>,
    mut commands: Commands,
    mut cameras: Query<
        (
            Entity,
            &mut Msaa,
            Has<DepthPrepass>,
            Has<DeferredPrepass>,
            Option<&ZevyDeferredLightingCamera>,
        ),
        With<Camera3d>,
    >,
) {
    let configured_msaa = config.resolved_msaa();
    let use_deferred = config.local_lighting_pipeline.uses_deferred_prepass();
    for (entity, mut msaa, has_depth, has_deferred, zevy_deferred) in &mut cameras {
        if *msaa != configured_msaa {
            *msaa = configured_msaa;
        }

        if use_deferred {
            let mut camera = commands.entity(entity);
            if zevy_deferred.is_none() {
                camera.insert(ZevyDeferredLightingCamera {
                    had_depth_prepass: has_depth,
                    had_deferred_prepass: has_deferred,
                });
            }
            if !has_depth {
                camera.insert(DepthPrepass);
            }
            if !has_deferred {
                camera.insert(DeferredPrepass);
            }
        } else if let Some(previous) = zevy_deferred {
            let mut camera = commands.entity(entity);
            camera.remove::<ZevyDeferredLightingCamera>();
            if !previous.had_depth_prepass {
                camera.remove::<DepthPrepass>();
            }
            if !previous.had_deferred_prepass {
                camera.remove::<DeferredPrepass>();
            }
        }
    }
}

pub(crate) fn log_render_quality_config(config: Res<RenderQualityConfig>) {
    let resolved_scale = config.resolved_xr_render_scale();
    let resolved_msaa = config.resolved_msaa();
    let requested_msaa = config.resolved_requested_msaa();
    let point_shadow_policy = if !config.point_light_shadows {
        "disabled by fixed A/B profile".to_owned()
    } else if config.max_shadowed_point_lights == 0 {
        "all level-enabled lights".to_owned()
    } else {
        format!("up to {} lights", config.max_shadowed_point_lights)
    };
    let dynamic_overlay_enabled = config.point_light_shadows
        && config.persistent_point_shadow_cache
        && config.scalable_point_lighting
        && config.dynamic_shadow_caster_overlay;

    if config.msaa_samples != requested_msaa.samples() {
        warn!(
            "Unsupported RenderQualityConfig.msaa_samples={}; using {}x",
            config.msaa_samples,
            requested_msaa.samples()
        );
    }
    if config.local_lighting_pipeline.uses_deferred_prepass() && requested_msaa != resolved_msaa {
        info!(
            "{} disables MSAA: requested {}x, effective {}x",
            config.local_lighting_pipeline.label(),
            requested_msaa.samples(),
            resolved_msaa.samples(),
        );
    }
    if (config.xr_render_scale - resolved_scale).abs() > f32::EPSILON {
        warn!(
            "RenderQualityConfig.xr_render_scale={} is outside the supported range; using {}",
            config.xr_render_scale, resolved_scale
        );
    }
    if !config.point_light_direct_lighting && !config.scalable_point_lighting {
        warn!(
            "RenderQualityConfig.point_light_direct_lighting=false only compiles out the PointLight fragment loop when scalable_point_lighting=true"
        );
    }
    if config.world_space_light_reservoir && config.clustered_light_preselection {
        warn!(
            "Both world_space_light_reservoir and clustered_light_preselection are enabled; the world-space path takes precedence"
        );
    }

    info!(
        "Render quality: XR scale {:.2}, {} with MSAA {}x, A/B {}, point shadow eligibility {} at {}px, clusters {}/{}z to {:.1}m, scalable point lights {} ({} Hero + {} tail samples, exact through {} lights, {}, selection {}), persistent shadow cache {} + dynamic overlay {}, continuous projection proxy {} at {:.2}x (redraw reference {} Hz, {} light/frame)",
        resolved_scale,
        config.local_lighting_pipeline.label(),
        resolved_msaa.samples(),
        config.point_light_ab_profile_label(),
        point_shadow_policy,
        config.resolved_point_shadow_map_size(),
        config.resolved_cluster_total(),
        config.resolved_cluster_z_slices(),
        config.resolved_cluster_far_z_m(),
        if config.scalable_point_lighting {
            "on"
        } else {
            "off"
        },
        config.resolved_point_light_hero_samples(),
        config.resolved_point_light_tail_samples(),
        config.resolved_point_light_exact_threshold(),
        if config.temporal_point_light_sampling {
            format!(
                "{} frame temporal rotation",
                config.resolved_light_sample_period_frames()
            )
        } else {
            "world-stable sampling".to_owned()
        },
        config.point_light_selection_mode_label(),
        if config.persistent_point_shadow_cache {
            "on"
        } else {
            "off"
        },
        if dynamic_overlay_enabled { "on" } else { "off" },
        if config.continuous_point_shadow_proxy_enabled() {
            "on"
        } else {
            "off"
        },
        config.resolved_point_shadow_proxy_sway_scale(),
        config.resolved_cached_point_shadow_update_hz(),
        config.max_cached_point_shadow_updates_per_frame,
    );
    info!(
        "SlowMoving point shadows: cross-fade {}, {} sparse slots, snapshots up to {:.1} Hz or {:.3} m displacement, {:.3} s blend",
        if config.slow_moving_shadow_crossfade_enabled() {
            "on"
        } else {
            "off"
        },
        config.resolved_slow_moving_shadow_transition_slots(),
        config.resolved_slow_moving_shadow_snapshot_hz(),
        config.resolved_slow_moving_shadow_snapshot_distance_m(),
        config.resolved_slow_moving_shadow_crossfade_seconds(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_point_shadow_residency_tracks_the_imported_light_count() {
        let quality = RenderQualityConfig::default();
        assert_eq!(
            quality.local_lighting_pipeline,
            LocalLightingPipeline::Forward
        );
        assert_eq!(quality.max_shadowed_point_lights, 0);
        assert_eq!(quality.resolved_point_shadow_resident_count(16), 16);
        assert!(quality.point_light_direct_lighting);
        assert!(quality.point_light_shadows);
        assert!(quality.dynamic_shadow_caster_overlay);
        assert!(quality.continuous_point_shadow_proxy_enabled());
        assert_eq!(quality.resolved_point_shadow_proxy_sway_scale(), 1.0);
        assert!(!quality.clustered_light_preselection);
        assert!(quality.world_space_light_reservoir);
        assert_eq!(quality.resolved_point_light_exact_threshold(), 18);
        assert_eq!(
            quality.point_light_selection_mode_label(),
            "single-scan world reservoir"
        );
    }

    #[test]
    fn deferred_reference_is_explicit_and_forces_single_sample_gbuffer() {
        let quality = RenderQualityConfig {
            local_lighting_pipeline: LocalLightingPipeline::DeferredReference,
            msaa_samples: 8,
            ..default()
        };
        assert!(quality.local_lighting_pipeline.uses_deferred_prepass());
        assert_eq!(quality.resolved_requested_msaa(), Msaa::Sample8);
        assert_eq!(quality.resolved_msaa(), Msaa::Off);
        assert_eq!(
            LocalLightingPipeline::from_debug_label("deferred"),
            Some(LocalLightingPipeline::DeferredReference)
        );
        assert_eq!(LocalLightingPipeline::from_debug_label("unknown"), None);
    }

    #[test]
    fn deferred_camera_switch_restores_preexisting_prepass_state() {
        let mut app = App::new();
        app.insert_resource(RenderQualityConfig {
            local_lighting_pipeline: LocalLightingPipeline::DeferredReference,
            msaa_samples: 4,
            ..default()
        })
        .add_systems(Update, apply_render_quality_to_cameras);

        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), Msaa::Sample4, DepthPrepass))
            .id();
        app.update();

        let deferred_camera = app.world().entity(camera);
        assert_eq!(deferred_camera.get::<Msaa>(), Some(&Msaa::Off));
        assert!(deferred_camera.contains::<DepthPrepass>());
        assert!(deferred_camera.contains::<DeferredPrepass>());
        assert!(deferred_camera.contains::<ZevyDeferredLightingCamera>());

        app.world_mut()
            .resource_mut::<RenderQualityConfig>()
            .local_lighting_pipeline = LocalLightingPipeline::Forward;
        app.update();

        let forward_camera = app.world().entity(camera);
        assert_eq!(forward_camera.get::<Msaa>(), Some(&Msaa::Sample4));
        assert!(forward_camera.contains::<DepthPrepass>());
        assert!(!forward_camera.contains::<DeferredPrepass>());
        assert!(!forward_camera.contains::<ZevyDeferredLightingCamera>());
    }

    #[test]
    fn explicit_point_shadow_residency_cap_remains_available_for_ab_tests() {
        let quality = RenderQualityConfig {
            max_shadowed_point_lights: 7,
            ..default()
        };
        assert_eq!(quality.resolved_point_shadow_resident_count(16), 7);
        assert_eq!(quality.resolved_point_shadow_resident_count(4), 4);
    }

    #[test]
    fn point_light_ab_switches_form_a_stable_four_way_matrix() {
        let full = RenderQualityConfig::default();
        assert_eq!(
            full.point_light_ab_profile_label(),
            "FULL: direct + shadows"
        );

        let direct_only = RenderQualityConfig {
            point_light_shadows: false,
            ..full
        };
        assert_eq!(direct_only.resolved_point_shadow_resident_count(16), 0);
        assert_eq!(
            direct_only.point_light_ab_profile_label(),
            "DIRECT ONLY: shadows off"
        );

        let floor = RenderQualityConfig {
            point_light_direct_lighting: false,
            point_light_shadows: false,
            ..full
        };
        assert_eq!(
            floor.point_light_ab_profile_label(),
            "GEOMETRY / POST FLOOR"
        );

        let hero_only = RenderQualityConfig {
            point_light_tail_samples: 0,
            ..full
        };
        assert_eq!(hero_only.resolved_point_light_tail_samples(), 0);
    }

    #[test]
    fn light_selection_modes_are_explicit_and_world_path_has_priority() {
        let world = RenderQualityConfig::default();
        assert_eq!(
            world.point_light_selection_mode_label(),
            "single-scan world reservoir"
        );

        let scalar = RenderQualityConfig {
            world_space_light_reservoir: false,
            clustered_light_preselection: false,
            ..world
        };
        assert_eq!(
            scalar.point_light_selection_mode_label(),
            "scalar Hero/Tail reference"
        );

        let aggressive = RenderQualityConfig {
            clustered_light_preselection: true,
            ..scalar
        };
        assert_eq!(
            aggressive.point_light_selection_mode_label(),
            "aggressive 2x2 supercluster"
        );

        let both = RenderQualityConfig {
            world_space_light_reservoir: true,
            ..aggressive
        };
        assert_eq!(
            both.point_light_selection_mode_label(),
            "single-scan world reservoir"
        );
    }

    #[test]
    fn exact_light_threshold_never_drops_below_the_fixed_sample_budget() {
        let quality = RenderQualityConfig {
            point_light_exact_threshold: 1,
            point_light_hero_samples: 2,
            point_light_tail_samples: 2,
            ..default()
        };
        assert_eq!(quality.resolved_point_light_exact_threshold(), 4);

        let exact_reference = RenderQualityConfig {
            point_light_exact_threshold: 16,
            ..quality
        };
        assert_eq!(exact_reference.resolved_point_light_exact_threshold(), 16);
    }

    #[test]
    fn point_shadow_proxy_scale_is_finite_and_bounded() {
        let negative = RenderQualityConfig {
            point_shadow_proxy_sway_scale: -1.0,
            ..default()
        };
        assert_eq!(negative.resolved_point_shadow_proxy_sway_scale(), 0.0);

        let oversized = RenderQualityConfig {
            point_shadow_proxy_sway_scale: 99.0,
            ..default()
        };
        assert_eq!(oversized.resolved_point_shadow_proxy_sway_scale(), 4.0);

        let invalid = RenderQualityConfig {
            point_shadow_proxy_sway_scale: f32::NAN,
            ..default()
        };
        assert_eq!(invalid.resolved_point_shadow_proxy_sway_scale(), 1.0);
    }

    #[test]
    fn slow_shadow_crossfade_requires_the_complete_cached_shader_path() {
        let enabled = RenderQualityConfig::default();
        assert!(enabled.slow_moving_shadow_crossfade_enabled());
        assert_eq!(enabled.resolved_slow_moving_shadow_transition_slots(), 4);

        let no_cache = RenderQualityConfig {
            persistent_point_shadow_cache: false,
            ..enabled
        };
        assert!(!no_cache.slow_moving_shadow_crossfade_enabled());

        let no_slots = RenderQualityConfig {
            slow_moving_shadow_transition_slots: 0,
            ..enabled
        };
        assert!(!no_slots.slow_moving_shadow_crossfade_enabled());

        let clamped = RenderQualityConfig {
            slow_moving_shadow_transition_slots: 99,
            slow_moving_shadow_snapshot_hz: f32::INFINITY,
            slow_moving_shadow_snapshot_distance_m: -1.0,
            slow_moving_shadow_crossfade_seconds: f32::NAN,
            ..enabled
        };
        assert_eq!(clamped.resolved_slow_moving_shadow_transition_slots(), 16);
        assert_eq!(clamped.resolved_slow_moving_shadow_snapshot_hz(), 8.0);
        assert_eq!(
            clamped.resolved_slow_moving_shadow_snapshot_distance_m(),
            0.001
        );
        assert_eq!(
            clamped.resolved_slow_moving_shadow_crossfade_seconds(),
            0.12
        );
    }
}
