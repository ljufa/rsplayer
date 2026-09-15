package io.github.ljufa.rsplayer

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.os.Process
import android.os.SystemClock
import android.util.Log
import java.io.File

/**
 * Trampoline for [RestartPlugin], running in its own `:restart` process (see
 * the manifest). Relaunching the activity from the main process and then
 * exiting does not work: while that process is still alive the activity
 * manager binds the new activity to it, and it dies with the process. So this
 * activity kills the main process, waits until it is really gone, and only
 * then starts the launcher activity, which gets a fresh process.
 */
class RestartActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val mainPid = intent.getIntExtra(EXTRA_MAIN_PID, -1)
        Thread {
            if (mainPid > 0) {
                Process.killProcess(mainPid)
                val deadline = SystemClock.uptimeMillis() + PROCESS_EXIT_TIMEOUT_MS
                while (File("/proc/$mainPid").exists() && SystemClock.uptimeMillis() < deadline) {
                    SystemClock.sleep(20)
                }
                // Give the activity manager a moment to register the death.
                SystemClock.sleep(100)
            }
            runOnUiThread {
                val component = packageManager.getLaunchIntentForPackage(packageName)?.component
                if (component != null) {
                    startActivity(Intent.makeRestartActivityTask(component))
                } else {
                    Log.e(TAG, "No launch intent for $packageName; cannot restart")
                }
                finish()
                Runtime.getRuntime().exit(0)
            }
        }.start()
    }

    companion object {
        const val EXTRA_MAIN_PID = "mainPid"
        private const val TAG = "RestartActivity"
        private const val PROCESS_EXIT_TIMEOUT_MS = 5_000L
    }
}
