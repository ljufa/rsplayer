//! Android-only startup glue, run once before anything else in [`crate::run`].
//!
//! Two things the Tauri/tao runtime does not do for us:
//! - install a logger that writes to logcat (tao only redirects stdout/stderr,
//!   and `env_logger` would print to that pipe without levels or filtering);
//! - register the JVM + activity with `ndk-context`. cpal's AAudio host and
//!   iroh's interface discovery (`netdev`) call `ndk_context::android_context()`
//!   and panic if nothing registered it. tao keeps the raw pointers in its
//!   activity table, populated from `WryActivity.onCreate`; the Rust entry
//!   point is started by a process lifecycle observer that may fire before
//!   that, so wait briefly for the activity to show up.

use std::time::{Duration, Instant};

use log::{error, info};

const ACTIVITY_WAIT: Duration = Duration::from_secs(10);

pub fn early_init() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Trace)
            .with_tag("rsplayer")
            .with_filter(android_logger::FilterBuilder::new().parse(&filter).build()),
    );

    let deadline = Instant::now() + ACTIVITY_WAIT;
    loop {
        if let Some(ctx) = tauri::tao::platform::android::prelude::main_android_context() {
            // SAFETY: both pointers come from tao's JNI activity registration
            // and stay valid for the life of the process; this runs once,
            // before any thread touches cpal or iroh.
            unsafe { ndk_context::initialize_android_context(ctx.java_vm, ctx.context_jobject) };
            info!("ndk-context registered from the Tauri activity");
            return;
        }
        if Instant::now() > deadline {
            error!("No Android activity context after {ACTIVITY_WAIT:?}; audio device enumeration and multiroom discovery will fail");
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
