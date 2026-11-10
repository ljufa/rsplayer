package de.rsplayer.app

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.CommandButton
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaLibraryService
import androidx.media3.session.MediaSession
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionError
import androidx.media3.session.SessionResult
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.FutureCallback
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.MoreExecutors
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
 * It is a [MediaLibraryService] so Android Auto lists the app and can browse
 * and play the library, see [MediaLibrary].
 *
 * Also holds the Wi-Fi multicast lock that iroh's mDNS peer discovery
 * (multiroom) needs — Android filters multicast otherwise.
 */
@UnstableApi
class PlaybackService : MediaLibraryService() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private lateinit var client: BackendClient
    private lateinit var player: BackendPlayer
    private val library = MediaLibrary()
    private var session: MediaLibrarySession? = null
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
        // No-op when the activity started the backend already; without one
        // (Android Auto, media button, sticky restart) this is what starts it.
        NativeBackend.start(applicationContext)

        client = BackendClient(scope)
        player = BackendPlayer(this, mainLooper, client, ::play)

        val openApp = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        session = MediaLibrarySession.Builder(this, player, LibraryCallback())
            .setSessionActivity(openApp)
            .setMediaButtonPreferences(ImmutableList.of(playbackModeButton(client.state.value.playbackMode)))
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
            var scanRunning = false
            var queueOf: String? = null
            var playbackMode = client.state.value.playbackMode
            client.state.collect { state ->
                player.update(state)
                if (state.playbackMode != playbackMode) {
                    playbackMode = state.playbackMode
                    session?.setMediaButtonPreferences(ImmutableList.of(playbackModeButton(playbackMode)))
                }
                // The queue shown to controllers starts at the current song.
                if (state.connected && state.file != queueOf) {
                    queueOf = state.file
                    refreshQueue()
                }
                if (!state.connected) queueOf = null
                // A scan just started (Update library, or the startup scan):
                // keep the process alive until it ends, see ScanService.
                val running = state.scan.status == ScanState.Status.RUNNING
                if (running && !scanRunning) ScanService.start(this@PlaybackService)
                scanRunning = running
                if (state.isPlaying) wifiLock?.takeIf { !it.isHeld }?.acquire()
                else wifiLock?.takeIf { it.isHeld }?.release()
            }
        }
        client.start()
        Log.i(TAG, "PlaybackService created")
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaLibrarySession? = session

    /**
     * Serves the browse tree ([MediaLibrary]) to media browsers such as
     * Android Auto. TODO: restrict who may connect.
     */
    private inner class LibraryCallback : MediaLibrarySession.Callback {
        override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult =
            MediaSession.ConnectionResult.AcceptedResultBuilder(session)
                .setAvailableSessionCommands(
                    MediaSession.ConnectionResult.DEFAULT_SESSION_AND_LIBRARY_COMMANDS.buildUpon()
                        .add(CYCLE_PLAYBACK_MODE)
                        .build(),
                )
                .build()

        override fun onCustomCommand(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
            customCommand: SessionCommand,
            args: Bundle,
        ): ListenableFuture<SessionResult> {
            if (customCommand.customAction != CYCLE_PLAYBACK_MODE.customAction) {
                return Futures.immediateFuture(SessionResult(SessionError.ERROR_NOT_SUPPORTED))
            }
            client.cyclePlaybackMode()
            return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
        }

        override fun onGetLibraryRoot(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<MediaItem>> {
            Log.i(TAG, "Media browser connected: ${browser.packageName}")
            return Futures.immediateFuture(LibraryResult.ofItem(library.root, params))
        }

        override fun onGetChildren(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            parentId: String,
            page: Int,
            pageSize: Int,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> {
            return paged(library.children(parentId), page, pageSize, params)
        }

        override fun onSearch(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            query: String,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<Void>> {
            Futures.addCallback(
                library.search(query),
                object : FutureCallback<List<MediaItem>> {
                    override fun onSuccess(found: List<MediaItem>) {
                        session.notifySearchResultChanged(browser, query, found.size, params)
                    }

                    override fun onFailure(t: Throwable) {
                        session.notifySearchResultChanged(browser, query, 0, params)
                    }
                },
                ContextCompat.getMainExecutor(this@PlaybackService),
            )
            return Futures.immediateFuture(LibraryResult.ofVoid(params))
        }

        override fun onGetSearchResult(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            query: String,
            page: Int,
            pageSize: Int,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> = paged(library.search(query), page, pageSize, params)

        /**
         * A browser asks to play items it only knows by id, or, for a spoken
         * request, by a search query. Ids are passed on as they are:
         * [BackendPlayer] hands the id to [play].
         */
        override fun onAddMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: List<MediaItem>,
        ): ListenableFuture<List<MediaItem>> {
            val first = mediaItems.firstOrNull() ?: return Futures.immediateFuture(mediaItems)
            val query = first.requestMetadata.searchQuery
            if (first.mediaId.isNotEmpty() || query == null) return Futures.immediateFuture(mediaItems)
            return Futures.transform(
                library.playableFor(query),
                { mediaId -> listOf(first.buildUpon().setMediaId(mediaId).build()) },
                MoreExecutors.directExecutor(),
            )
        }

        private fun paged(
            items: ListenableFuture<List<MediaItem>>,
            page: Int,
            pageSize: Int,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> = Futures.catching(
            Futures.transform(
                items,
                { all ->
                    val from = (page.toLong() * pageSize).coerceAtMost(all.size.toLong()).toInt()
                    val to = (from.toLong() + pageSize).coerceAtMost(all.size.toLong()).toInt()
                    LibraryResult.ofItemList(ImmutableList.copyOf(all.subList(from, to)), params)
                },
                MoreExecutors.directExecutor(),
            ),
            Exception::class.java,
            { e ->
                LibraryResult.ofError(
                    if (e is IllegalArgumentException) SessionError.ERROR_BAD_VALUE else SessionError.ERROR_IO,
                )
            },
            MoreExecutors.directExecutor(),
        )
    }

    private fun refreshQueue() {
        Futures.addCallback(
            library.queue(),
            object : FutureCallback<List<QueueSong>> {
                override fun onSuccess(songs: List<QueueSong>) = player.setQueue(songs)

                override fun onFailure(t: Throwable) = player.setQueue(emptyList())
            },
            ContextCompat.getMainExecutor(this),
        )
    }

    /**
     * The button controllers show next to the transport controls (Android
     * Auto's player screen, the notification): it displays the playback
     * [mode] and switches to the next one when pressed.
     */
    private fun playbackModeButton(mode: String): CommandButton {
        val (icon, name) = when (mode) {
            "Random" -> CommandButton.ICON_SHUFFLE_ON to "Shuffle"
            "LoopSingle" -> CommandButton.ICON_REPEAT_ONE to "Repeat song"
            "LoopQueue" -> CommandButton.ICON_REPEAT_ALL to "Repeat queue"
            else -> CommandButton.ICON_SHUFFLE_OFF to "Play in order"
        }
        return CommandButton.Builder(icon)
            .setDisplayName(name)
            .setSessionCommand(CYCLE_PLAYBACK_MODE)
            .build()
    }

    /** Tell the backend to play the browse-tree item with this id. */
    private fun play(mediaId: String) {
        if (mediaId == MediaLibrary.RESUME) {
            if (!client.state.value.isPlaying) client.togglePlay()
            return
        }
        Futures.addCallback(
            library.playCommand(mediaId),
            object : FutureCallback<String?> {
                override fun onSuccess(command: String?) {
                    if (command == null) {
                        Log.w(TAG, "Not playable: $mediaId")
                    } else if (!client.send(command)) {
                        Log.w(TAG, "Backend not connected, cannot play $mediaId")
                    }
                }

                override fun onFailure(t: Throwable) {
                    Log.w(TAG, "Cannot play $mediaId: ${t.message}")
                }
            },
            ContextCompat.getMainExecutor(this),
        )
    }

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
        library.close()
        wifiLock?.takeIf { it.isHeld }?.release()
        multicastLock?.takeIf { it.isHeld }?.release()
        scope.cancel()
        super.onDestroy()
    }

    private companion object {
        const val TAG = "rsplayer"

        /** Custom session command: switch to the next playback mode. */
        val CYCLE_PLAYBACK_MODE = SessionCommand("de.rsplayer.app.CYCLE_PLAYBACK_MODE", Bundle.EMPTY)
    }
}
