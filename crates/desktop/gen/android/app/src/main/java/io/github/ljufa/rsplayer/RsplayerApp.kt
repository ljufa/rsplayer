package io.github.ljufa.rsplayer

import android.app.Application

/**
 * Process entry point. The Rust entry point is started by wry's process
 * lifecycle observer, which can fire before the activity's own onCreate, so
 * the environment the backend reads (port, data dir, music dir) has to be in
 * place here — the earliest hook in the process.
 */
class RsplayerApp : Application() {
    override fun onCreate() {
        super.onCreate()
        BackendConfig.ensureInitialized(this)
    }
}
