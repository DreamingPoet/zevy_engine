mod app;
mod platform;
mod scene;
mod xr;

pub use app::{main, run};

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: bevy::window::android_activity::AndroidApp) {
    platform::android_main(android_app);
}
