use std::{
    collections::{HashSet, VecDeque},
    env,
    fmt::Write as _,
    time::Duration,
};

use bevy::{
    diagnostic::{
        DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
        SystemInformationDiagnosticsPlugin,
    },
    ecs::system::SystemParam,
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
    clustered_light_preselection::ClusterLightPreselectionStats,
    config::RenderQualityConfig,
    input::{XrDebugHudPageAction, XrDebugHudToggleAction},
    scene::{ImportedZevyEntity, MirrorCamera},
    shadow_cache::{ZevyShadowCacheFrame, is_dynamic_shadow_caster},
    shadow_motion_policy::ShadowMotionPolicyTelemetry,
    shadow_overlay::DynamicShadowCaster,
};

const HUD_UPDATE_INTERVAL_SECONDS: f32 = 0.25;
const PASS_SAMPLE_WINDOW: Duration = Duration::from_millis(750);
const MAX_PASS_ROWS: usize = 12;
const HUD_PANEL_WIDTH: f32 = 320.0;
const HUD_PANEL_PADDING: f32 = 2.2;
const HUD_FONT_SIZE: f32 = 11.0;
const HUD_TEXT_SHADOW_OFFSET: f32 = 0.1;
const FRAME_HISTORY_SECONDS: f64 = 10.0;

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
    Workload,
    Passes,
    Materials,
}

impl RenderDebugPage {
    fn next(self) -> Self {
        match self {
            Self::Overview => Self::Workload,
            Self::Workload => Self::Passes,
            Self::Passes => Self::Materials,
            Self::Materials => Self::Overview,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Overview => "OVERVIEW",
            Self::Workload => "FULL-FRAME WORKLOAD",
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
                value if value.starts_with("--debug-hud-page=") => {
                    if let Some(selected) =
                        debug_page_from_label(value.trim_start_matches("--debug-hud-page="))
                    {
                        page = selected;
                    }
                }
                _ => {}
            }
        }

        // Android NativeActivity does not expose ordinary desktop argv. This
        // debug-only system property gives automated ADB captures the same page
        // selection without synthesizing controller input or changing the
        // renderer. Example: `adb shell setprop debug.zevy.hud_page workload`.
        #[cfg(target_os = "android")]
        if let Some(selected) = android_debug_hud_page_override() {
            page = selected;
        }

        Self { visible, page }
    }
}

fn debug_page_from_label(label: &str) -> Option<RenderDebugPage> {
    match label.trim().to_ascii_lowercase().as_str() {
        "overview" => Some(RenderDebugPage::Overview),
        "workload" => Some(RenderDebugPage::Workload),
        "passes" => Some(RenderDebugPage::Passes),
        "materials" => Some(RenderDebugPage::Materials),
        _ => None,
    }
}

#[cfg(target_os = "android")]
fn android_debug_hud_page_override() -> Option<RenderDebugPage> {
    android_system_property("debug.zevy.hud_page")
        .as_deref()
        .and_then(debug_page_from_label)
}

#[cfg(target_os = "android")]
pub(crate) fn apply_android_render_quality_overrides(config: &mut RenderQualityConfig) {
    if let Some(value) = android_system_property("debug.zevy.point_direct")
        .as_deref()
        .and_then(parse_debug_bool)
    {
        config.point_light_direct_lighting = value;
    }
    if let Some(value) = android_system_property("debug.zevy.point_shadows")
        .as_deref()
        .and_then(parse_debug_bool)
    {
        config.point_light_shadows = value;
    }
    if let Some(value) = android_system_property("debug.zevy.cluster_preselection")
        .as_deref()
        .and_then(parse_debug_bool)
    {
        config.clustered_light_preselection = value;
    }
    if let Some(value) = android_system_property("debug.zevy.world_reservoir")
        .as_deref()
        .and_then(parse_debug_bool)
    {
        config.world_space_light_reservoir = value;
    }
    if let Some(value) = android_system_property("debug.zevy.dynamic_overlay")
        .as_deref()
        .and_then(parse_debug_bool)
    {
        config.dynamic_shadow_caster_overlay = value;
    }
    if let Some(value) = android_system_property("debug.zevy.shadow_proxy")
        .as_deref()
        .and_then(parse_debug_bool)
    {
        config.continuous_point_shadow_proxy = value;
    }
    if let Some(value) = android_system_property("debug.zevy.shadow_proxy_scale")
        .as_deref()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite())
    {
        config.point_shadow_proxy_sway_scale = value;
    }
    if let Some(value) = android_system_property("debug.zevy.shadow_updates")
        .as_deref()
        .and_then(|value| value.trim().parse::<usize>().ok())
    {
        config.max_cached_point_shadow_updates_per_frame = value.min(64);
    }
    if let Some(value) = android_system_property("debug.zevy.shadow_hz")
        .as_deref()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite())
    {
        config.cached_point_shadow_update_hz = value;
    }
    if let Some(value) = android_system_property("debug.zevy.hero_samples")
        .as_deref()
        .and_then(|value| value.trim().parse::<u32>().ok())
    {
        config.point_light_hero_samples = value.clamp(1, 2);
    }
    if let Some(value) = android_system_property("debug.zevy.tail_samples")
        .as_deref()
        .and_then(|value| value.trim().parse::<u32>().ok())
    {
        config.point_light_tail_samples = value.min(8);
    }
    if let Some(value) = android_system_property("debug.zevy.exact_lights")
        .as_deref()
        .and_then(|value| value.trim().parse::<u32>().ok())
    {
        config.point_light_exact_threshold = value.min(64);
    }
}

#[cfg(any(target_os = "android", test))]
fn parse_debug_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "android")]
fn android_system_property(name: &str) -> Option<String> {
    use std::{
        ffi::{CStr, CString},
        os::raw::c_char,
    };

    const PROPERTY_VALUE_MAX: usize = 92;
    unsafe extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> i32;
    }

    let name = CString::new(name).ok()?;
    let mut value = [0 as c_char; PROPERTY_VALUE_MAX];
    // SAFETY: `name` is NUL-terminated and `value` is the Bionic-documented
    // PROPERTY_VALUE_MAX-sized writable output buffer.
    let length = unsafe { __system_property_get(name.as_ptr(), value.as_mut_ptr()) };
    if length <= 0 {
        return None;
    }
    // SAFETY: a successful __system_property_get call NUL-terminates `value`.
    Some(
        unsafe { CStr::from_ptr(value.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[derive(Resource, Default)]
struct RenderDebugSnapshot {
    overview: String,
    workload: String,
    passes: String,
    materials: String,
    elapsed_since_update: f32,
    frame_history: VecDeque<FrameTimeSample>,
}

impl RenderDebugSnapshot {
    fn page_text(&self, page: RenderDebugPage) -> &str {
        match page {
            RenderDebugPage::Overview => &self.overview,
            RenderDebugPage::Workload => &self.workload,
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

#[derive(SystemParam)]
struct RenderDebugSceneData<'w, 's> {
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<StandardMaterial>>,
    images: Res<'w, Assets<Image>>,
    renderables: Query<
        'w,
        's,
        (
            Entity,
            &'static Mesh3d,
            &'static MeshMaterial3d<StandardMaterial>,
            &'static ViewVisibility,
            Option<&'static NotShadowCaster>,
        ),
    >,
    dynamic_shadow_markers: Query<'w, 's, (), With<DynamicShadowCaster>>,
    imported_entities: Query<'w, 's, (), With<ImportedZevyEntity>>,
    parents: Query<'w, 's, &'static ChildOf>,
    point_lights: Query<'w, 's, &'static PointLight>,
    spot_lights: Query<'w, 's, &'static SpotLight>,
    directional_lights: Query<'w, 's, &'static DirectionalLight>,
}

#[derive(Clone, Copy, Debug)]
struct FrameTimeSample {
    timestamp_seconds: f64,
    frame_ms: f64,
}

#[derive(Clone, Copy, Debug)]
struct FrameTimePercentiles {
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    sample_count: usize,
    window_seconds: f64,
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
    cluster_preselection: Res<ClusterLightPreselectionStats>,
    shadow_cache_frame: Res<ZevyShadowCacheFrame>,
    motion_policy: Res<ShadowMotionPolicyTelemetry>,
    xr_session_config: Option<Res<OxrCurrentSessionConfig>>,
    camera_views: Query<(&Camera, &Msaa, Option<&XrCamera>), With<Camera3d>>,
    scene_data: RenderDebugSceneData,
    mut snapshot: ResMut<RenderDebugSnapshot>,
) {
    record_frame_time_sample(
        &mut snapshot.frame_history,
        time.elapsed().as_secs_f64(),
        time.delta().as_secs_f64() * 1_000.0,
    );
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
    let frame_percentiles = frame_time_percentiles(&snapshot.frame_history);
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
        &scene_data.meshes,
        &scene_data.materials,
        &scene_data.images,
        &scene_data.renderables,
        &scene_data.dynamic_shadow_markers,
        &scene_data.imported_entities,
        &scene_data.parents,
        &scene_data.point_lights,
        &scene_data.spot_lights,
        &scene_data.directional_lights,
    );

    let pass_metric = if support.timestamp_query && support.timestamp_inside_passes {
        "elapsed_gpu"
    } else {
        "elapsed_cpu"
    };
    let pass_rows = collect_render_metric_rows(&diagnostics, pass_metric, fps);
    let total_pass_ms = pass_rows.iter().map(|row| row.value_per_frame).sum::<f64>();
    let workload = collect_render_workload(&diagnostics, pass_metric, fps);
    let gpu_primitives = collect_render_metric_total(&diagnostics, "clipper_primitives_out", fps);
    let fragment_invocations =
        collect_render_metric_total(&diagnostics, "fragment_shader_invocations", fps);
    let target_stats = collect_render_target_stats(
        &camera_views,
        xr_session_config.as_deref(),
        quality.resolved_msaa().samples(),
    );
    let shadow_cache = shadow_cache_frame.telemetry();
    let dynamic_overlay_enabled = quality.point_light_shadows
        && quality.persistent_point_shadow_cache
        && quality.scalable_point_lighting
        && quality.dynamic_shadow_caster_overlay;
    let point_shadow_policy = if !quality.point_light_shadows {
        "disabled by fixed A/B profile".to_owned()
    } else if quality.max_shadowed_point_lights == 0 {
        "all level-enabled lights".to_owned()
    } else {
        format!("cap {} lights", quality.max_shadowed_point_lights)
    };

    let top_pass = pass_rows.first();
    let using_gpu_timestamps = pass_metric == "elapsed_gpu";
    let bottleneck = bottleneck_hint(
        frame_ms,
        &scene,
        &workload,
        top_pass,
        total_pass_ms,
        using_gpu_timestamps,
        cfg!(target_os = "android"),
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
    if let Some(percentiles) = frame_percentiles {
        let _ = writeln!(
            overview,
            "Frame P50/P95/P99  {:>5.1} / {:>5.1} / {:>5.1} ms  ({}f/{:.0}s)",
            percentiles.p50_ms,
            percentiles.p95_ms,
            percentiles.p99_ms,
            percentiles.sample_count,
            percentiles.window_seconds,
        );
    }
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
        "PointLight A/B    {}",
        quality.point_light_ab_profile_label()
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
        "Shadow-enabled    {:>4} @ {:>4}px",
        scene.shadowed_point_lights,
        quality.resolved_point_shadow_map_size(),
    );
    let _ = writeln!(
        overview,
        "Cache faces R/D/U {:>4} / {:>2} / {:>2}",
        shadow_cache.resident_views, shadow_cache.rendered_views, shadow_cache.reused_views,
    );
    let _ = writeln!(
        overview,
        "Shadow cache mode {:>4}",
        if quality.persistent_point_shadow_cache {
            "ON"
        } else {
            "OFF"
        },
    );
    let _ = writeln!(
        overview,
        "Light selection  {}",
        quality.point_light_selection_mode_label(),
    );
    if quality.clustered_light_preselection {
        let average_candidates = if cluster_preselection.nonempty_superclusters > 0 {
            cluster_preselection.candidate_references as f64
                / cluster_preselection.nonempty_superclusters as f64
        } else {
            0.0
        };
        let _ = writeln!(
            overview,
            "Cluster select    {:>4} / XR {:>1} / avgN {:>3.1} max {:>2}",
            if cluster_preselection.active {
                "ON"
            } else {
                "WAIT"
            },
            cluster_preselection.xr_views,
            average_candidates,
            cluster_preselection.max_candidates,
        );
    }
    let _ = writeln!(
        overview,
        "Dynamic overlay   {:>4} / caster {:>2} draw {:>2}",
        if dynamic_overlay_enabled { "ON" } else { "OFF" },
        shadow_cache.dynamic_casters,
        shadow_cache.dynamic_views_rendered,
    );
    let _ = writeln!(
        overview,
        "Motion L S/M/K/F  {:>2}/{:>2}/{:>2}/{:>2} C S/D {:>2}/{:>2}",
        motion_policy.light_static,
        motion_policy.light_micro_motion,
        motion_policy.light_slow_moving,
        motion_policy.light_fully_dynamic,
        motion_policy.caster_static,
        motion_policy.caster_dynamic_overlay,
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
    let _ = writeln!(
        overview,
        "Exact local list  <= {:>2} lights",
        quality.resolved_point_light_exact_threshold(),
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
        "Batch savings est    {:>10}",
        format_count(scene.estimated_instance_savings)
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
    let _ = writeln!(
        overview,
        "Caster tris S / D   {:>9} / {}",
        format_count(scene.static_shadow_caster_triangles),
        format_count(scene.dynamic_shadow_caster_triangles),
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
    if let Some((class, class_stats)) = workload.top_timing() {
        let percentage = if workload.total_timing_ms > 0.0 {
            class_stats.timing_ms / workload.total_timing_ms * 100.0
        } else {
            0.0
        };
        let _ = writeln!(
            overview,
            "Top workload: {}  {:.2} ms ({:.0}%)",
            class.label(),
            class_stats.timing_ms,
            percentage,
        );
    }
    let _ = writeln!(
        overview,
        "Pass timing source: {}",
        if pass_metric == "elapsed_gpu" {
            if cfg!(target_os = "android") {
                "GPU timestamps (instrumented spans only)"
            } else {
                "GPU timestamps"
            }
        } else {
            "CPU command recording fallback"
        }
    );

    let mut workload_page = String::with_capacity(3_200);
    let _ = writeln!(
        workload_page,
        "ZEVY RENDER DEBUG  |  {}",
        RenderDebugPage::Workload.label()
    );
    let _ = writeln!(
        workload_page,
        "A/B: {}",
        quality.point_light_ab_profile_label()
    );
    let _ = writeln!(
        workload_page,
        "Timing: {}    Counters: {}",
        if using_gpu_timestamps {
            if cfg!(target_os = "android") {
                "GPU spans (partial)"
            } else {
                "GPU"
            }
        } else {
            "CPU fallback"
        },
        if support.pipeline_statistics {
            "GPU pipeline"
        } else {
            "N/A on this adapter"
        },
    );
    let _ = writeln!(
        workload_page,
        "------------------------------------------------------------"
    );
    let _ = writeln!(
        workload_page,
        "Category              ms     %       VS       Prim      Frag"
    );
    for class in RenderWorkloadClass::ALL {
        let stats = workload.stats(class);
        let percentage = if workload.total_timing_ms > 0.0 {
            stats.timing_ms / workload.total_timing_ms * 100.0
        } else {
            0.0
        };
        let _ = writeln!(
            workload_page,
            "{:<18} {:>6.2} {:>5.1} {:>9} {:>9} {:>9}",
            class.label(),
            stats.timing_ms,
            percentage,
            format_optional_compact_count(stats.vertex_invocations),
            format_optional_compact_count(stats.clipper_primitives_out),
            format_optional_compact_count(stats.fragment_invocations),
        );
    }
    let _ = writeln!(
        workload_page,
        "------------------------------------------------------------"
    );
    let main_workload = workload.stats(RenderWorkloadClass::Main3d);
    if let Some(main_fragments) = main_workload.fragment_invocations
        && target_stats.total_target_pixels > 0
    {
        let _ = writeln!(
            workload_page,
            "Main fragment / target px   {:>8.2}",
            main_fragments / target_stats.total_target_pixels as f64,
        );
    }
    if let (Some(main_fragments), Some(main_primitives)) = (
        main_workload.fragment_invocations,
        main_workload.clipper_primitives_out,
    ) && main_primitives > 0.0
    {
        let _ = writeln!(
            workload_page,
            "Main fragment / primitive   {:>8.2}  (coverage proxy)",
            main_fragments / main_primitives,
        );
    }
    let static_triangle_upper_bound = scene
        .static_shadow_caster_triangles
        .saturating_mul(shadow_cache.rendered_views);
    let dynamic_triangle_upper_bound = scene
        .dynamic_shadow_caster_triangles
        .saturating_mul(shadow_cache.dynamic_views_rendered);
    let shadow_face_texels = (quality.resolved_point_shadow_map_size() as u64).saturating_pow(2);
    let updated_shadow_texels = shadow_cache
        .rendered_views
        .saturating_add(shadow_cache.dynamic_views_rendered)
        .saturating_mul(shadow_face_texels);
    let _ = writeln!(
        workload_page,
        "Visible verts / tris       {:>9} / {}",
        format_count(scene.visible_vertices),
        format_count(scene.visible_triangles),
    );
    let _ = writeln!(
        workload_page,
        "Main entities O/T          {:>9} / {}",
        scene.visible_opaque_entities, scene.visible_transparent_entities,
    );
    let _ = writeln!(
        workload_page,
        "Main draws O/T est         {:>9} / {}",
        scene.estimated_opaque_draws, scene.estimated_transparent_draws,
    );
    let _ = writeln!(
        workload_page,
        "Batch savings est          {:>9}",
        scene.estimated_instance_savings,
    );
    let _ = writeln!(
        workload_page,
        "Loaded casters S/D         {:>9} / {} entities",
        scene.static_shadow_caster_entities, scene.dynamic_shadow_caster_entities,
    );
    let _ = writeln!(
        workload_page,
        "Loaded caster tris S/D     {:>9} / {}",
        format_count(scene.static_shadow_caster_triangles),
        format_count(scene.dynamic_shadow_caster_triangles),
    );
    let _ = writeln!(
        workload_page,
        "Updated faces static/dyn   {:>9} / {}",
        shadow_cache.rendered_views, shadow_cache.dynamic_views_rendered,
    );
    let _ = writeln!(
        workload_page,
        "Caster tri upper bound S/D {:>9} / {}",
        format_count(static_triangle_upper_bound),
        format_count(dynamic_triangle_upper_bound),
    );
    let _ = writeln!(
        workload_page,
        "Updated shadow texels      {:>9}",
        format_count(updated_shadow_texels),
    );
    let _ = writeln!(
        workload_page,
        "------------------------------------------------------------"
    );
    let _ = writeln!(
        workload_page,
        "GPU rows include all eyes. Caster upper bound is before face frustum culling."
    );
    if !support.pipeline_statistics {
        let _ = writeln!(
            workload_page,
            "Use AGI/vendor capture for exact Android VS/primitive/fragment counters."
        );
    }

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
            if cfg!(target_os = "android") {
                "GPU spans (partial; use runtime/AGI for frame total)"
            } else {
                "GPU"
            }
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
        "Point direct / shadows      {} / {}",
        if quality.point_light_direct_lighting {
            "ON"
        } else {
            "OFF (compiled)"
        },
        if quality.point_light_shadows {
            "ON"
        } else {
            "OFF (fixed A/B)"
        },
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
        "Light policy S/M/K/F        {} / {} / {} / {}",
        motion_policy.light_static,
        motion_policy.light_micro_motion,
        motion_policy.light_slow_moving,
        motion_policy.light_fully_dynamic,
    );
    let _ = writeln!(
        material_page,
        "Caster policy static/dyn    {} / {} ({} transitions)",
        motion_policy.caster_static,
        motion_policy.caster_dynamic_overlay,
        motion_policy.transitions_this_frame,
    );
    if quality.continuous_point_shadow_proxy_enabled() {
        let _ = writeln!(
            material_page,
            "Projection animation       continuous proxy x{:.2} (no sway redraw)",
            quality.resolved_point_shadow_proxy_sway_scale(),
        );
    } else {
        let _ = writeln!(
            material_page,
            "Projection animation       real redraw {:.1} Hz, <= {} light/frame",
            quality.resolved_cached_point_shadow_update_hz(),
            quality.max_cached_point_shadow_updates_per_frame,
        );
    }
    let _ = writeln!(
        material_page,
        "Cluster grid budget         {} / {} z-slices",
        quality.resolved_cluster_total(),
        quality.resolved_cluster_z_slices(),
    );
    let _ = writeln!(
        material_page,
        "Scalable PointLight path    {}",
        if !quality.point_light_direct_lighting && quality.scalable_point_lighting {
            "compiled out for fixed A/B"
        } else {
            quality.point_light_selection_mode_label()
        }
    );
    let _ = writeln!(
        material_page,
        "Cluster preselection        {} ({} views / {} superclusters)",
        if cluster_preselection.active {
            "active"
        } else if quality.clustered_light_preselection && quality.world_space_light_reservoir {
            "ignored (world path wins)"
        } else if quality.clustered_light_preselection {
            "waiting/fallback"
        } else {
            "disabled"
        },
        cluster_preselection.views,
        cluster_preselection.superclusters,
    );
    if quality.scalable_point_lighting && quality.point_light_direct_lighting {
        let hero_lights = quality.resolved_point_light_hero_samples() as usize;
        let tail_samples = quality.resolved_point_light_tail_samples() as usize;
        let bounded_evaluations = hero_lights.saturating_add(tail_samples);
        let _ = writeln!(
            material_page,
            "Point BRDF budget/pixel    <= {} exact / {} overflow",
            quality.resolved_point_light_exact_threshold(),
            bounded_evaluations,
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
    snapshot.workload = workload_page;
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

fn record_frame_time_sample(
    history: &mut VecDeque<FrameTimeSample>,
    timestamp_seconds: f64,
    frame_ms: f64,
) {
    if !timestamp_seconds.is_finite() || !frame_ms.is_finite() || frame_ms <= 0.0 {
        return;
    }
    if history
        .back()
        .is_some_and(|sample| sample.timestamp_seconds > timestamp_seconds)
    {
        history.clear();
    }
    history.push_back(FrameTimeSample {
        timestamp_seconds,
        frame_ms,
    });
    let oldest_allowed = timestamp_seconds - FRAME_HISTORY_SECONDS;
    while history
        .front()
        .is_some_and(|sample| sample.timestamp_seconds < oldest_allowed)
    {
        history.pop_front();
    }
}

fn frame_time_percentiles(history: &VecDeque<FrameTimeSample>) -> Option<FrameTimePercentiles> {
    if history.is_empty() {
        return None;
    }
    let mut values = history
        .iter()
        .map(|sample| sample.frame_ms)
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let first_timestamp = history.front()?.timestamp_seconds;
    let last_timestamp = history.back()?.timestamp_seconds;
    Some(FrameTimePercentiles {
        p50_ms: percentile_sorted(&values, 0.50),
        p95_ms: percentile_sorted(&values, 0.95),
        p99_ms: percentile_sorted(&values, 0.99),
        sample_count: values.len(),
        window_seconds: (last_timestamp - first_timestamp).max(0.0),
    })
}

fn percentile_sorted(values: &[f64], quantile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let position = quantile.clamp(0.0, 1.0) * (values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let fraction = position - lower as f64;
    values[lower] + (values[upper] - values[lower]) * fraction
}

#[derive(Default)]
struct SceneRenderStats {
    visible_mesh_entities: u64,
    visible_opaque_entities: u64,
    visible_transparent_entities: u64,
    unique_meshes: usize,
    visible_vertices: u64,
    visible_triangles: u64,
    estimated_opaque_draws: u64,
    estimated_transparent_draws: u64,
    estimated_main_draws: u64,
    estimated_instance_savings: u64,
    static_shadow_caster_entities: u64,
    dynamic_shadow_caster_entities: u64,
    static_shadow_caster_triangles: u64,
    dynamic_shadow_caster_triangles: u64,
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
        Entity,
        &Mesh3d,
        &MeshMaterial3d<StandardMaterial>,
        &ViewVisibility,
        Option<&NotShadowCaster>,
    )>,
    dynamic_shadow_markers: &Query<(), With<DynamicShadowCaster>>,
    imported_entities: &Query<(), With<ImportedZevyEntity>>,
    parents: &Query<&ChildOf>,
    point_lights: &Query<&PointLight>,
    spot_lights: &Query<&SpotLight>,
    directional_lights: &Query<&DirectionalLight>,
) -> SceneRenderStats {
    let mut stats = SceneRenderStats::default();
    let mut unique_meshes = HashSet::new();
    let mut unique_materials = HashSet::new();
    let mut opaque_draw_keys = HashSet::new();
    let mut transparent_draws = 0_u64;

    for (entity, mesh_handle, material_handle, view_visibility, not_shadow_caster) in
        renderables.iter()
    {
        let mesh = meshes.get(mesh_handle.0.id());
        let mesh_triangles = mesh.map(mesh_triangle_count).unwrap_or_default();
        if not_shadow_caster.is_none() {
            if is_dynamic_shadow_caster(entity, dynamic_shadow_markers, imported_entities, parents)
            {
                stats.dynamic_shadow_caster_entities += 1;
                stats.dynamic_shadow_caster_triangles += mesh_triangles;
            } else {
                stats.static_shadow_caster_entities += 1;
                stats.static_shadow_caster_triangles += mesh_triangles;
            }
        }

        if !view_visibility.get() {
            continue;
        }

        stats.visible_mesh_entities += 1;
        let mesh_id = mesh_handle.0.id();
        let material_id = material_handle.0.id();
        unique_meshes.insert(mesh_id);
        unique_materials.insert(material_id);

        if let Some(mesh) = mesh {
            stats.visible_vertices += mesh.count_vertices() as u64;
            stats.visible_triangles += mesh_triangles;
        }

        let is_blended = materials.get(material_id).is_some_and(|material| {
            matches!(
                material.alpha_mode,
                AlphaMode::Blend | AlphaMode::Premultiplied | AlphaMode::Add | AlphaMode::Multiply
            )
        });
        if is_blended {
            stats.visible_transparent_entities += 1;
            transparent_draws += 1;
        } else {
            stats.visible_opaque_entities += 1;
            opaque_draw_keys.insert((mesh_id, material_id));
        }
    }

    stats.unique_meshes = unique_meshes.len();
    stats.unique_materials = unique_materials.len();
    stats.estimated_opaque_draws = opaque_draw_keys.len() as u64;
    stats.estimated_transparent_draws = transparent_draws;
    stats.estimated_main_draws = stats.estimated_opaque_draws + stats.estimated_transparent_draws;
    stats.estimated_instance_savings = stats
        .visible_mesh_entities
        .saturating_sub(stats.estimated_main_draws);

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
enum RenderWorkloadClass {
    Main3d = 0,
    Visibility = 1,
    StaticShadow = 2,
    DynamicShadow = 3,
    PostProcess = 4,
    Ui = 5,
    Other = 6,
}

impl RenderWorkloadClass {
    const ALL: [Self; 7] = [
        Self::Main3d,
        Self::Visibility,
        Self::StaticShadow,
        Self::DynamicShadow,
        Self::PostProcess,
        Self::Ui,
        Self::Other,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Main3d => "Main 3D",
            Self::Visibility => "Depth / visibility",
            Self::StaticShadow => "Static shadow",
            Self::DynamicShadow => "Dynamic shadow",
            Self::PostProcess => "Post-process",
            Self::Ui => "UI / debug",
            Self::Other => "Other / compute",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RenderWorkloadClassStats {
    timing_ms: f64,
    vertex_invocations: Option<f64>,
    clipper_invocations: Option<f64>,
    clipper_primitives_out: Option<f64>,
    fragment_invocations: Option<f64>,
    compute_invocations: Option<f64>,
}

#[derive(Debug, Default)]
struct RenderWorkloadBreakdown {
    classes: [RenderWorkloadClassStats; 7],
    total_timing_ms: f64,
}

impl RenderWorkloadBreakdown {
    fn stats(&self, class: RenderWorkloadClass) -> &RenderWorkloadClassStats {
        &self.classes[class as usize]
    }

    fn stats_mut(&mut self, class: RenderWorkloadClass) -> &mut RenderWorkloadClassStats {
        &mut self.classes[class as usize]
    }

    fn top_timing(&self) -> Option<(RenderWorkloadClass, &RenderWorkloadClassStats)> {
        RenderWorkloadClass::ALL
            .into_iter()
            .map(|class| (class, self.stats(class)))
            .filter(|(_, stats)| stats.timing_ms > 0.0)
            .max_by(|(_, left), (_, right)| left.timing_ms.total_cmp(&right.timing_ms))
    }
}

#[derive(Clone, Copy)]
enum RenderCounterKind {
    Vertex,
    Clipper,
    Primitive,
    Fragment,
    Compute,
}

fn collect_render_workload(
    diagnostics: &DiagnosticsStore,
    timing_metric: &str,
    fps: f64,
) -> RenderWorkloadBreakdown {
    let mut breakdown = RenderWorkloadBreakdown::default();
    for row in collect_render_metric_rows(diagnostics, timing_metric, fps) {
        let stats = breakdown.stats_mut(classify_render_pass(&row.label));
        stats.timing_ms += row.value_per_frame;
        breakdown.total_timing_ms += row.value_per_frame;
    }

    for (metric, kind) in [
        ("vertex_shader_invocations", RenderCounterKind::Vertex),
        ("clipper_invocations", RenderCounterKind::Clipper),
        ("clipper_primitives_out", RenderCounterKind::Primitive),
        ("fragment_shader_invocations", RenderCounterKind::Fragment),
        ("compute_shader_invocations", RenderCounterKind::Compute),
    ] {
        for row in collect_render_metric_rows(diagnostics, metric, fps) {
            let stats = breakdown.stats_mut(classify_render_pass(&row.label));
            let destination = match kind {
                RenderCounterKind::Vertex => &mut stats.vertex_invocations,
                RenderCounterKind::Clipper => &mut stats.clipper_invocations,
                RenderCounterKind::Primitive => &mut stats.clipper_primitives_out,
                RenderCounterKind::Fragment => &mut stats.fragment_invocations,
                RenderCounterKind::Compute => &mut stats.compute_invocations,
            };
            *destination = Some(destination.unwrap_or_default() + row.value_per_frame);
        }
    }

    breakdown
}

fn classify_render_pass(label: &str) -> RenderWorkloadClass {
    let label = label.to_ascii_lowercase();
    if label.contains("dynamic point shadow") || label.contains("dynamic shadow") {
        return RenderWorkloadClass::DynamicShadow;
    }
    if label.contains("shadow") {
        return RenderWorkloadClass::StaticShadow;
    }
    if label.contains("prepass")
        || label.contains("depth pre")
        || label.contains("visibility buffer")
        || label.contains("occlusion")
        || label.contains("meshlet")
    {
        return RenderWorkloadClass::Visibility;
    }
    if label.contains("main_opaque")
        || label.contains("main transparent")
        || label.contains("main_transparent")
        || label.contains("opaque_pass_3d")
        || label.contains("transparent_pass_3d")
        || label.contains("transmissive")
        || label.contains("deferred")
    {
        return RenderWorkloadClass::Main3d;
    }
    if label.contains("ui") || label.contains("egui") || label.contains("gizmo") {
        return RenderWorkloadClass::Ui;
    }
    if label.contains("bloom")
        || label.contains("tonemap")
        || label.contains("upscal")
        || label.contains("fxaa")
        || label.contains("smaa")
        || label.contains("taa")
        || label.contains("motion blur")
        || label.contains("motion_blur")
        || label.contains("depth of field")
        || label.contains("depth_of_field")
        || label.contains("chromatic")
        || label.contains("ssao")
        || label.contains("screen space")
    {
        return RenderWorkloadClass::PostProcess;
    }
    RenderWorkloadClass::Other
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
    workload: &RenderWorkloadBreakdown,
    top_pass: Option<&RenderMetricRow>,
    total_pass_ms: f64,
    using_gpu_timestamps: bool,
    timestamps_may_be_partial: bool,
) -> &'static str {
    if frame_ms <= 0.0 {
        return "collecting samples";
    }
    if using_gpu_timestamps && total_pass_ms > 0.0 && total_pass_ms < frame_ms * 0.55 {
        if timestamps_may_be_partial {
            return "external/runtime GPU timing required; mobile pass timestamps are partial";
        }
        return "CPU, asset streaming or frame pacing; measured GPU passes are below frame time";
    }
    if let Some((class, stats)) = workload.top_timing() {
        let share = if workload.total_timing_ms > 0.0 {
            stats.timing_ms / workload.total_timing_ms
        } else {
            0.0
        };
        if share >= 0.30
            && matches!(
                class,
                RenderWorkloadClass::StaticShadow | RenderWorkloadClass::DynamicShadow
            )
        {
            return "shadow projection/update; inspect updated faces, caster geometry and cache reuse";
        }
        if share >= 0.40 && class == RenderWorkloadClass::Main3d {
            return "main 3D shading; compare direct-only vs geometry floor and fragment/primitive";
        }
        if share >= 0.35 && class == RenderWorkloadClass::Visibility {
            return "visibility/depth geometry; inspect duplicated views and submitted caster/main meshes";
        }
        if share >= 0.35 && class == RenderWorkloadClass::PostProcess {
            return "post-processing / render-target bandwidth; compare scale and MSAA tiers";
        }
    }
    if let Some(top_pass) = top_pass {
        let share = if total_pass_ms > 0.0 {
            top_pass.value_per_frame / total_pass_ms
        } else {
            0.0
        };
        let label = top_pass.label.to_ascii_lowercase();
        if share >= 0.35 && label.contains("shadow") {
            return "shadow rendering; inspect updated faces, caster geometry and cache reuse";
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

fn format_optional_compact_count(value: Option<f64>) -> String {
    let Some(value) = value else {
        return "N/A".to_owned();
    };
    let value = value.max(0.0);
    if value >= 1_000_000_000.0 {
        format!("{:.2}G", value / 1_000_000_000.0)
    } else if value >= 1_000_000.0 {
        format!("{:.2}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else {
        format!("{value:.0}")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_pass_classifier_separates_main_and_shadow_layers() {
        assert_eq!(
            classify_render_pass("main_opaque_pass_3d"),
            RenderWorkloadClass::Main3d
        );
        assert_eq!(
            classify_render_pass("shadow pass point light 3 +x"),
            RenderWorkloadClass::StaticShadow
        );
        assert_eq!(
            classify_render_pass("dynamic point shadow 12v1 face 5"),
            RenderWorkloadClass::DynamicShadow
        );
        assert_eq!(
            classify_render_pass("prepass_3d"),
            RenderWorkloadClass::Visibility
        );
        assert_eq!(
            classify_render_pass("bloom_downsample"),
            RenderWorkloadClass::PostProcess
        );
    }

    #[test]
    fn frame_history_reports_interpolated_percentiles_and_evicts_old_samples() {
        let mut history = VecDeque::new();
        for (timestamp, frame_ms) in [(0.0, 10.0), (1.0, 20.0), (2.0, 30.0), (3.0, 40.0)] {
            record_frame_time_sample(&mut history, timestamp, frame_ms);
        }
        let percentiles = frame_time_percentiles(&history).unwrap();
        assert!((percentiles.p50_ms - 25.0).abs() < 0.001);
        assert!((percentiles.p95_ms - 38.5).abs() < 0.001);
        assert!((percentiles.p99_ms - 39.7).abs() < 0.001);

        record_frame_time_sample(&mut history, FRAME_HISTORY_SECONDS + 1.0, 50.0);
        assert_eq!(history.front().unwrap().timestamp_seconds, 1.0);
        assert_eq!(history.back().unwrap().frame_ms, 50.0);
    }

    #[test]
    fn hud_page_cycle_includes_workload_breakdown() {
        assert_eq!(RenderDebugPage::Overview.next(), RenderDebugPage::Workload);
        assert_eq!(RenderDebugPage::Workload.next(), RenderDebugPage::Passes);
        assert_eq!(RenderDebugPage::Passes.next(), RenderDebugPage::Materials);
        assert_eq!(RenderDebugPage::Materials.next(), RenderDebugPage::Overview);
    }

    #[test]
    fn debug_page_labels_support_argv_and_android_property_values() {
        assert_eq!(
            debug_page_from_label("workload"),
            Some(RenderDebugPage::Workload)
        );
        assert_eq!(
            debug_page_from_label(" PASSES "),
            Some(RenderDebugPage::Passes)
        );
        assert_eq!(debug_page_from_label("invalid"), None);
        assert_eq!(parse_debug_bool("ON"), Some(true));
        assert_eq!(parse_debug_bool("0"), Some(false));
        assert_eq!(parse_debug_bool("maybe"), None);
    }

    #[test]
    fn partial_mobile_gpu_spans_do_not_claim_a_cpu_bottleneck() {
        let scene = SceneRenderStats::default();
        let workload = RenderWorkloadBreakdown::default();

        assert_eq!(
            bottleneck_hint(30.0, &scene, &workload, None, 1.0, true, true),
            "external/runtime GPU timing required; mobile pass timestamps are partial"
        );
        assert_eq!(
            bottleneck_hint(30.0, &scene, &workload, None, 1.0, true, false),
            "CPU, asset streaming or frame pacing; measured GPU passes are below frame time"
        );
    }
}
