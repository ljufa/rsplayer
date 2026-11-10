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
import android.os.Process
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.CommandButton
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaConstants
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
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull

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
    private lateinit var openApp: PendingIntent
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

        openApp = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        session = MediaLibrarySession.Builder(this, player, LibraryCallback())
            .setSessionActivity(openApp)
            // A message for the user (see phoneActionNeeded) must not turn a
            // session that may be playing into a failed one.
            .setLibraryErrorReplicationMode(MediaLibrarySession.LIBRARY_ERROR_REPLICATION_MODE_NON_FATAL)
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
     * Android Auto.
     */
    private inner class LibraryCallback : MediaLibrarySession.Callback {
        /**
         * The service is exported so that Android Auto can bind to it, which
         * lets any installed app try. Only these may browse the library and
         * control playback: this app, Android Auto and Android Automotive,
         * and what the system itself trusts with media sessions (System UI,
         * Bluetooth, the assistant, enabled notification listeners).
         */
        override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult {
            val allowed = controller.uid == Process.myUid() ||
                controller.isTrusted ||
                session.isMediaNotificationController(controller) ||
                session.isAutoCompanionController(controller) ||
                session.isAutomotiveController(controller)
            if (!allowed) {
                Log.w(TAG, "Rejected media controller ${controller.packageName} (uid ${controller.uid})")
                return MediaSession.ConnectionResult.reject()
            }
            return MediaSession.ConnectionResult.AcceptedResultBuilder(session)
                .setAvailableSessionCommands(
                    MediaSession.ConnectionResult.DEFAULT_SESSION_AND_LIBRARY_COMMANDS.buildUpon()
                        .add(CYCLE_PLAYBACK_MODE)
                        .build(),
                )
                .build()
        }

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
            val children = library.children(parentId)
            if (parentId != MediaLibrary.ARTISTS_TAB || PermissionGate.hasAudioAccess(this@PlaybackService)) {
                return paged(children, page, pageSize, params)
            }
            // No artists and no access to the phone's music: say why. (With
            // access denied the tab can still have artists, from a remote library.)
            return Futures.transformAsync(
                children,
                { artists ->
                    if (artists.isEmpty()) {
                        Futures.immediateFuture(
                            phoneActionNeeded("Allow RSPlayer to access your music: open the app on your phone", params),
                        )
                    } else {
                        paged(children, page, pageSize, params)
                    }
                },
                MoreExecutors.directExecutor(),
            )
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
         * "Play" with nothing loaded: a media button, the system's resume
         * card, or a browser asking for the most recent item. The backend
         * remembers what it played last, so this resumes it.
         */
        override fun onPlaybackResumption(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
        ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> = Futures.immediateFuture(
            MediaSession.MediaItemsWithStartPosition(
                listOf(MediaItem.Builder().setMediaId(MediaLibrary.RESUME).build()),
                0,
                C.TIME_UNSET,
            ),
        )

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
                if (e is IllegalArgumentException) {
                    LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
                } else {
                    // The backend did not answer, even after the retries.
                    phoneActionNeeded("RSPlayer is not ready: open the app on your phone", params)
                }
            },
            MoreExecutors.directExecutor(),
        )

        /**
         * A failed browse request with a [message] for the user. Android Auto
         * shows it, with a button that opens the app on the phone, instead
         * of an empty list. Of the session errors media3 passes on to such
         * browsers, "authentication expired" is the one meaning "the user has
         * to do something in the app"; the next successful request clears it.
         */
        private fun <T : Any> phoneActionNeeded(message: String, params: LibraryParams?): LibraryResult<T> {
            val extras = Bundle().apply {
                putString(MediaConstants.EXTRAS_KEY_ERROR_RESOLUTION_ACTION_LABEL_COMPAT, "Open RSPlayer")
                putParcelable(MediaConstants.EXTRAS_KEY_ERROR_RESOLUTION_ACTION_INTENT_COMPAT, openApp)
            }
            val error = SessionError(SessionError.ERROR_SESSION_AUTHENTICATION_EXPIRED, message, extras)
            return if (params == null) LibraryResult.ofError(error) else LibraryResult.ofError(error, params)
        }
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

    /**
     * Tell the backend to play the browse-tree item with this id. Right
     * after a start without an activity the backend may not be connected
     * yet, so wait for it, bounded.
     */
    private fun play(mediaId: String) {
        scope.launch {
            val connected = withTimeoutOrNull(BACKEND_WAIT_MS) { client.state.first { it.connected } }
            if (connected == null) {
                Log.w(TAG, "Backend not connected, cannot play $mediaId")
                return@launch
            }
            if (mediaId == MediaLibrary.RESUME) {
                // Give a backend that has just started the time to report
                // whether it already plays (auto-resume): a toggle would pause it.
                if (!connected.isPlaying) delay(STATE_SETTLE_MS)
                if (!client.state.value.isPlaying) client.togglePlay()
                return@launch
            }
            val command = try {
                await(library.playCommand(mediaId))
            } catch (e: Exception) {
                Log.w(TAG, "Cannot play $mediaId: ${e.message}")
                return@launch
            }
            if (command == null) Log.w(TAG, "Not playable: $mediaId") else client.send(command)
        }
    }

    private suspend fun <T> await(future: ListenableFuture<T>): T = suspendCancellableCoroutine { continuation ->
        Futures.addCallback(
            future,
            object : FutureCallback<T> {
                override fun onSuccess(result: T) = continuation.resumeWith(Result.success(result))

                override fun onFailure(t: Throwable) = continuation.resumeWith(Result.failure(t))
            },
            MoreExecutors.directExecutor(),
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
        const val BACKEND_WAIT_MS = 15_000L
        const val STATE_SETTLE_MS = 500L

        /** Custom session command: switch to the next playback mode. */
        val CYCLE_PLAYBACK_MODE = SessionCommand("de.rsplayer.app.CYCLE_PLAYBACK_MODE", Bundle.EMPTY)
    }
}
