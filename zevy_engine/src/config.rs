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
}

impl Default for RenderQualityConfig {
    fn default() -> Self {
        Self {
            xr_render_scale: 0.8,
            msaa_samples: 2,
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

    info!(
        "Render quality: XR scale {:.2}, MSAA {}x",
        resolved_scale,
        resolved_msaa.samples()
    );
}
