//! Android-only startup glue.
//!
//! The backend is not tied to the Tauri activity: Android Auto (or a media
//! button, or a sticky restart) can start the process by binding the Kotlin
//! `PlaybackService` alone, with no activity and so no Tauri runtime. The
//! backend therefore lives in a process-wide [`AndroidBackend`], started by
//! whoever comes first:
//! - the Kotlin `NativeBackend.start(context)`, called from
//!   `PlaybackService.onCreate` ([`Java_de_rsplayer_app_NativeBackend_start`]);
//! - [`crate::run`], the Tauri entry point, once an activity exists.
//!
//! Two things the Tauri/tao runtime does not do for us:
//! - install a logger that writes to logcat (tao only redirects stdout/stderr,
//!   and `env_logger` would print to that pipe without levels or filtering);
//! - register the JVM + application context with `ndk-context` *before an
//!   activity exists*. cpal's AAudio host and iroh's interface discovery
//!   (`netdev`) call `ndk_context::android_context()` and panic if nothing
//!   registered it. tao (0.37+) registers it in `WryActivity.onCreate`, and
//!   `ndk-context` asserts that it is initialized only once, so the two
//!   registrations must never overlap, see [`CONTEXT_OWNED`].
//!
//! All context bookkeeping happens on the Android main thread (service and
//! activity `onCreate`), which is what makes the hand-over below race free
//! among the registrations themselves.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use jni::JNIEnv;
use jni::objects::{JClass, JObject};
use log::{error, info};

use crate::{BackendRun, ShutdownSlot};

const ACTIVITY_WAIT: Duration = Duration::from_secs(10);

/// We registered the application context with `ndk-context` ourselves
/// (service-only start) and tao has not taken over yet.
static CONTEXT_OWNED: AtomicBool = AtomicBool::new(false);
/// An activity is being or has been created in this process, so tao owns the
/// `ndk-context` registration from now on.
static ACTIVITY_SEEN: AtomicBool = AtomicBool::new(false);

static BACKEND: OnceLock<AndroidBackend> = OnceLock::new();

/// The one backend of this process and the runtime it runs on.
pub struct AndroidBackend {
    pub runtime: tokio::runtime::Runtime,
    pub port: u16,
    pub shutdown: ShutdownSlot,
    /// Handed to the Tauri app (restart loop) when an activity shows up.
    run: Mutex<Option<BackendRun>>,
}

impl AndroidBackend {
    pub fn take_run(&self) -> Option<BackendRun> {
        self.run.lock().ok().and_then(|mut run| run.take())
    }
}

/// The running backend, started on first use. `ndk-context` must be
/// registered by then, by us or by tao.
pub fn backend() -> &'static AndroidBackend {
    BACKEND.get_or_init(|| {
        init_logging();
        let port = crate::prepare_environment();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");
        let shutdown: ShutdownSlot = Arc::new(Mutex::new(None));
        let run = {
            let _guard = runtime.enter();
            crate::spawn_backend(&shutdown)
        };
        info!("Backend started on port {port}");
        AndroidBackend {
            runtime,
            port,
            shutdown,
            run: Mutex::new(Some(run)),
        }
    })
}

fn init_logging() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Trace)
            .with_tag("rsplayer")
            .with_filter(android_logger::FilterBuilder::new().parse(&filter).build()),
    );
}

/// Called at the top of [`crate::run`]. The Rust entry point is started by a
/// process lifecycle observer that may fire before tao has seen the activity
/// (and registered `ndk-context`), so wait briefly for it to show up.
pub fn wait_for_activity_context() {
    init_logging();
    let deadline = Instant::now() + ACTIVITY_WAIT;
    loop {
        if tauri::tao::platform::android::prelude::main_android_context().is_some() {
            info!("Android context registered by the Tauri activity");
            return;
        }
        if Instant::now() > deadline {
            error!("No Android activity context after {ACTIVITY_WAIT:?}; audio device enumeration and multiroom discovery will fail");
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// `NativeBackend.start(context)`: start the backend without an activity.
/// No-op when it already runs. Main thread only.
#[unsafe(no_mangle)]
pub extern "system" fn Java_de_rsplayer_app_NativeBackend_start<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    context: JObject<'local>,
) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        init_logging();
        if !ACTIVITY_SEEN.load(Ordering::SeqCst) && !CONTEXT_OWNED.load(Ordering::SeqCst) {
            match (env.get_java_vm(), env.new_global_ref(&context)) {
                (Ok(vm), Ok(context)) => {
                    // SAFETY: the VM outlives the process and the global ref
                    // is leaked, so both pointers stay valid; nothing else
                    // has registered a context (no activity yet, checked
                    // above on the main thread).
                    unsafe {
                        ndk_context::initialize_android_context(
                            vm.get_java_vm_pointer().cast(),
                            context.as_obj().as_raw().cast(),
                        );
                    }
                    std::mem::forget(context);
                    CONTEXT_OWNED.store(true, Ordering::SeqCst);
                    info!("Android context registered by the media service (no activity)");
                }
                (vm, context) => {
                    error!(
                        "Cannot register the Android context: vm={:?} context={:?}",
                        vm.err(),
                        context.err()
                    );
                    return;
                }
            }
        }
        backend();
    }));
    if result.is_err() {
        error!("Starting the backend from the media service panicked");
    }
}

/// `NativeBackend.activityCreating()`: called by `MainActivity.onCreate`
/// before `super.onCreate`, where tao registers the application context with
/// `ndk-context`. Drops our own registration first, because a second
/// registration aborts the process. Until tao has re-registered (a few
/// milliseconds later, still inside `onCreate`) `ndk_context::android_context()`
/// panics; only code that opens an audio stream or reads the DNS
/// configuration in exactly that window is affected. Main thread only.
#[unsafe(no_mangle)]
pub extern "system" fn Java_de_rsplayer_app_NativeBackend_activityCreating<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) {
    ACTIVITY_SEEN.store(true, Ordering::SeqCst);
    if CONTEXT_OWNED.swap(false, Ordering::SeqCst) {
        // SAFETY: we registered the context ourselves (`CONTEXT_OWNED`).
        unsafe { ndk_context::release_android_context() };
        init_logging();
        info!("Android context handed over to the Tauri activity");
    }
}
