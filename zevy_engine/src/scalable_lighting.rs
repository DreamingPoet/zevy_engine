use bevy::{asset::Assets, pbr::PBR_FUNCTIONS_HANDLE, prelude::*, render::render_resource::Shader};

use crate::config::RenderQualityConfig;

const PBR_FUNCTIONS_TEMPLATE: &str = include_str!("shaders/zevy_pbr_functions.wgsl");
const HERO_SAMPLES_TEMPLATE: &str = "const ZEVY_POINT_LIGHT_HERO_SAMPLES: u32 = 2u;";
const TAIL_SAMPLES_TEMPLATE: &str = "const ZEVY_POINT_LIGHT_TAIL_SAMPLES: u32 = 2u;";
const EXACT_LIGHT_THRESHOLD_TEMPLATE: &str = "const ZEVY_POINT_LIGHT_EXACT_THRESHOLD: u32 = 8u;";
const TEMPORAL_SAMPLING_TEMPLATE: &str = "const ZEVY_TEMPORAL_LIGHT_SAMPLING: bool = false;";
const SAMPLE_PERIOD_TEMPLATE: &str = "const ZEVY_LIGHT_SAMPLE_PERIOD_FRAMES: u32 = 4u;";
const DYNAMIC_SHADOW_OVERLAY_TEMPLATE: &str = "const ZEVY_DYNAMIC_SHADOW_OVERLAY: bool = true;";
const POINT_LIGHT_DIRECT_TEMPLATE: &str = "const ZEVY_POINT_LIGHT_DIRECT_LIGHTING: bool = true;";
const CLUSTERED_LIGHT_PRESELECTION_TEMPLATE: &str =
    "const ZEVY_CLUSTERED_LIGHT_PRESELECTION: bool = true;";
const WORLD_SPACE_LIGHT_RESERVOIR_TEMPLATE: &str =
    "const ZEVY_WORLD_SPACE_LIGHT_RESERVOIR: bool = true;";

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
    let exact_light_threshold = quality.resolved_point_light_exact_threshold();
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
            EXACT_LIGHT_THRESHOLD_TEMPLATE,
            &format!("const ZEVY_POINT_LIGHT_EXACT_THRESHOLD: u32 = {exact_light_threshold}u;"),
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
        )
        .replace(
            CLUSTERED_LIGHT_PRESELECTION_TEMPLATE,
            &format!(
                "const ZEVY_CLUSTERED_LIGHT_PRESELECTION: bool = {};",
                quality.clustered_light_preselection
            ),
        )
        .replace(
            WORLD_SPACE_LIGHT_RESERVOIR_TEMPLATE,
            &format!(
                "const ZEVY_WORLD_SPACE_LIGHT_RESERVOIR: bool = {};",
                quality.world_space_light_reservoir
            ),
        );

    debug_assert!(source.contains(&format!(
        "const ZEVY_POINT_LIGHT_HERO_SAMPLES: u32 = {hero_samples}u;"
    )));
    debug_assert!(source.contains(&format!(
        "const ZEVY_POINT_LIGHT_TAIL_SAMPLES: u32 = {tail_samples}u;"
    )));
    debug_assert!(source.contains(&format!(
        "const ZEVY_POINT_LIGHT_EXACT_THRESHOLD: u32 = {exact_light_threshold}u;"
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
    debug_assert!(source.contains(&format!(
        "const ZEVY_CLUSTERED_LIGHT_PRESELECTION: bool = {};",
        quality.clustered_light_preselection
    )));
    debug_assert!(source.contains(&format!(
        "const ZEVY_WORLD_SPACE_LIGHT_RESERVOIR: bool = {};",
        quality.world_space_light_reservoir
    )));
    shaders.insert(
        PBR_FUNCTIONS_HANDLE.id(),
        Shader::from_wgsl(source, "zevy://shaders/zevy_pbr_functions.wgsl"),
    );

    info!(
        "Installed Zevy scalable point lighting: direct shading {}, {} highest-contribution Hero lights + {} importance-sampled shadowed tail lights per shading point, exact through {} local lights, {}, selection {}, dynamic shadow overlay {}",
        if quality.point_light_direct_lighting {
            "on"
        } else {
            "compiled out for fixed A/B"
        },
        hero_samples,
        tail_samples,
        exact_light_threshold,
        if quality.temporal_point_light_sampling {
            format!("selection rotates every {sample_period} frames")
        } else {
            "selection is anchored in world space".to_owned()
        },
        quality.point_light_selection_mode_label(),
        if dynamic_shadow_overlay { "on" } else { "off" },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_template_contains_runtime_replacement_markers() {
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(TAIL_SAMPLES_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(EXACT_LIGHT_THRESHOLD_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(HERO_SAMPLES_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(TEMPORAL_SAMPLING_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(SAMPLE_PERIOD_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(DYNAMIC_SHADOW_OVERLAY_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(POINT_LIGHT_DIRECT_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(CLUSTERED_LIGHT_PRESELECTION_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains(WORLD_SPACE_LIGHT_RESERVOIR_TEMPLATE));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("zevy_point_light_importance"));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("zevy_reservoir_seed"));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("zevy_reservoir_random_pair"));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("zevy_fetch_point_shadow_combined"));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("zevy_point_shadow_map_jitter"));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("POINT_LIGHT_FLAGS_SHADOW_MAP_JITTER_BIT"));
        assert!(PBR_FUNCTIONS_TEMPLATE.contains("static_visibility * dynamic_visibility"));
    }

    #[test]
    fn exact_cluster_path_precedes_every_approximate_selection_path() {
        let exact = PBR_FUNCTIONS_TEMPLATE
            .find("if (point_light_count <= exact_evaluation_threshold)")
            .expect("exact PointLight branch must exist");
        let world_reservoir = PBR_FUNCTIONS_TEMPLATE
            .find("else if (ZEVY_WORLD_SPACE_LIGHT_RESERVOIR")
            .expect("world-space reservoir branch must exist");
        let screen_supercluster = PBR_FUNCTIONS_TEMPLATE
            .find("else if (ZEVY_CLUSTERED_LIGHT_PRESELECTION")
            .expect("aggressive supercluster A/B branch must exist");

        assert!(exact < world_reservoir);
        assert!(world_reservoir < screen_supercluster);
    }
}
