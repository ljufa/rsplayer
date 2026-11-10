package de.rsplayer.app

import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.MoreExecutors
import com.google.common.util.concurrent.SettableFuture
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.net.URLDecoder
import java.net.URLEncoder
import java.text.Collator
import java.util.Locale
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** One entry of the play queue. */
data class QueueSong(
    val file: String,
    val title: String,
    val artist: String?,
    val album: String?,
    val durationMs: Long,
)

/**
 * The browse tree offered to media browsers (Android Auto), read from the
 * backend's `GET /api/browse/...` endpoints, and the backend command that
 * plays a tapped item.
 *
 * Media ids are `kind|part|part`, each part URL-encoded:
 * - `root`, then the tabs `tab|playlists`, `tab|artists`, `tab|radio`, `tab|podcasts`
 * - `playlist|<id>` holds `plsong|<playlist id>|<song file>`
 * - `albums|added` and `albums|released` (newest albums, under Playlists) hold `album|<id>`
 * - `artist|<name>` holds `album|<id>`
 * - `album|<id>` holds `alsong|<album id>|<song file>`
 * - `radio|<station id>`
 * - `podcast|<id>` holds `episode|<id>`
 * - `song|<song file>`: a search result, played on its own
 * - `queue|<song file>`: an entry of the play queue (see [queue])
 * - `resume`: continue whatever was playing
 */
class MediaLibrary {
    private val http = OkHttpClient.Builder()
        .connectTimeout(2, TimeUnit.SECONDS)
        .readTimeout(10, TimeUnit.SECONDS)
        .build()
    private val retryTimer = Executors.newSingleThreadScheduledExecutor()

    /** Station JSON by id, as last listed: playing a station needs all of it. */
    private val stations = ConcurrentHashMap<String, JSONObject>()

    /** The last search and its result, so paging through it does not search again. */
    @Volatile
    private var lastSearch: Pair<String, ListenableFuture<List<MediaItem>>>? = null

    fun close() {
        retryTimer.shutdownNow()
    }

    val root: MediaItem = folder(ROOT, "RSPlayer")

    private val tabs = listOf(
        folder(id(TAB, "playlists"), "Playlists", MediaMetadata.MEDIA_TYPE_FOLDER_PLAYLISTS),
        folder(id(TAB, "artists"), "Artists", MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS),
        folder(id(TAB, "radio"), "Radio", MediaMetadata.MEDIA_TYPE_FOLDER_RADIO_STATIONS),
        folder(id(TAB, "podcasts"), "Podcasts", MediaMetadata.MEDIA_TYPE_FOLDER_PODCASTS),
    )

    /** Children of a browsable item; fails with [IllegalArgumentException] for an unknown id. */
    fun children(parentId: String): ListenableFuture<List<MediaItem>> {
        val parts = split(parentId)
        return when (parts[0]) {
            ROOT -> Futures.immediateFuture(tabs)
            TAB -> when (parts.getOrNull(1)) {
                "playlists" -> Futures.transform(
                    list("playlists") { playlist(it) },
                    { playlists -> playlists + newestAlbums },
                    MoreExecutors.directExecutor(),
                )
                "artists" -> Futures.transform(
                    get("artists"),
                    // One alphabetical list: Android Auto adds its own A-Z
                    // jump button to long sorted lists.
                    { artists -> strings(artists).sortedWith(alphabetical()).map(::artist) },
                    MoreExecutors.directExecutor(),
                )
                "radio" -> list("radio") { station(it) }
                "podcasts" -> list("podcasts") { podcast(it) }
                else -> unknown(parentId)
            }
            PLAYLIST -> list("playlist_songs", "id" to parts[1]) { song(it, PLAYLIST_SONG, parts[1]) }
            ALBUMS -> list("albums", "sort" to parts[1]) { album(it, withArtist = true) }
            ARTIST -> list("albums", "artist" to parts[1]) { album(it) }
            ALBUM -> list("songs", "id" to parts[1]) { song(it, ALBUM_SONG, parts[1]) }
            PODCAST -> list("episodes", "id" to parts[1]) { episode(it) }
            else -> unknown(parentId)
        }
    }

    /**
     * Artists, albums and songs matching [query], in that order. The result
     * of the latest query is kept, see [lastSearch].
     */
    fun search(query: String): ListenableFuture<List<MediaItem>> {
        lastSearch?.takeIf { it.first == query }?.let { return it.second }
        val result = Futures.transform(
            getObject("search", "q" to query),
            { found ->
                strings(found.getJSONArray("artists")).map(::artist) +
                    objects(found.getJSONArray("albums")).map { album(it, withArtist = true) } +
                    objects(found.getJSONArray("songs")).map { song(it, SONG) }
            },
            MoreExecutors.directExecutor(),
        )
        lastSearch = query to result
        return result
    }

    /**
     * The media id to play for a spoken request ("play ... on RSPlayer"):
     * the best song for [query], or [RESUME] when nothing was asked for or
     * found.
     */
    fun playableFor(query: String): ListenableFuture<String> {
        if (query.isBlank()) return Futures.immediateFuture(RESUME)
        return Futures.catching(
            Futures.transform(
                search(query),
                { found -> found.firstOrNull { it.mediaMetadata.isPlayable == true }?.mediaId ?: RESUME },
                MoreExecutors.directExecutor(),
            ),
            Exception::class.java,
            { RESUME },
            MoreExecutors.directExecutor(),
        )
    }

    /** The play queue from the current song onwards. */
    fun queue(): ListenableFuture<List<QueueSong>> = Futures.transform(
        get("queue"),
        { songs ->
            objects(songs).map { json ->
                val file = json.getString("file")
                QueueSong(
                    file = file,
                    title = json.optString("title").ifBlank { file.substringAfterLast('/') },
                    artist = json.optString("artist").ifBlank { null },
                    album = json.optString("album").ifBlank { null },
                    durationMs = json.optJSONObject("time")?.optLong("secs")?.times(1000) ?: 0,
                )
            }
        },
        MoreExecutors.directExecutor(),
    )

    /**
     * The backend command (a JSON `UserCommand`) that plays this item, or
     * null when the id is not playable ([RESUME] is handled by the caller).
     */
    fun playCommand(mediaId: String): ListenableFuture<String?> {
        val parts = split(mediaId)
        fun command(group: String, name: String, value: Any): String =
            JSONObject().put(group, JSONObject().put(name, value)).toString()
        return when (parts[0]) {
            PLAYLIST_SONG -> Futures.immediateFuture(
                command("Queue", "LoadPlaylistInQueueFrom", JSONArray().put(parts[1]).put(parts[2])),
            )
            ALBUM_SONG -> Futures.immediateFuture(
                command("Queue", "LoadAlbumInQueueFrom", JSONArray().put(parts[1]).put(parts[2])),
            )
            EPISODE -> Futures.immediateFuture(command("Podcast", "PlayEpisode", parts[1]))
            SONG -> Futures.immediateFuture(command("Queue", "AddSongAndPlay", parts[1]))
            QUEUE -> Futures.immediateFuture(command("Player", "PlayItem", parts[1]))
            RADIO -> {
                val known = stations[parts[1]]
                if (known != null) {
                    Futures.immediateFuture(command("Player", "PlayRadioStation", known))
                } else {
                    // Not listed in this process yet (the browser remembered the id).
                    Futures.transform(
                        list("radio") { station(it) },
                        { stations[parts[1]]?.let { command("Player", "PlayRadioStation", it) } },
                        MoreExecutors.directExecutor(),
                    )
                }
            }
            else -> Futures.immediateFuture(null)
        }
    }

    private fun playlist(json: JSONObject): MediaItem =
        folder(id(PLAYLIST, json.getString("id")), json.getString("name"), MediaMetadata.MEDIA_TYPE_PLAYLIST)

    private val newestAlbums = listOf(
        folder(id(ALBUMS, "added"), "Recently added", MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS),
        folder(id(ALBUMS, "released"), "Latest releases", MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS),
    )

    private fun artist(name: String): MediaItem = folder(id(ARTIST, name), name, MediaMetadata.MEDIA_TYPE_ARTIST)

    /** Subtitle: the release year, preceded by the artist outside an artist's own list. */
    private fun album(json: JSONObject, withArtist: Boolean = false): MediaItem {
        val year = json.optString("released").take(4).takeIf { it.length == 4 }
        val artist = json.optString("artist").takeIf { withArtist && it.isNotBlank() }
        val subtitle = listOfNotNull(artist, year).joinToString(" · ").ifBlank { null }
        return folder(id(ALBUM, json.getString("id")), json.getString("title"), MediaMetadata.MEDIA_TYPE_ALBUM, subtitle)
    }

    private fun podcast(json: JSONObject): MediaItem = folder(
        id(PODCAST, json.getString("id")),
        json.getString("title"),
        MediaMetadata.MEDIA_TYPE_PODCAST,
        json.optString("author").ifBlank { null },
    )

    /** [containerId]: the playlist or album the song is listed in, none for a search result. */
    private fun song(json: JSONObject, kind: String, containerId: String? = null): MediaItem {
        val file = json.getString("file")
        val title = json.optString("title").ifBlank { file.substringAfterLast('/') }
        return playable(
            if (containerId == null) id(kind, file) else id(kind, containerId, file),
            title,
            json.optString("artist").ifBlank { null },
            MediaMetadata.MEDIA_TYPE_MUSIC,
        )
    }

    private fun station(json: JSONObject): MediaItem {
        val stationId = json.getString("id")
        stations[stationId] = json
        return playable(id(RADIO, stationId), json.getString("name"), null, MediaMetadata.MEDIA_TYPE_RADIO_STATION)
    }

    private fun episode(json: JSONObject): MediaItem = playable(
        id(EPISODE, json.getString("id")),
        json.getString("title"),
        json.optString("published").take(10).ifBlank { null },
        MediaMetadata.MEDIA_TYPE_PODCAST_EPISODE,
    )

    private fun list(
        endpoint: String,
        vararg query: Pair<String, String>,
        convert: (JSONObject) -> MediaItem,
    ): ListenableFuture<List<MediaItem>> = Futures.transform(
        get(endpoint, *query),
        { array -> objects(array).map(convert) },
        MoreExecutors.directExecutor(),
    )

    private fun get(endpoint: String, vararg query: Pair<String, String>): ListenableFuture<JSONArray> =
        Futures.transform(fetch(endpoint, *query), { JSONArray(it) }, MoreExecutors.directExecutor())

    private fun getObject(endpoint: String, vararg query: Pair<String, String>): ListenableFuture<JSONObject> =
        Futures.transform(fetch(endpoint, *query), { JSONObject(it) }, MoreExecutors.directExecutor())

    /**
     * `GET /api/browse/<endpoint>`, the response body. Connection failures
     * are retried for a few seconds: a browser can ask before the backend has
     * finished starting.
     */
    private fun fetch(endpoint: String, vararg query: Pair<String, String>): ListenableFuture<String> {
        val result = SettableFuture.create<String>()
        fun attempt(remaining: Int) {
            // Built per attempt: the port is only final once the backend runs.
            val url = "${BackendConfig.baseUrl}/api/browse/$endpoint".toHttpUrl().newBuilder()
                .apply { query.forEach { (name, value) -> addQueryParameter(name, value) } }
                .build()
            http.newCall(Request.Builder().url(url).build()).enqueue(object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    if (remaining > 0 && !retryTimer.isShutdown) {
                        retryTimer.schedule({ attempt(remaining - 1) }, RETRY_DELAY_MS, TimeUnit.MILLISECONDS)
                    } else {
                        Log.w(TAG, "Browse $endpoint failed: ${e.message}")
                        result.setException(e)
                    }
                }

                override fun onResponse(call: Call, response: Response) {
                    response.use {
                        try {
                            if (!it.isSuccessful) throw IOException("HTTP ${it.code}")
                            result.set(it.body!!.string())
                        } catch (e: Exception) {
                            Log.w(TAG, "Browse $endpoint failed: ${e.message}")
                            result.setException(e)
                        }
                    }
                }
            })
        }
        attempt(RETRIES)
        return result
    }

    private fun <T> unknown(mediaId: String): ListenableFuture<T> =
        Futures.immediateFailedFuture(IllegalArgumentException("Unknown media id $mediaId"))

    companion object {
        private const val TAG = "rsplayer"
        private const val RETRIES = 20
        private const val RETRY_DELAY_MS = 500L

        const val ROOT = "root"
        const val RESUME = "resume"
        private const val ALBUMS = "albums"
        private const val SONG = "song"
        private const val QUEUE = "queue"
        /** Media id of a play-queue entry. */
        fun queueId(file: String): String = id(QUEUE, file)

        /** Ignores case and accents, so "Đorđe" sorts with D and "Økse" with O. */
        private fun alphabetical(): Comparator<String> {
            val collator = Collator.getInstance(Locale.ROOT).apply { strength = Collator.PRIMARY }
            return Comparator { a, b -> collator.compare(a, b) }
        }

        private fun strings(array: JSONArray): List<String> = (0 until array.length()).map { array.getString(it) }

        private fun objects(array: JSONArray): List<JSONObject> = (0 until array.length()).map { array.getJSONObject(it) }
        private const val TAB = "tab"
        private const val PLAYLIST = "playlist"
        private const val PLAYLIST_SONG = "plsong"
        private const val ARTIST = "artist"
        private const val ALBUM = "album"
        private const val ALBUM_SONG = "alsong"
        private const val RADIO = "radio"
        private const val PODCAST = "podcast"
        private const val EPISODE = "episode"

        private fun id(vararg parts: String): String = parts.joinToString("|") { URLEncoder.encode(it, "UTF-8") }

        private fun split(mediaId: String): List<String> = mediaId.split('|').map { URLDecoder.decode(it, "UTF-8") }

        private fun folder(
            mediaId: String,
            title: String,
            type: Int = MediaMetadata.MEDIA_TYPE_FOLDER_MIXED,
            subtitle: String? = null,
        ): MediaItem = item(mediaId, title, subtitle, type, browsable = true)

        private fun playable(mediaId: String, title: String, subtitle: String?, type: Int): MediaItem =
            item(mediaId, title, subtitle, type, browsable = false)

        private fun item(mediaId: String, title: String, subtitle: String?, type: Int, browsable: Boolean): MediaItem =
            MediaItem.Builder()
                .setMediaId(mediaId)
                .setMediaMetadata(
                    MediaMetadata.Builder()
                        .setTitle(title)
                        .setArtist(subtitle)
                        .setSubtitle(subtitle)
                        .setIsBrowsable(browsable)
                        .setIsPlayable(!browsable)
                        .setMediaType(type)
                        .build(),
                )
                .build()
    }
}
