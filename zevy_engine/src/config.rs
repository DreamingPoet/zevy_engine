use bevy::{prelude::*, render::view::Msaa};

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
    /// Supported values are 1 (off), 2, 4, and 8 samples.
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
}

impl Default for RenderQualityConfig {
    fn default() -> Self {
        Self {
            xr_render_scale: 0.8,
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
            point_light_hero_samples: 2,
            point_light_tail_samples: 2,
            temporal_point_light_sampling: false,
            light_sample_period_frames: 4,
            persistent_point_shadow_cache: true,
            dynamic_shadow_caster_overlay: true,
            point_shadow_cache_warmup_frames: 3,
            cached_point_shadow_update_hz: 8.0,
            max_cached_point_shadow_updates_per_frame: 2,
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

    pub(crate) fn resolved_msaa(self) -> Msaa {
        match self.msaa_samples {
            1 => Msaa::Off,
            2 => Msaa::Sample2,
            4 => Msaa::Sample4,
            8 => Msaa::Sample8,
            _ => Msaa::Sample2,
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

    pub(crate) fn resolved_light_sample_period_frames(self) -> u32 {
        self.light_sample_period_frames.clamp(1, 120)
    }

    pub(crate) fn resolved_point_shadow_cache_warmup_frames(self) -> u8 {
        self.point_shadow_cache_warmup_frames.clamp(1, 30)
    }

    pub(crate) fn resolved_cached_point_shadow_update_hz(self) -> f32 {
        if self.cached_point_shadow_update_hz.is_finite() {
            self.cached_point_shadow_update_hz.clamp(0.0, 30.0)
        } else {
            8.0
        }
    }
}

pub(crate) fn apply_render_quality_to_cameras(
    config: Res<RenderQualityConfig>,
    mut cameras: Query<&mut Msaa, With<Camera3d>>,
) {
    let configured_msaa = config.resolved_msaa();
    for mut msaa in &mut cameras {
        if *msaa != configured_msaa {
            *msaa = configured_msaa;
        }
    }
}

pub(crate) fn log_render_quality_config(config: Res<RenderQualityConfig>) {
    let resolved_scale = config.resolved_xr_render_scale();
    let resolved_msaa = config.resolved_msaa();
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

    if config.msaa_samples != resolved_msaa.samples() {
        warn!(
            "Unsupported RenderQualityConfig.msaa_samples={}; using {}x",
            config.msaa_samples,
            resolved_msaa.samples()
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

    info!(
        "Render quality: XR scale {:.2}, MSAA {}x, A/B {}, point shadow residency {} at {}px, clusters {}/{}z to {:.1}m, scalable point lights {} ({} Hero + {} tail samples, {}), persistent shadow cache {} + dynamic overlay {} ({} Hz, {} light/frame)",
        resolved_scale,
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
        if config.temporal_point_light_sampling {
            format!(
                "{} frame temporal rotation",
                config.resolved_light_sample_period_frames()
            )
        } else {
            "world-stable sampling".to_owned()
        },
        if config.persistent_point_shadow_cache {
            "on"
        } else {
            "off"
        },
        if dynamic_overlay_enabled { "on" } else { "off" },
        config.resolved_cached_point_shadow_update_hz(),
        config.max_cached_point_shadow_updates_per_frame,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_point_shadow_residency_tracks_the_imported_light_count() {
        let quality = RenderQualityConfig::default();
        assert_eq!(quality.max_shadowed_point_lights, 0);
        assert_eq!(quality.resolved_point_shadow_resident_count(16), 16);
        assert!(quality.point_light_direct_lighting);
        assert!(quality.point_light_shadows);
        assert!(quality.dynamic_shadow_caster_overlay);
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
}
