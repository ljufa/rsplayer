package io.github.ljufa.rsplayer

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioManager
import android.net.wifi.WifiManager
import android.os.Build
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * Foreground media service: owns the [MediaSession] for lock-screen /
 * notification / headset controls and keeps playback alive when the activity
 * is in the background. media3 promotes the service to a `mediaPlayback`
 * foreground service while the player reports playing.
 *
 * Also holds the Wi-Fi multicast lock that iroh's mDNS peer discovery
 * (multiroom) needs — Android filters multicast otherwise.
 */
@UnstableApi
class PlaybackService : MediaSessionService() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private lateinit var client: BackendClient
    private lateinit var player: BackendPlayer
    private var session: MediaSession? = null
    private var multicastLock: WifiManager.MulticastLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    private val noisyReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) player.pauseIfPlaying()
        }
    }

    override fun onCreate() {
        super.onCreate()
        BackendConfig.ensureInitialized(this)

        client = BackendClient(scope)
        player = BackendPlayer(this, mainLooper, client)

        val openApp = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        session = MediaSession.Builder(this, player)
            .setSessionActivity(openApp)
            .build()
            .also { addSession(it) } // sessions built outside onGetSession must be registered explicitly

        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        multicastLock = wifi.createMulticastLock("rsplayer-mdns").apply {
            setReferenceCounted(false)
            acquire()
        }
        // Keeps Wi-Fi awake for radio/podcast streams with the screen off.
        val wifiMode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            WifiManager.WIFI_MODE_FULL_LOW_LATENCY
        } else {
            @Suppress("DEPRECATION")
            WifiManager.WIFI_MODE_FULL_HIGH_PERF
        }
        wifiLock = wifi.createWifiLock(wifiMode, "rsplayer-stream").apply {
            setReferenceCounted(false)
        }

        ContextCompat.registerReceiver(
            this,
            noisyReceiver,
            IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )

        scope.launch {
            client.state.collect { state ->
                player.update(state)
                if (state.isPlaying) wifiLock?.takeIf { !it.isHeld }?.acquire()
                else wifiLock?.takeIf { it.isHeld }?.release()
            }
        }
        client.start()
        Log.i(TAG, "PlaybackService created")
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    override fun onTaskRemoved(rootIntent: Intent?) {
        if (!player.isPlaying) {
            stopSelf()
        }
    }

    override fun onDestroy() {
        unregisterReceiver(noisyReceiver)
        session?.run {
            player.release()
            release()
        }
        session = null
        client.close()
        wifiLock?.takeIf { it.isHeld }?.release()
        multicastLock?.takeIf { it.isHeld }?.release()
        scope.cancel()
        super.onDestroy()
    }

    private companion object {
        const val TAG = "rsplayer"
    }
}
