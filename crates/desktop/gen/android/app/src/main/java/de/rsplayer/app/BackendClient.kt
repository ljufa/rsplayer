package de.rsplayer.app

import android.os.SystemClock
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/** Library scan as the backend reports it. */
data class ScanState(
    val status: Status = Status.UNKNOWN,
    /** Listing / Reading / Removing. */
    val phase: String = "",
    val done: Int = 0,
    /** 0 while unknown (listing). */
    val total: Int = 0,
    /** Folder or file being worked on, or a status like "Connection lost…". */
    val current: String = "",
) {
    /** UNKNOWN until the backend has said whether a scan runs. */
    enum class Status { UNKNOWN, RUNNING, IDLE }
}

/** What the media session and the scan service need to know about the backend. */
data class BackendState(
    val connected: Boolean = false,
    /** PLAYING / PAUSED / STOPPED / ERROR, as the backend reports it. */
    val playbackState: String = "STOPPED",
    val title: String? = null,
    val artist: String? = null,
    val album: String? = null,
    val artworkUrl: String? = null,
    val durationMs: Long = 0,
    val positionMs: Long = 0,
    /** [SystemClock.elapsedRealtime] when [positionMs] was received. */
    val positionAtElapsedMs: Long = 0,
    val scan: ScanState = ScanState(),
) {
    val isPlaying: Boolean get() = playbackState == "PLAYING"
    val hasSong: Boolean get() = title != null || durationMs > 0

    /** Position extrapolated from the last progress event while playing. */
    fun currentPositionMs(): Long {
        if (!isPlaying || positionAtElapsedMs == 0L) return positionMs
        val extrapolated = positionMs + (SystemClock.elapsedRealtime() - positionAtElapsedMs)
        return if (durationMs > 0) extrapolated.coerceAtMost(durationMs) else extrapolated
    }
}

/**
 * WebSocket client for the in-process backend. Commands are the same JSON
 * `UserCommand` frames the web UI sends; state comes back as
 * `StateChangeEvent` frames. Reconnects with backoff — the backend starts a
 * little after the process does.
 */
class BackendClient(private val scope: CoroutineScope) {
    private val _state = MutableStateFlow(BackendState())
    val state: StateFlow<BackendState> = _state

    private val http = OkHttpClient.Builder()
        .pingInterval(20, TimeUnit.SECONDS)
        .build()

    @Volatile
    private var socket: WebSocket? = null
    @Volatile
    private var closed = false
    private var backoffMs = 500L

    fun start() = connect()

    fun close() {
        closed = true
        socket?.close(1000, "service stopped")
        socket = null
    }

    fun togglePlay() = send(TOGGLE_PLAY)
    fun next() = send("""{"Player":"Next"}""")
    fun prev() = send("""{"Player":"Prev"}""")
    fun stop() = send("""{"Player":"Stop"}""")
    fun seek(seconds: Int) = send("""{"Player":{"Seek":$seconds}}""")
    fun setVolume(percent: Int) = send("""{"System":{"SetVol":${percent.coerceIn(0, 100)}}}""")

    fun send(json: String): Boolean {
        val s = socket ?: return false
        return s.send(json)
    }

    private fun connect() {
        if (closed) return
        val request = Request.Builder().url(BackendConfig.wsUrl).build()
        socket = http.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                Log.i(TAG, "Connected to backend at ${BackendConfig.wsUrl}")
                backoffMs = 500L
                _state.update { it.copy(connected = true) }
                webSocket.send(QUERY_PLAYER_INFO)
                webSocket.send(QUERY_CURRENT_SONG)
                // Answered with progress while a scan runs, else the last summary.
                webSocket.send(QUERY_SCAN_STATUS)
            }

            override fun onMessage(webSocket: WebSocket, text: String) = handle(text)

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                scheduleReconnect()
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                scheduleReconnect()
            }
        })
    }

    private fun scheduleReconnect() {
        socket = null
        _state.update { it.copy(connected = false) }
        if (closed) return
        val wait = backoffMs
        backoffMs = (backoffMs * 2).coerceAtMost(5_000L)
        scope.launch {
            delay(wait)
            connect()
        }
    }

    private fun handle(text: String) {
        // Unit variants arrive as bare JSON strings.
        if (text == "\"MetadataSongScanStarted\"") {
            _state.update { it.copy(scan = ScanState(status = ScanState.Status.RUNNING, phase = "Listing")) }
            return
        }
        val event = try {
            JSONObject(text)
        } catch (_: Exception) {
            return
        }
        when {
            event.has("MetadataScanProgress") -> {
                val p = event.getJSONObject("MetadataScanProgress")
                _state.update {
                    it.copy(
                        scan = ScanState(
                            status = ScanState.Status.RUNNING,
                            phase = p.optString("phase"),
                            done = p.optInt("done"),
                            total = p.optInt("total"),
                            current = p.optString("current"),
                        ),
                    )
                }
            }
            event.has("MetadataSongScanFinished") || event.has("MetadataScanSummary") -> {
                _state.update { it.copy(scan = ScanState(status = ScanState.Status.IDLE)) }
            }
            event.has("PlaybackStateEvent") -> {
                val value = event.get("PlaybackStateEvent")
                val playback = if (value is JSONObject) "ERROR" else value.toString()
                _state.update { it.copy(playbackState = playback) }
            }
            event.has("CurrentSongEvent") -> {
                val song = event.getJSONObject("CurrentSongEvent")
                val file = song.optString("file", "")
                val title = song.optString("title").ifBlank { file.substringAfterLast('/').ifBlank { null } }
                val artwork = song.optString("image_url").ifBlank { null }
                    ?: song.optString("image_id").ifBlank { null }?.let { "${BackendConfig.baseUrl}/artwork/$it" }
                _state.update {
                    it.copy(
                        title = title,
                        artist = song.optString("artist").ifBlank { null },
                        album = song.optString("album").ifBlank { null },
                        artworkUrl = artwork,
                        durationMs = song.optJSONObject("time")?.let(::durationMs) ?: 0,
                        positionMs = 0,
                        positionAtElapsedMs = SystemClock.elapsedRealtime(),
                    )
                }
            }
            event.has("SongTimeEvent") -> {
                val progress = event.getJSONObject("SongTimeEvent")
                _state.update {
                    it.copy(
                        durationMs = progress.optJSONObject("total_time")?.let(::durationMs) ?: it.durationMs,
                        positionMs = progress.optJSONObject("current_time")?.let(::durationMs) ?: it.positionMs,
                        positionAtElapsedMs = SystemClock.elapsedRealtime(),
                    )
                }
            }
        }
    }

    /** serde's `Duration` shape: `{"secs": u64, "nanos": u32}`. */
    private fun durationMs(d: JSONObject): Long = d.optLong("secs") * 1000 + d.optLong("nanos") / 1_000_000

    companion object {
        private const val TAG = "rsplayer"
        const val TOGGLE_PLAY = """{"Player":"TogglePlay"}"""
        const val QUERY_PLAYER_INFO = """{"Player":"QueryCurrentPlayerInfo"}"""
        const val QUERY_CURRENT_SONG = """{"Queue":"QueryCurrentSong"}"""
        const val QUERY_SCAN_STATUS = """{"Metadata":"QueryScanStatus"}"""
        const val RESCAN_METADATA = """{"Metadata":{"RescanMetadata":["",false]}}"""

        /** Open a throwaway connection, send one command, close. */
        fun fireAndForget(json: String) {
            val client = OkHttpClient()
            val request = Request.Builder().url(BackendConfig.wsUrl).build()
            client.newWebSocket(request, object : WebSocketListener() {
                private var sent = false

                override fun onOpen(webSocket: WebSocket, response: Response) {
                    sent = webSocket.send(json)
                    webSocket.close(1000, null)
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    // The server drops the socket right after our close; only a
                    // failure before the frame went out is worth reporting.
                    if (!sent) Log.w(TAG, "fireAndForget failed: ${t.message}")
                }
            })
        }
    }
}
