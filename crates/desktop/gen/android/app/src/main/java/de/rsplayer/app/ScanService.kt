package de.rsplayer.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.os.SystemClock
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.text.NumberFormat

/**
 * Keeps the process running while the backend scans the music library.
 *
 * With nothing playing, Android stops the idle [PlaybackService], freezes the
 * cached process and lets the CPU and Wi-Fi sleep once the app leaves the
 * screen, so a long scan of a remote library stopped half way and its
 * remaining songs stayed missing. This `dataSync` foreground service shows the
 * scan's progress, holds a partial wake lock and a Wi-Fi lock, and stops
 * itself when the scan ends. It has its own backend connection: it must
 * outlive [PlaybackService], which only starts it.
 */
class ScanService : Service() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var client: BackendClient? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var lastNotified = 0L

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Every start must promote within a few seconds, even a repeated one.
        if (promote(ScanState(status = ScanState.Status.RUNNING, phase = "Listing")) && client == null) start()
        return START_NOT_STICKY
    }

    private fun promote(scan: ScanState): Boolean {
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0
        try {
            ServiceCompat.startForeground(this, NOTIFICATION_ID, notification(scan), type)
            return true
        } catch (e: Exception) {
            // ForegroundServiceStartNotAllowedException and the like: without
            // foreground status the locks would not keep the scan alive anyway.
            Log.w(TAG, "Scan service could not go foreground: ${e.message}")
            stopSelf()
            return false
        }
    }

    private fun start() {
        val power = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "rsplayer:library-scan").apply {
            setReferenceCounted(false)
            acquire(MAX_SCAN_MS)
        }
        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        // Same mode as PlaybackService's stream lock.
        val wifiMode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            WifiManager.WIFI_MODE_FULL_LOW_LATENCY
        } else {
            @Suppress("DEPRECATION")
            WifiManager.WIFI_MODE_FULL_HIGH_PERF
        }
        wifiLock = wifi.createWifiLock(wifiMode, "rsplayer-library-scan").apply {
            setReferenceCounted(false)
            acquire()
        }
        val client = BackendClient(scope).also { this.client = it }
        scope.launch {
            client.state.collect { state ->
                when (state.scan.status) {
                    ScanState.Status.IDLE -> stop("scan finished")
                    ScanState.Status.RUNNING -> {
                        // At most one notification update a second.
                        val now = SystemClock.elapsedRealtime()
                        if (now - lastNotified >= 1000) {
                            lastNotified = now
                            getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification(state.scan))
                        }
                    }
                    ScanState.Status.UNKNOWN -> {}
                }
            }
        }
        // A backend that never ran a scan answers the status query with
        // nothing: don't hold the locks waiting for a scan that isn't there.
        scope.launch {
            delay(NO_SCAN_TIMEOUT_MS)
            if (client.state.value.scan.status == ScanState.Status.UNKNOWN) stop("no scan running")
        }
        client.start()
        Log.i(TAG, "Library scan service started")
    }

    private fun stop(reason: String) {
        Log.i(TAG, "Library scan service stopping: $reason")
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    /** Android 15+ limits `dataSync` services to 6 h a day. */
    override fun onTimeout(startId: Int, fgsType: Int) = stop("foreground time limit reached")

    override fun onDestroy() {
        client?.close()
        client = null
        scope.cancel()
        wakeLock?.takeIf { it.isHeld }?.release()
        wifiLock?.takeIf { it.isHeld }?.release()
        super.onDestroy()
    }

    private fun notification(scan: ScanState): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && manager.getNotificationChannel(CHANNEL_ID) == null) {
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Library updates", NotificationManager.IMPORTANCE_LOW).apply {
                    description = "Progress while the music library is updated"
                },
            )
        }
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val number = NumberFormat.getIntegerInstance()
        val text = when {
            scan.current.startsWith("Connection lost") -> scan.current
            scan.phase == "Reading" -> "Reading new songs ${number.format(scan.done)} / ${number.format(scan.total)}"
            scan.phase == "Removing" -> "Removing missing songs"
            else -> "Looking for new music…"
        }
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle("Updating music library")
            .setContentText(text)
            .setProgress(scan.total, scan.done, scan.total == 0)
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .build()
    }

    companion object {
        private const val TAG = "rsplayer"
        private const val CHANNEL_ID = "library_scan"
        private const val NOTIFICATION_ID = 2
        /** Safety cap on the wake lock. */
        private const val MAX_SCAN_MS = 6 * 60 * 60 * 1000L
        private const val NO_SCAN_TIMEOUT_MS = 30_000L

        /** Called when a scan starts, while the app is on screen. */
        fun start(context: Context) {
            try {
                ContextCompat.startForegroundService(context, Intent(context, ScanService::class.java))
            } catch (e: IllegalStateException) {
                // Background start refused (no battery exemption, app not on
                // screen): the scan runs unprotected, as before.
                Log.w(TAG, "Library scan service not started: ${e.message}")
            }
        }
    }
}
