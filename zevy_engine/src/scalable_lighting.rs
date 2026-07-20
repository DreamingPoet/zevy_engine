use bevy::{asset::Assets, pbr::PBR_FUNCTIONS_HANDLE, prelude::*, render::render_resource::Shader};

use crate::config::RenderQualityConfig;

const PBR_FUNCTIONS_TEMPLATE: &str = include_str!("shaders/zevy_pbr_functions.wgsl");
const HERO_SAMPLES_TEMPLATE: &str = "const ZEVY_POINT_LIGHT_HERO_SAMPLES: u32 = 2u;";
const TAIL_SAMPLES_TEMPLATE: &str = "const ZEVY_POINT_LIGHT_TAIL_SAMPLES: u32 = 2u;";
const TEMPORAL_SAMPLING_TEMPLATE: &str = "const ZEVY_TEMPORAL_LIGHT_SAMPLING: bool = false;";
const SAMPLE_PERIOD_TEMPLATE: &str = "const ZEVY_LIGHT_SAMPLE_PERIOD_FRAMES: u32 = 4u;";
const DYNAMIC_SHADOW_OVERLAY_TEMPLATE: &str = "const ZEVY_DYNAMIC_SHADOW_OVERLAY: bool = true;";
const POINT_LIGHT_DIRECT_TEMPLATE: &str = "const ZEVY_POINT_LIGHT_DIRECT_LIGHTING: bool = true;";

/// Installs Zevy's fixed-budget local-lighting experiment in place of Bevy's
/// stock StandardMaterial PBR lighting function. Disabling the corresponding
/// quality setting leaves Bevy's original shader asset untouched.
pub(crate) struct ScalableLightingPlugin;

impl Plugin for ScalableLightingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, install_scalable_pbr_shader);
    }
}

fn install_scalable_pbr_shader(
    quality: Res<RenderQualityConfig>,
    mut shaders: ResMut<Assets<Shader>>,
) {
    if !quality.scalable_point_lighting {
        info!("Zevy scalable point lighting disabled; using Bevy's deterministic PBR loop");
        return;
    }

    let hero_samples = quality.resolved_point_light_hero_samples();
    let tail_samples = quality.resolved_point_light_tail_samples();
    let sample_period = quality.resolved_light_sample_period_frames();
    let dynamic_shadow_overlay = quality.point_light_shadows
        && quality.persistent_point_shadow_cache
        && quality.dynamic_shadow_caster_overlay;
    let source = PBR_FUNCTIONS_TEMPLATE
        .replace(
            HERO_SAMPLES_TEMPLATE,
            &format!("const ZEVY_POINT_LIGHT_HERO_SAMPLES: u32 = {hero_samples}u;"),
        )
        .replace(
            TAIL_SAMPLES_TEMPLATE,
            &format!("const ZEVY_POINT_LIGHT_TAIL_SAMPLES: u32 = {tail_samples}u;"),
        )
        .replace(
            TEMPORAL_SAMPLING_TEMPLATE,
            &format!(
                "const ZEVY_TEMPORAL_LIGHT_SAMPLING: bool = {};",
                quality.temporal_point_light_sampling
            ),
        )
        .replace(
            SAMPLE_PERIOD_TEMPLATE,
            &format!("const ZEVY_LIGHT_SAMPLE_PERIOD_FRAMES: u32 = {sample_period}u;"),
        )
        .replace(
            DYNAMIC_SHADOW_OVERLAY_TEMPLATE,
            &format!("const ZEVY_DYNAMIC_SHADOW_OVERLAY: bool = {dynamic_shadow_overlay};"),
        )
        .replace(
            POINT_LIGHT_DIRECT_TEMPLATE,
            &format!(
                "const ZEVY_POINT_LIGHT_DIRECT_LIGHTING: bool = {};",
                quality.point_light_direct_lighting
            ),
        );

    debug_assert!(source.contains(&format!(
        "const ZEVY_POINT_LIGHT_HERO_SAMPLES: u32 = {hero_samples}u;"
    )));
    debug_assert!(source.contains(&format!(
        "const ZEVY_POINT_LIGHT_TAIL_SAMPLES: u32 = {tail_samples}u;"
    )));
    debug_assert!(source.contains(&format!(
        "const ZEVY_TEMPORAL_LIGHT_SAMPLING: bool = {};",
        quality.temporal_point_light_sampling
    )));
    debug_assert!(source.contains(&format!(
        "const ZEVY_LIGHT_SAMPLE_PERIOD_FRAMES: u32 = {sample_period}u;"
    )));
    debug_assert!(source.contains(&format!(
        "const ZEVY_DYNAMIC_SHADOW_OVERLAY: bool = {dynamic_shadow_overlay};"
    )));
    debug_assert!(source.contains(&format!(
        "const ZEVY_POINT_LIGHT_DIRECT_LIGHTING: bool = {};",
        quality.point_light_direct_lighting
    )));
    shaders.insert(
        PBR_FUNCTIONS_HANDLE.id(),
        Shader::from_wgsl(source, "zevy://shaders/zevy_pbr_functions.wgsl"),
    );

    info!(
        "Installed Zevy scalable point lighting: direct shading {}, {} highest-contribution Hero lights + {} importance-sampled shadowed tail lights per shading point, {}, dynamic shadow overlay {}",
        if quality.point_light_direct_lighting {
            "on"
        } else {
            "compiled out for fixed A/B"
        },
        hero_samples,
        tail_samples,
        if quality.temporal_point_light_sampling {
            format!("selection rotates every {sample_period} frames")
        } else {
            "selection is anchored in world space".to_owned()
        },
        if dynamic_shadow_overlay { "on" } else { "off" },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_template_contains_runtime_replacement_markers() {
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(TAIL_SAMPLES_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(HERO_SAMPLES_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(TEMPORAL_SAMPLING_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(SAMPLE_PERIOD_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(DYNAMIC_SHADOW_OVERLAY_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(POINT_LIGHT_DIRECT_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("zevy_point_light_importance"));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("zevy_fetch_point_shadow_combined"));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("static_visibility * dynamic_visibility"));
    }
}
