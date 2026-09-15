package io.github.ljufa.rsplayer

import android.app.Activity
import android.content.Intent
import android.os.Process
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * "Restart RSPlayer" on Android. The backend runs inside this process and
 * can't be torn down and rebuilt in place (its tasks, audio stream and
 * database handle outlive a shutdown), so the whole process is restarted:
 * the Rust wrapper first shuts the backend down (database persisted), then
 * calls [restart], which hands over to [RestartActivity] in a separate
 * process to kill this one and relaunch the app.
 */
@TauriPlugin
class RestartPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun restart(invoke: Invoke) {
        invoke.resolve()
        activity.startActivity(
            Intent(activity, RestartActivity::class.java)
                .putExtra(RestartActivity.EXTRA_MAIN_PID, Process.myPid())
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }
}
