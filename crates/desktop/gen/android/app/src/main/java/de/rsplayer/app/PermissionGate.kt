package de.rsplayer.app

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.util.Log
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

/**
 * Runtime permissions: media (audio + cover images) so the library scan can
 * read the shared Music folder, and notifications for the playback controls.
 * No "all files" access — Play policy, and the media grant is enough for
 * audio files reached by path.
 *
 * Also asks once to be exempted from battery optimization. Without it Android
 * stops the idle media service a minute after the app leaves the screen,
 * blocks the app's network in the background (streams die on the next
 * reconnect) and won't let playback started remotely — another device's web
 * UI, a multiroom leader — promote the service to the foreground.
 */
object PermissionGate {
    const val REQUEST_CODE = 0x5250
    private const val TAG = "rsplayer"
    private const val PREFS = "permission_gate"
    private const val KEY_BATTERY_ASKED = "battery_exemption_asked"

    private val storagePermissions: List<String>
        get() = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            listOf(Manifest.permission.READ_MEDIA_AUDIO, Manifest.permission.READ_MEDIA_IMAGES)
        } else {
            listOf(Manifest.permission.READ_EXTERNAL_STORAGE)
        }

    /** Requests missing runtime permissions; true when the system dialog was shown. */
    fun request(activity: Activity): Boolean {
        val wanted = storagePermissions.toMutableList()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            wanted += Manifest.permission.POST_NOTIFICATIONS
        }
        val missing = wanted.filter {
            ContextCompat.checkSelfPermission(activity, it) != PackageManager.PERMISSION_GRANTED
        }
        if (missing.isNotEmpty()) {
            ActivityCompat.requestPermissions(activity, missing.toTypedArray(), REQUEST_CODE)
            return true
        }
        return false
    }

    /** True when this result newly granted audio access. */
    fun audioGranted(requestCode: Int, permissions: Array<out String>, grantResults: IntArray): Boolean {
        if (requestCode != REQUEST_CODE) return false
        val audio = storagePermissions.first()
        return permissions.indices.any { permissions[it] == audio && grantResults[it] == PackageManager.PERMISSION_GRANTED }
    }

    /**
     * Shows the system "let the app run in the background" dialog, once. A
     * user who declines can still pick "Unrestricted" in the app's battery
     * settings.
     */
    @SuppressLint("BatteryLife")
    fun requestBatteryExemption(activity: Activity) {
        val power = activity.getSystemService(Context.POWER_SERVICE) as PowerManager
        if (power.isIgnoringBatteryOptimizations(activity.packageName)) return
        val prefs = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (prefs.getBoolean(KEY_BATTERY_ASKED, false)) return
        prefs.edit().putBoolean(KEY_BATTERY_ASKED, true).apply()
        try {
            activity.startActivity(
                Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${activity.packageName}")),
            )
        } catch (e: ActivityNotFoundException) {
            Log.w(TAG, "No battery optimization dialog, opening the settings list", e)
            runCatching { activity.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) }
        }
    }
}
