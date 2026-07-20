use std::{
    collections::{HashMap, HashSet},
    env,
    fmt::Write as _,
    time::Duration,
};

use bevy::{
    diagnostic::{
        DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
        SystemInformationDiagnosticsPlugin,
    },
    pbr::NotShadowCaster,
    prelude::*,
    render::{
        RenderApp, RenderPlugin,
        alpha::AlphaMode,
        diagnostic::RenderDiagnosticsPlugin,
        render_resource::{PrimitiveTopology, WgpuFeatures},
        settings::{RenderCreation, WgpuSettings},
        view::{Msaa, ViewVisibility},
    },
    ui::UiTargetCamera,
};
use bevy_mod_openxr::resources::OxrCurrentSessionConfig;
use bevy_mod_xr::camera::XrCamera;
use bevy_xr_utils::xr_utils_actions::XRUtilsActionState;

use crate::{
    app::{LaunchMode, StartupMode},
    config::RenderQualityConfig,
    input::{XrDebugHudPageAction, XrDebugHudToggleAction},
    scene::MirrorCamera,
    shadow_cache::ZevyShadowCacheFrame,
};

const HUD_UPDATE_INTERVAL_SECONDS: f32 = 0.25;
const PASS_SAMPLE_WINDOW: Duration = Duration::from_millis(750);
const MAX_PASS_ROWS: usize = 12;
const HUD_PANEL_WIDTH: f32 = 280.0;
const HUD_PANEL_PADDING: f32 = 2.2;
const HUD_FONT_SIZE: f32 = 7.0;
const HUD_TEXT_SHADOW_OFFSET: f32 = 0.1;

pub(crate) fn desktop_render_plugin() -> RenderPlugin {
    let mut settings = WgpuSettings::default();
    settings.features |= WgpuFeatures::TIMESTAMP_QUERY
        | WgpuFeatures::TIMESTAMP_QUERY_INSIDE_ENCODERS
        | WgpuFeatures::TIMESTAMP_QUERY_INSIDE_PASSES
        | WgpuFeatures::PIPELINE_STATISTICS_QUERY;

    RenderPlugin {
        render_creation: RenderCreation::Automatic(settings),
        ..default()
    }
}

pub(crate) struct RenderDebugPlugin;

impl Plugin for RenderDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin,
            SystemInformationDiagnosticsPlugin,
            RenderDiagnosticsPlugin,
        ))
        .insert_resource(RenderDebugState::from_args())
        .init_resource::<RenderDebugSnapshot>()
        .init_resource::<GpuDiagnosticSupport>()
        .add_systems(
            Update,
            (
                handle_debug_hud_input,
                ensure_debug_hud_for_cameras,
                update_debug_snapshot,
                apply_debug_hud_state,
            )
                .chain(),
        );
    }

    fn finish(&self, app: &mut App) {
        let support = app
            .get_sub_app(RenderApp)
            .map(|render_app| {
                let device = render_app
                    .world()
                    .resource::<bevy::render::renderer::RenderDevice>();
                let adapter = render_app
                    .world()
                    .resource::<bevy::render::renderer::RenderAdapterInfo>();
                let features = device.features();

                GpuDiagnosticSupport {
                    adapter: adapter.name.clone(),
                    backend: format!("{:?}", adapter.backend),
                    timestamp_query: features.contains(WgpuFeatures::TIMESTAMP_QUERY),
                    timestamp_inside_passes: features
                        .contains(WgpuFeatures::TIMESTAMP_QUERY_INSIDE_PASSES),
                    pipeline_statistics: features.contains(WgpuFeatures::PIPELINE_STATISTICS_QUERY),
                }
            })
            .unwrap_or_default();

        app.world_mut().insert_resource(support);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RenderDebugPage {
    #[default]
    Overview,
    Passes,
    Materials,
}

impl RenderDebugPage {
    fn next(self) -> Self {
        match self {
            Self::Overview => Self::Passes,
            Self::Passes => Self::Materials,
            Self::Materials => Self::Overview,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Overview => "OVERVIEW",
            Self::Passes => "GPU / RENDER PASSES",
            Self::Materials => "MATERIALS / LIGHTS",
        }
    }
}

#[derive(Resource)]
struct RenderDebugState {
    visible: bool,
    page: RenderDebugPage,
}

impl RenderDebugState {
    fn from_args() -> Self {
        let mut visible = true;
        let mut page = RenderDebugPage::Overview;

        for argument in env::args().skip(1) {
            match argument.as_str() {
                "--debug-hud" | "--debug-hud=on" => visible = true,
                "--no-debug-hud" | "--debug-hud=off" => visible = false,
                "--debug-hud-page=passes" => page = RenderDebugPage::Passes,
                "--debug-hud-page=materials" => page = RenderDebugPage::Materials,
                _ => {}
            }
        }

        Self { visible, page }
    }
}

#[derive(Resource, Default)]
struct RenderDebugSnapshot {
    overview: String,
    passes: String,
    materials: String,
    elapsed_since_update: f32,
}

impl RenderDebugSnapshot {
    fn page_text(&self, page: RenderDebugPage) -> &str {
        match page {
            RenderDebugPage::Overview => &self.overview,
            RenderDebugPage::Passes => &self.passes,
            RenderDebugPage::Materials => &self.materials,
        }
    }
}

#[derive(Resource, Default)]
struct GpuDiagnosticSupport {
    adapter: String,
    backend: String,
    timestamp_query: bool,
    timestamp_inside_passes: bool,
    pipeline_statistics: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct RenderTargetStats {
    resolution_per_view: Option<UVec2>,
    view_count: u32,
    total_target_pixels: u64,
    actual_msaa_samples: u32,
    xr_active: bool,
}

#[derive(Component)]
struct RenderDebugHudRoot {
    target_camera: Entity,
}

#[derive(Component)]
struct RenderDebugHudText;

fn handle_debug_hud_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    toggle_actions: Query<&XRUtilsActionState, With<XrDebugHudToggleAction>>,
    page_actions: Query<&XRUtilsActionState, With<XrDebugHudPageAction>>,
    mut state: ResMut<RenderDebugState>,
) {
    let xr_toggle = xr_action_just_pressed(&toggle_actions);
    let xr_page = xr_action_just_pressed(&page_actions);

    if keyboard.just_pressed(KeyCode::F3) || xr_toggle {
        state.visible = !state.visible;
    }
    if keyboard.just_pressed(KeyCode::F4) || xr_page {
        state.page = state.page.next();
    }
}

fn xr_action_just_pressed(
    actions: &Query<&XRUtilsActionState, impl bevy::ecs::query::QueryFilter>,
) -> bool {
    actions.iter().any(|state| {
        matches!(
            state,
            XRUtilsActionState::Bool(button)
                if button.is_active
                    && button.changed_since_last_sync
                    && button.current_state
        )
    })
}

fn ensure_debug_hud_for_cameras(
    mut commands: Commands,
    startup_mode: Res<StartupMode>,
    state: Res<RenderDebugState>,
    snapshot: Res<RenderDebugSnapshot>,
    cameras: Query<(Entity, &Camera, Option<&XrCamera>, Option<&MirrorCamera>), With<Camera3d>>,
    roots: Query<&RenderDebugHudRoot>,
) {
    let existing_targets = roots
        .iter()
        .map(|root| root.target_camera)
        .collect::<HashSet<_>>();

    for (camera_entity, camera, xr_camera, mirror_camera) in &cameras {
        if !camera.is_active || existing_targets.contains(&camera_entity) {
            continue;
        }

        let should_target = match startup_mode.0 {
            LaunchMode::Desktop => xr_camera.is_none(),
            LaunchMode::Xr => xr_camera.is_some() || mirror_camera.is_some(),
        };
        if !should_target {
            continue;
        }

        commands
            .spawn((
                Name::new(format!("RenderDebugHud({camera_entity:?})")),
                RenderDebugHudRoot {
                    target_camera: camera_entity,
                },
                UiTargetCamera(camera_entity),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::ZERO,
                    left: Val::ZERO,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    display: if state.visible {
                        Display::Flex
                    } else {
                        Display::None
                    },
                    ..default()
                },
                GlobalZIndex(10_000),
            ))
            .with_children(|root| {
                root.spawn((
                    Name::new("RenderDebugPanel"),
                    Node {
                        width: Val::Px(HUD_PANEL_WIDTH),
                        padding: UiRect::all(Val::Px(HUD_PANEL_PADDING)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.012, 0.018, 0.025, 0.88)),
                ))
                .with_child((
                    RenderDebugHudText,
                    Text::new(snapshot.page_text(state.page)),
                    TextFont {
                        font_size: HUD_FONT_SIZE,
                        ..default()
                    },
                    TextColor(Color::srgb(0.82, 0.94, 1.0)),
                    TextShadow {
                        offset: Vec2::splat(HUD_TEXT_SHADOW_OFFSET),
                        color: Color::BLACK,
                    },
                ));
            });
    }
}

fn apply_debug_hud_state(
    state: Res<RenderDebugState>,
    snapshot: Res<RenderDebugSnapshot>,
    mut roots: Query<&mut Node, With<RenderDebugHudRoot>>,
    mut texts: Query<&mut Text, With<RenderDebugHudText>>,
) {
    for mut root in &mut roots {
        root.display = if state.visible {
            Display::Flex
        } else {
            Display::None
        };
    }

    if state.is_changed() || snapshot.is_changed() {
        for mut text in &mut texts {
            text.0.clear();
            text.0.push_str(snapshot.page_text(state.page));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_debug_snapshot(
    time: Res<Time>,
    state: Res<RenderDebugState>,
    diagnostics: Res<DiagnosticsStore>,
    support: Res<GpuDiagnosticSupport>,
    quality: Res<RenderQualityConfig>,
    shadow_cache_frame: Res<ZevyShadowCacheFrame>,
    xr_session_config: Option<Res<OxrCurrentSessionConfig>>,
    camera_views: Query<(&Camera, &Msaa, Option<&XrCamera>), With<Camera3d>>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    renderables: Query<(
        &Mesh3d,
        &MeshMaterial3d<StandardMaterial>,
        &ViewVisibility,
        Option<&NotShadowCaster>,
    )>,
    point_lights: Query<&PointLight>,
    spot_lights: Query<&SpotLight>,
    directional_lights: Query<&DirectionalLight>,
    mut snapshot: ResMut<RenderDebugSnapshot>,
) {
    snapshot.elapsed_since_update += time.delta_secs();
    if !state.visible && !snapshot.overview.is_empty() {
        return;
    }
    if snapshot.elapsed_since_update < HUD_UPDATE_INTERVAL_SECONDS && !snapshot.overview.is_empty()
    {
        return;
    }
    snapshot.elapsed_since_update = 0.0;

    let fps = diagnostic_value(&diagnostics, &FrameTimeDiagnosticsPlugin::FPS).unwrap_or(0.0);
    let frame_ms = diagnostic_value(&diagnostics, &FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .unwrap_or_else(|| if fps > 0.0 { 1_000.0 / fps } else { 0.0 });
    let entity_count = diagnostic_value(&diagnostics, &EntityCountDiagnosticsPlugin::ENTITY_COUNT)
        .unwrap_or(0.0) as u64;
    let process_cpu = diagnostic_value(
        &diagnostics,
        &SystemInformationDiagnosticsPlugin::PROCESS_CPU_USAGE,
    );
    let process_memory = diagnostic_value(
        &diagnostics,
        &SystemInformationDiagnosticsPlugin::PROCESS_MEM_USAGE,
    );

    let scene = collect_scene_stats(
        &meshes,
        &materials,
        &images,
        &renderables,
        &point_lights,
        &spot_lights,
        &directional_lights,
    );

    let pass_metric = if support.timestamp_query && support.timestamp_inside_passes {
        "elapsed_gpu"
    } else {
        "elapsed_cpu"
    };
    let pass_rows = collect_render_metric_rows(&diagnostics, pass_metric, fps);
    let total_pass_ms = pass_rows.iter().map(|row| row.value_per_frame).sum::<f64>();
    let gpu_primitives = collect_render_metric_total(&diagnostics, "clipper_primitives_out", fps);
    let fragment_invocations =
        collect_render_metric_total(&diagnostics, "fragment_shader_invocations", fps);
    let target_stats = collect_render_target_stats(
        &camera_views,
        xr_session_config.as_deref(),
        quality.resolved_msaa().samples(),
    );
    let shadow_cache = shadow_cache_frame.telemetry();
    let dynamic_overlay_enabled = quality.persistent_point_shadow_cache
        && quality.scalable_point_lighting
        && quality.dynamic_shadow_caster_overlay;
    let point_shadow_policy = if quality.max_shadowed_point_lights == 0 {
        "all level-enabled lights".to_owned()
    } else {
        format!("cap {} lights", quality.max_shadowed_point_lights)
    };

    let top_pass = pass_rows.first();
    let using_gpu_timestamps = pass_metric == "elapsed_gpu";
    let bottleneck = bottleneck_hint(
        frame_ms,
        &scene,
        top_pass,
        total_pass_ms,
        using_gpu_timestamps,
    );

    let mut overview = String::with_capacity(1_800);
    let _ = writeln!(
        overview,
        "ZEVY RENDER DEBUG  |  {}",
        RenderDebugPage::Overview.label()
    );
    let _ = writeln!(overview, "F3 / Right A: hide    F4 / Right B: page");
    let _ = writeln!(
        overview,
        "------------------------------------------------------------"
    );
    let _ = writeln!(
        overview,
        "FPS {:>6.1}    Frame {:>6.2} ms    Entities {}",
        fps, frame_ms, entity_count
    );
    if let Some(cpu) = process_cpu {
        let _ = write!(overview, "Process CPU {:>5.1}%", cpu);
    }
    if let Some(memory) = process_memory {
        let _ = write!(overview, "    Process RAM {:>7.2} GiB", memory);
    }
    let _ = writeln!(overview);
    let _ = writeln!(
        overview,
        "XR scale config       {:>8.2}",
        quality.resolved_xr_render_scale()
    );
    let _ = writeln!(
        overview,
        "Clusters          {:>4} / {:>2}z / {:>4.0}m",
        quality.resolved_cluster_total(),
        quality.resolved_cluster_z_slices(),
        quality.resolved_cluster_far_z_m(),
    );
    let _ = writeln!(
        overview,
        "Point shadows     {:>4} resident / {:>4}px",
        scene.shadowed_point_lights,
        quality.resolved_point_shadow_map_size(),
    );
    let _ = writeln!(
        overview,
        "Shadow cache      {:>4} / draw {:>2} reuse {:>2}",
        if quality.persistent_point_shadow_cache {
            "ON"
        } else {
            "OFF"
        },
        shadow_cache.rendered_views,
        shadow_cache.reused_views,
    );
    let _ = writeln!(
        overview,
        "Dynamic overlay   {:>4} / caster {:>2} draw {:>2}",
        if dynamic_overlay_enabled { "ON" } else { "OFF" },
        shadow_cache.dynamic_casters,
        shadow_cache.dynamic_views_rendered,
    );
    let _ = writeln!(
        overview,
        "Scalable lights   {:>4} / {:>1}H+{:>1}T / {}",
        if quality.scalable_point_lighting {
            "ON"
        } else {
            "OFF"
        },
        quality.resolved_point_light_hero_samples(),
        quality.resolved_point_light_tail_samples(),
        if quality.temporal_point_light_sampling {
            format!("{}f", quality.resolved_light_sample_period_frames())
        } else {
            "stable".to_owned()
        },
    );
    if let Some(resolution) = target_stats.resolution_per_view {
        let resolution_label = if target_stats.xr_active {
            "Per-eye resolution"
        } else {
            "View resolution"
        };
        let _ = writeln!(
            overview,
            "{resolution_label:<20}{:>5} x {:<5}",
            resolution.x, resolution.y
        );
        let _ = writeln!(
            overview,
            "Views / actual MSAA {:>7} / {}x",
            target_stats.view_count, target_stats.actual_msaa_samples
        );
        let _ = writeln!(
            overview,
            "Target pixels/frame {:>10}",
            format_count(target_stats.total_target_pixels)
        );
    } else {
        let _ = writeln!(
            overview,
            "Actual MSAA          {:>9}x",
            target_stats.actual_msaa_samples
        );
    }
    let _ = writeln!(
        overview,
        "Visible meshes      {:>10}",
        scene.visible_mesh_entities
    );
    let _ = writeln!(overview, "Unique meshes       {:>10}", scene.unique_meshes);
    let _ = writeln!(
        overview,
        "Triangles / eye     {:>10}",
        format_count(scene.visible_triangles)
    );
    let _ = writeln!(
        overview,
        "Main draws est/eye  {:>10}",
        format_count(scene.estimated_main_draws)
    );
    let _ = writeln!(
        overview,
        "Materials visible   {:>10}",
        scene.unique_materials
    );
    let _ = writeln!(
        overview,
        "Shadow views est    {:>10}",
        scene.estimated_shadow_views
    );
    if let Some(primitives) = gpu_primitives {
        let _ = writeln!(
            overview,
            "GPU primitives/frame{:>10}",
            format_count(primitives as u64)
        );
    }
    if let Some(fragments) = fragment_invocations {
        let _ = writeln!(
            overview,
            "Fragment invocations{:>10}",
            format_count(fragments as u64)
        );
        if target_stats.total_target_pixels > 0 {
            let _ = writeln!(
                overview,
                "Fragment / target px{:>9.2}",
                fragments / target_stats.total_target_pixels as f64
            );
        }
    }
    let _ = writeln!(
        overview,
        "------------------------------------------------------------"
    );
    let _ = writeln!(overview, "Likely bottleneck: {bottleneck}");
    if let Some(row) = top_pass {
        let percentage = if total_pass_ms > 0.0 {
            row.value_per_frame / total_pass_ms * 100.0
        } else {
            0.0
        };
        let _ = writeln!(
            overview,
            "Top measured pass: {}  {:.2} ms ({:.0}%)",
            row.label, row.value_per_frame, percentage
        );
    }
    let _ = writeln!(
        overview,
        "Pass timing source: {}",
        if pass_metric == "elapsed_gpu" {
            "GPU timestamps"
        } else {
            "CPU command recording fallback"
        }
    );

    let mut passes = String::with_capacity(2_500);
    let _ = writeln!(
        passes,
        "ZEVY RENDER DEBUG  |  {}",
        RenderDebugPage::Passes.label()
    );
    let _ = writeln!(
        passes,
        "Adapter: {}  ({})",
        support.adapter, support.backend
    );
    let _ = writeln!(
        passes,
        "GPU timestamps: {}    Pipeline statistics: {}",
        yes_no(support.timestamp_query),
        yes_no(support.pipeline_statistics)
    );
    let _ = writeln!(
        passes,
        "Timing source: {}    Recorded total: {:.2} ms/frame",
        if pass_metric == "elapsed_gpu" {
            "GPU"
        } else {
            "CPU fallback"
        },
        total_pass_ms
    );
    let _ = writeln!(
        passes,
        "------------------------------------------------------------"
    );
    if pass_rows.is_empty() {
        let _ = writeln!(passes, "Waiting for render diagnostics...");
    } else {
        for row in pass_rows.iter().take(MAX_PASS_ROWS) {
            let percentage = if total_pass_ms > 0.0 {
                row.value_per_frame / total_pass_ms * 100.0
            } else {
                0.0
            };
            let bar = percentage_bar(percentage);
            let _ = writeln!(
                passes,
                "{:>6.2} ms  {:>5.1}%  {:<12} {}",
                row.value_per_frame, percentage, bar, row.label
            );
        }
    }
    let _ = writeln!(
        passes,
        "------------------------------------------------------------"
    );
    let _ = writeln!(
        passes,
        "Percentages cover Bevy instrumented spans and may overlap."
    );
    let _ = writeln!(
        passes,
        "Vulkan/DX12 provide GPU data; unsupported devices show CPU fallback."
    );

    let mut material_page = String::with_capacity(2_200);
    let _ = writeln!(
        material_page,
        "ZEVY RENDER DEBUG  |  {}",
        RenderDebugPage::Materials.label()
    );
    let _ = writeln!(
        material_page,
        "Visible material analysis (StandardMaterial)"
    );
    let _ = writeln!(
        material_page,
        "------------------------------------------------------------"
    );
    let _ = writeln!(
        material_page,
        "Unique materials        {:>8}",
        scene.unique_materials
    );
    let _ = writeln!(
        material_page,
        "Texture slots total     {:>8}",
        scene.material_texture_slots
    );
    let _ = writeln!(
        material_page,
        "Average slots/material  {:>8.2}",
        scene.average_texture_slots
    );
    let _ = writeln!(
        material_page,
        "4+ texture materials    {:>8}",
        scene.heavy_texture_materials
    );
    let _ = writeln!(
        material_page,
        "Alpha blended           {:>8}",
        scene.alpha_blended_materials
    );
    let _ = writeln!(
        material_page,
        "Alpha masked            {:>8}",
        scene.alpha_masked_materials
    );
    let _ = writeln!(
        material_page,
        "Double sided            {:>8}",
        scene.double_sided_materials
    );
    let _ = writeln!(
        material_page,
        "Normal mapped           {:>8}",
        scene.normal_mapped_materials
    );
    let _ = writeln!(
        material_page,
        "Transmission enabled    {:>8}",
        scene.transmission_materials
    );
    let _ = writeln!(
        material_page,
        "Unlit                   {:>8}",
        scene.unlit_materials
    );
    let _ = writeln!(
        material_page,
        "Loaded image CPU data   {:>8.1} MiB",
        scene.image_data_mib
    );
    let _ = writeln!(
        material_page,
        "------------------------------------------------------------"
    );
    let _ = writeln!(
        material_page,
        "Point / Spot / Dir lights   {} / {} / {}",
        scene.point_lights, scene.spot_lights, scene.directional_lights
    );
    let _ = writeln!(
        material_page,
        "Shadowed lights             {}",
        scene.shadowed_lights
    );
    let _ = writeln!(
        material_page,
        "Estimated shadow views      {}",
        scene.estimated_shadow_views
    );
    let _ = writeln!(
        material_page,
        "Point shadow residency      {}",
        point_shadow_policy
    );
    let _ = writeln!(
        material_page,
        "Point shadow face           {} x {} px",
        quality.resolved_point_shadow_map_size(),
        quality.resolved_point_shadow_map_size(),
    );
    let _ = writeln!(
        material_page,
        "Persistent shadow cache     {}",
        if quality.persistent_point_shadow_cache {
            "enabled"
        } else {
            "disabled"
        }
    );
    let _ = writeln!(
        material_page,
        "Dynamic caster overlay      {}",
        if dynamic_overlay_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    let _ = writeln!(
        material_page,
        "Dynamic casters/views       {} / {} redraw",
        shadow_cache.dynamic_casters, shadow_cache.dynamic_views_rendered,
    );
    let _ = writeln!(
        material_page,
        "Shadow views redraw/reuse   {} / {} of {}",
        shadow_cache.rendered_views, shadow_cache.reused_views, shadow_cache.resident_views,
    );
    let _ = writeln!(
        material_page,
        "Invalidated lights/frame    {}",
        shadow_cache.invalidated_lights,
    );
    let _ = writeln!(
        material_page,
        "Cached projection schedule  {:.1} Hz, <= {} light/frame (fair)",
        quality.resolved_cached_point_shadow_update_hz(),
        quality.max_cached_point_shadow_updates_per_frame,
    );
    let _ = writeln!(
        material_page,
        "Cluster grid budget         {} / {} z-slices",
        quality.resolved_cluster_total(),
        quality.resolved_cluster_z_slices(),
    );
    let _ = writeln!(
        material_page,
        "Scalable PointLight path    {}",
        if quality.scalable_point_lighting {
            "Hero + stochastic tail"
        } else {
            "Bevy deterministic"
        }
    );
    if quality.scalable_point_lighting {
        let hero_lights = quality.resolved_point_light_hero_samples() as usize;
        let tail_samples = quality.resolved_point_light_tail_samples() as usize;
        let bounded_evaluations = hero_lights.saturating_add(tail_samples);
        let _ = writeln!(
            material_page,
            "Point BRDF budget/pixel    <= {} ({} Hero + {} tail)",
            bounded_evaluations, hero_lights, tail_samples,
        );
        let _ = writeln!(
            material_page,
            "Tail sample mode           {}",
            if quality.temporal_point_light_sampling {
                format!(
                    "rotate every {} frames",
                    quality.resolved_light_sample_period_frames()
                )
            } else {
                "world-space stable".to_owned()
            },
        );
    }
    let _ = writeln!(
        material_page,
        "------------------------------------------------------------"
    );
    let _ = writeln!(
        material_page,
        "Risk flags are cost indicators, not an exact shader instruction count."
    );
    let _ = writeln!(
        material_page,
        "An UE-style Shader Complexity heatmap requires a replacement shader pass."
    );

    snapshot.overview = overview;
    snapshot.passes = passes;
    snapshot.materials = material_page;
}

fn collect_render_target_stats(
    cameras: &Query<(&Camera, &Msaa, Option<&XrCamera>), With<Camera3d>>,
    xr_session_config: Option<&OxrCurrentSessionConfig>,
    configured_msaa_samples: u32,
) -> RenderTargetStats {
    let mut xr_view_count = 0_u32;
    let mut xr_msaa_samples = None;

    for (camera, msaa, xr_camera) in cameras.iter() {
        if camera.is_active && xr_camera.is_some() {
            xr_view_count += 1;
            xr_msaa_samples.get_or_insert(msaa.samples());
        }
    }

    if xr_view_count > 0 {
        let resolution = xr_session_config.map(|config| config.resolution);
        return RenderTargetStats {
            resolution_per_view: resolution,
            view_count: xr_view_count,
            total_target_pixels: total_target_pixels(resolution, xr_view_count),
            actual_msaa_samples: xr_msaa_samples.unwrap_or(configured_msaa_samples),
            xr_active: true,
        };
    }

    let desktop_view = cameras
        .iter()
        .find(|(camera, _, _)| camera.is_active)
        .map(|(camera, msaa, _)| (camera.physical_target_size(), msaa.samples()));

    let (resolution, actual_msaa_samples, view_count) = desktop_view
        .map(|(resolution, msaa)| (resolution, msaa, 1))
        .unwrap_or((None, configured_msaa_samples, 0));

    RenderTargetStats {
        resolution_per_view: resolution,
        view_count,
        total_target_pixels: total_target_pixels(resolution, view_count),
        actual_msaa_samples,
        xr_active: false,
    }
}

fn total_target_pixels(resolution: Option<UVec2>, view_count: u32) -> u64 {
    resolution
        .map(|resolution| u64::from(resolution.x) * u64::from(resolution.y) * u64::from(view_count))
        .unwrap_or_default()
}

fn diagnostic_value(
    diagnostics: &DiagnosticsStore,
    path: &bevy::diagnostic::DiagnosticPath,
) -> Option<f64> {
    diagnostics
        .get(path)
        .and_then(|diagnostic| diagnostic.smoothed())
}

#[derive(Default)]
struct SceneRenderStats {
    visible_mesh_entities: u64,
    unique_meshes: usize,
    visible_triangles: u64,
    estimated_main_draws: u64,
    unique_materials: usize,
    material_texture_slots: u64,
    average_texture_slots: f64,
    heavy_texture_materials: u64,
    alpha_blended_materials: u64,
    alpha_masked_materials: u64,
    double_sided_materials: u64,
    normal_mapped_materials: u64,
    transmission_materials: u64,
    unlit_materials: u64,
    image_data_mib: f64,
    point_lights: usize,
    spot_lights: usize,
    directional_lights: usize,
    shadowed_point_lights: usize,
    shadowed_lights: usize,
    estimated_shadow_views: usize,
}

fn collect_scene_stats(
    meshes: &Assets<Mesh>,
    materials: &Assets<StandardMaterial>,
    images: &Assets<Image>,
    renderables: &Query<(
        &Mesh3d,
        &MeshMaterial3d<StandardMaterial>,
        &ViewVisibility,
        Option<&NotShadowCaster>,
    )>,
    point_lights: &Query<&PointLight>,
    spot_lights: &Query<&SpotLight>,
    directional_lights: &Query<&DirectionalLight>,
) -> SceneRenderStats {
    let mut stats = SceneRenderStats::default();
    let mut unique_meshes = HashSet::new();
    let mut unique_materials = HashSet::new();
    let mut opaque_draw_keys = HashSet::new();
    let mut transparent_draws = 0_u64;
    let mut material_usage = HashMap::new();

    for (mesh_handle, material_handle, view_visibility, _not_shadow_caster) in renderables.iter() {
        if !view_visibility.get() {
            continue;
        }

        stats.visible_mesh_entities += 1;
        let mesh_id = mesh_handle.0.id();
        let material_id = material_handle.0.id();
        unique_meshes.insert(mesh_id);
        unique_materials.insert(material_id);
        *material_usage.entry(material_id).or_insert(0_u64) += 1;

        if let Some(mesh) = meshes.get(mesh_id) {
            stats.visible_triangles += mesh_triangle_count(mesh);
        }

        let is_blended = materials.get(material_id).is_some_and(|material| {
            matches!(
                material.alpha_mode,
                AlphaMode::Blend | AlphaMode::Premultiplied | AlphaMode::Add | AlphaMode::Multiply
            )
        });
        if is_blended {
            transparent_draws += 1;
        } else {
            opaque_draw_keys.insert((mesh_id, material_id));
        }
    }

    stats.unique_meshes = unique_meshes.len();
    stats.unique_materials = unique_materials.len();
    stats.estimated_main_draws = opaque_draw_keys.len() as u64 + transparent_draws;

    for material_id in unique_materials {
        let Some(material) = materials.get(material_id) else {
            continue;
        };
        let texture_slots = standard_material_texture_slots(material);
        stats.material_texture_slots += texture_slots;
        stats.heavy_texture_materials += u64::from(texture_slots >= 4);
        stats.alpha_blended_materials += u64::from(matches!(
            material.alpha_mode,
            AlphaMode::Blend | AlphaMode::Premultiplied | AlphaMode::Add | AlphaMode::Multiply
        ));
        stats.alpha_masked_materials +=
            u64::from(matches!(material.alpha_mode, AlphaMode::Mask(_)));
        stats.double_sided_materials += u64::from(material.double_sided);
        stats.normal_mapped_materials += u64::from(material.normal_map_texture.is_some());
        stats.transmission_materials +=
            u64::from(material.diffuse_transmission > 0.0 || material.specular_transmission > 0.0);
        stats.unlit_materials += u64::from(material.unlit);
    }

    if stats.unique_materials > 0 {
        stats.average_texture_slots =
            stats.material_texture_slots as f64 / stats.unique_materials as f64;
    }
    stats.image_data_mib = images
        .iter()
        .filter_map(|(_, image)| image.data.as_ref())
        .map(Vec::len)
        .sum::<usize>() as f64
        / (1024.0 * 1024.0);

    stats.point_lights = point_lights.iter().count();
    stats.spot_lights = spot_lights.iter().count();
    stats.directional_lights = directional_lights.iter().count();

    let shadowed_points = point_lights
        .iter()
        .filter(|light| light.shadows_enabled)
        .count();
    let shadowed_spots = spot_lights
        .iter()
        .filter(|light| light.shadows_enabled)
        .count();
    let shadowed_directional = directional_lights
        .iter()
        .filter(|light| light.shadows_enabled)
        .count();
    stats.shadowed_point_lights = shadowed_points;
    stats.shadowed_lights = shadowed_points + shadowed_spots + shadowed_directional;
    stats.estimated_shadow_views = shadowed_points * 6 + shadowed_spots + shadowed_directional * 4;

    stats
}

fn mesh_triangle_count(mesh: &Mesh) -> u64 {
    let element_count = mesh
        .indices()
        .map(|indices| indices.len())
        .unwrap_or_else(|| mesh.count_vertices()) as u64;

    match mesh.primitive_topology() {
        PrimitiveTopology::TriangleList => element_count / 3,
        PrimitiveTopology::TriangleStrip => element_count.saturating_sub(2),
        _ => 0,
    }
}

fn standard_material_texture_slots(material: &StandardMaterial) -> u64 {
    [
        material.base_color_texture.is_some(),
        material.emissive_texture.is_some(),
        material.metallic_roughness_texture.is_some(),
        material.normal_map_texture.is_some(),
        material.occlusion_texture.is_some(),
        material.depth_map.is_some(),
    ]
    .into_iter()
    .map(u64::from)
    .sum()
}

struct RenderMetricRow {
    label: String,
    value_per_frame: f64,
}

fn collect_render_metric_rows(
    diagnostics: &DiagnosticsStore,
    metric: &str,
    fps: f64,
) -> Vec<RenderMetricRow> {
    let suffix = format!("/{metric}");
    let mut rows = diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let path = diagnostic.path().as_str();
            if !path.starts_with("render/") || !path.ends_with(&suffix) {
                return None;
            }

            let value_per_frame = diagnostic_total_per_frame(diagnostic, fps)?;
            Some(RenderMetricRow {
                label: clean_render_path(path, &suffix),
                value_per_frame,
            })
        })
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| right.value_per_frame.total_cmp(&left.value_per_frame));
    rows
}

fn collect_render_metric_total(
    diagnostics: &DiagnosticsStore,
    metric: &str,
    fps: f64,
) -> Option<f64> {
    let rows = collect_render_metric_rows(diagnostics, metric, fps);
    (!rows.is_empty()).then(|| rows.iter().map(|row| row.value_per_frame).sum())
}

fn diagnostic_total_per_frame(diagnostic: &bevy::diagnostic::Diagnostic, fps: f64) -> Option<f64> {
    let latest = diagnostic.measurement()?;
    let mut sample_count = 0_usize;
    let mut sum = 0.0;
    let mut oldest_time = latest.time;

    for measurement in diagnostic.measurements() {
        let age = latest.time.duration_since(measurement.time);
        if age > PASS_SAMPLE_WINDOW {
            continue;
        }
        sample_count += 1;
        sum += measurement.value;
        oldest_time = measurement.time;
    }

    if sample_count == 0 {
        return None;
    }
    let duration_seconds = latest.time.duration_since(oldest_time).as_secs_f64();
    if duration_seconds > 0.001 && fps > 1.0 {
        Some(sum / duration_seconds / fps)
    } else {
        Some(sum / sample_count as f64)
    }
}

fn clean_render_path(path: &str, suffix: &str) -> String {
    path.strip_prefix("render/")
        .unwrap_or(path)
        .strip_suffix(suffix)
        .unwrap_or(path)
        .replace('/', " > ")
}

fn bottleneck_hint(
    frame_ms: f64,
    scene: &SceneRenderStats,
    top_pass: Option<&RenderMetricRow>,
    total_pass_ms: f64,
    using_gpu_timestamps: bool,
) -> &'static str {
    if frame_ms <= 0.0 {
        return "collecting samples";
    }
    if using_gpu_timestamps && total_pass_ms > 0.0 && total_pass_ms < frame_ms * 0.55 {
        return "CPU, asset streaming or frame pacing; measured GPU passes are below frame time";
    }
    if let Some(top_pass) = top_pass {
        let share = if total_pass_ms > 0.0 {
            top_pass.value_per_frame / total_pass_ms
        } else {
            0.0
        };
        let label = top_pass.label.to_ascii_lowercase();
        if share >= 0.35 && label.contains("shadow") {
            return "GPU shadow rendering; reduce shadowed lights/range/casters";
        }
        if share >= 0.45 && (label.contains("opaque") || label.contains("transparent")) {
            return "main 3D pass; inspect fill-rate, materials and geometry";
        }
        if share >= 0.35 && (label.contains("bloom") || label.contains("tonemapping")) {
            return "post-processing / render resolution";
        }
    }
    if scene.alpha_blended_materials > 25 {
        return "transparent overdraw risk";
    }
    if scene.estimated_main_draws > 1_500 {
        return "draw submission / batching risk";
    }
    if scene.visible_triangles > 3_000_000 {
        return "geometry / vertex processing risk";
    }
    if frame_ms > 13.89 {
        return "below 72 Hz budget; inspect Passes page and CPU frame time";
    }
    if frame_ms > 11.11 {
        return "below 90 Hz budget; inspect Passes page";
    }
    "within common VR frame budgets; validate on target device"
}

fn percentage_bar(percentage: f64) -> String {
    let filled = ((percentage.clamp(0.0, 100.0) / 5.0).round() as usize).min(20);
    format!("{}{}", "#".repeat(filled), ".".repeat(20 - filled))
}

fn format_count(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    output
}

fn yes_no(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}
