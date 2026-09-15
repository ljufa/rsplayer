package io.github.ljufa.rsplayer

import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.view.View
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.annotation.OptIn
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.media3.common.util.UnstableApi

class MainActivity : TauriActivity() {
  @OptIn(UnstableApi::class)
  override fun onCreate(savedInstanceState: Bundle?) {
    // Normally done by RsplayerApp already; harmless to repeat, and it keeps
    // the ordering guarantee even if the process was resurrected for the
    // activity alone.
    BackendConfig.ensureInitialized(this)
    // Transparent system bars with light icons: the window background behind
    // them is the UI's dark base colour (see themes.xml).
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
    )
    super.onCreate(savedInstanceState)
    // Keep the webview out from under the status bar / camera cutout,
    // navigation bar and the on-screen keyboard.
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or
          WindowInsetsCompat.Type.displayCutout() or
          WindowInsetsCompat.Type.ime()
      )
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      WindowInsetsCompat.CONSUMED
    }
    // One dialog at a time: the battery exemption follows the permission
    // dialog (see onRequestPermissionsResult) or comes right away.
    if (!PermissionGate.request(this)) {
      PermissionGate.requestBatteryExemption(this)
    }
  }

  override fun onStart() {
    super.onStart()
    // Plain started service; media3 promotes it to a foreground service once
    // playback starts. Started on every return to the screen, not just in
    // onCreate: Android stops the idle service about a minute after the app
    // leaves the screen, and playback started later would run unprotected
    // (network blocked, process frozen). No-op while it is running.
    startService(Intent(this, PlaybackService::class.java))
  }

  override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    if (PermissionGate.audioGranted(requestCode, permissions, grantResults)) {
      // The startup scan ran before we could read the Music folder.
      BackendClient.fireAndForget(BackendClient.RESCAN_METADATA)
    }
    if (requestCode == PermissionGate.REQUEST_CODE) {
      PermissionGate.requestBatteryExemption(this)
    }
  }
}
