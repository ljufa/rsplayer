package de.rsplayer.app

import android.app.Application
import android.os.Build

/**
 * Process entry point. The Rust entry point is started by wry's process
 * lifecycle observer, which can fire before the activity's own onCreate, so
 * the environment the backend reads (port, data dir, music dir) has to be in
 * place here — the earliest hook in the process.
 */
class RsplayerApp : Application() {
    override fun onCreate() {
        super.onCreate()
        // The `:restart` process (RestartActivity) runs no backend; setting
        // one up there would delete the port file of the relaunched app.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && getProcessName() != packageName) return
        BackendConfig.ensureInitialized(this)
    }
}
