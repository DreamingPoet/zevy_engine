mod app;
mod config;
mod input;
mod platform;
#[cfg(feature = "render_debug")]
mod render_debug;
mod scene;
mod xr;

pub use app::{main, run};
pub use config::RenderQualityConfig;
pub use scene::{
    ImportedZevyEntity, ImportedZevyLevel, ImportedZevyLight, ZevyBevyLightParameters,
    ZevyLevelAsset, ZevyLevelAssetLoader, ZevyLevelEntityDefinition, ZevyLevelPlugin,
    ZevyLevelSceneAsset, ZevyLevelTransform, ZevyLightDefinition, ZevyLightKind,
    ZevyUnrealLightParameters, spawn_zevy_level,
};

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: bevy::window::android_activity::AndroidApp) {
    platform::android_main(android_app);
}
