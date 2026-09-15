package io.github.ljufa.rsplayer

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

/**
 * Runtime permissions: media (audio + cover images) so the library scan can
 * read the shared Music folder, and notifications for the playback controls.
 * No "all files" access — Play policy, and the media grant is enough for
 * audio files reached by path.
 */
object PermissionGate {
    const val REQUEST_CODE = 0x5250

    private val storagePermissions: List<String>
        get() = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            listOf(Manifest.permission.READ_MEDIA_AUDIO, Manifest.permission.READ_MEDIA_IMAGES)
        } else {
            listOf(Manifest.permission.READ_EXTERNAL_STORAGE)
        }

    fun request(activity: Activity) {
        val wanted = storagePermissions.toMutableList()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            wanted += Manifest.permission.POST_NOTIFICATIONS
        }
        val missing = wanted.filter {
            ContextCompat.checkSelfPermission(activity, it) != PackageManager.PERMISSION_GRANTED
        }
        if (missing.isNotEmpty()) {
            ActivityCompat.requestPermissions(activity, missing.toTypedArray(), REQUEST_CODE)
        }
    }

    /** True when this result newly granted audio access. */
    fun audioGranted(requestCode: Int, permissions: Array<out String>, grantResults: IntArray): Boolean {
        if (requestCode != REQUEST_CODE) return false
        val audio = storagePermissions.first()
        return permissions.indices.any { permissions[it] == audio && grantResults[it] == PackageManager.PERMISSION_GRANTED }
    }
}
