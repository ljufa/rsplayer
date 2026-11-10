package de.rsplayer.app

import android.content.Context

/**
 * Entry points into the Rust library that do not need the Tauri activity
 * (see `crates/desktop/src/android.rs`). Call both from the main thread.
 */
object NativeBackend {
    init {
        System.loadLibrary("rsplayer_desktop_lib")
    }

    /**
     * Starts the backend if it is not running yet. This is what lets the
     * process work without an activity: Android Auto, a media button or a
     * sticky restart create only [PlaybackService]. [context] must be the
     * application context, and [BackendConfig] must be initialized.
     */
    @JvmStatic
    external fun start(context: Context)

    /** Must run in [MainActivity.onCreate] before `super.onCreate`. */
    @JvmStatic
    external fun activityCreating()
}
