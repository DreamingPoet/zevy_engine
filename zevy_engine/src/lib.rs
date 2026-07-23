mod app;
mod clustered_light_preselection;
mod config;
mod input;
mod platform;
#[cfg(feature = "render_debug")]
mod render_debug;
mod scalable_lighting;
mod scene;
mod shadow_cache;
mod shadow_motion_policy;
mod shadow_overlay;
mod xr;

pub use app::{main, run};
pub use config::RenderQualityConfig;
pub use scene::{
    ImportedZevyEntity, ImportedZevyLevel, ImportedZevyLight, ZevyBevyLightParameters,
    ZevyLevelAsset, ZevyLevelAssetLoader, ZevyLevelEntityDefinition, ZevyLevelPlugin,
    ZevyLevelSceneAsset, ZevyLevelTransform, ZevyLightDefinition, ZevyLightKind,
    ZevyUnrealLightParameters, spawn_zevy_level,
};
pub use shadow_motion_policy::{
    LightShadowAutomaticThresholds, LightShadowMotionClass, LightShadowMotionMode,
    LightShadowMotionPolicy, ResolvedLightShadowMotion, ResolvedShadowCasterMotion,
    ShadowCasterAutomaticThresholds, ShadowCasterMotionClass, ShadowCasterMotionMode,
    ShadowCasterMotionPolicy, ShadowMotionPolicyTelemetry,
};
pub use shadow_overlay::DynamicShadowCaster;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: bevy::window::android_activity::AndroidApp) {
    platform::android_main(android_app);
}
