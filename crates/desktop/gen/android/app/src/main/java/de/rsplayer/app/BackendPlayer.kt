package de.rsplayer.app

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.net.Uri
import android.os.Looper
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.UnstableApi
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture

/**
 * A media3 [Player] facade over the Rust backend: state is whatever the
 * backend last reported over the WebSocket, and every control forwards a
 * command to it. media3 drives the notification, lock-screen controls,
 * Bluetooth/headset buttons and the MediaSession from this.
 *
 * Audio focus is handled here too — the backend plays through AAudio without
 * requesting focus, so this is what pauses us for calls and other apps.
 */
@UnstableApi
class BackendPlayer(
    context: Context,
    looper: Looper,
    private val client: BackendClient,
) : SimpleBasePlayer(looper) {

    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var focusRequest: AudioFocusRequest? = null
    private var pausedByFocusLoss = false
    private var current = BackendState()

    private val focusListener = AudioManager.OnAudioFocusChangeListener { change ->
        when (change) {
            AudioManager.AUDIOFOCUS_LOSS -> {
                pausedByFocusLoss = false
                if (current.isPlaying) client.togglePlay()
                abandonFocus()
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT,
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> {
                if (current.isPlaying) {
                    pausedByFocusLoss = true
                    client.togglePlay()
                }
            }
            AudioManager.AUDIOFOCUS_GAIN -> {
                if (pausedByFocusLoss && !current.isPlaying) client.togglePlay()
                pausedByFocusLoss = false
            }
        }
    }

    /** Called on the player looper whenever the backend state changes. */
    fun update(state: BackendState) {
        val wasPlaying = current.isPlaying
        current = state
        if (state.isPlaying && !wasPlaying && focusRequest == null) {
            // Playback started from the web UI: hold focus for it.
            requestFocus()
        }
        invalidateState()
    }

    /** Pause if playing (headphones unplugged, etc.). */
    fun pauseIfPlaying() {
        if (current.isPlaying) client.togglePlay()
    }

    override fun getState(): State {
        val commands = Player.Commands.Builder()
            .addAll(
                Player.COMMAND_PLAY_PAUSE,
                Player.COMMAND_PREPARE,
                Player.COMMAND_STOP,
                Player.COMMAND_SEEK_TO_NEXT,
                Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
                Player.COMMAND_SEEK_TO_PREVIOUS,
                Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
                Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM,
                Player.COMMAND_GET_CURRENT_MEDIA_ITEM,
                Player.COMMAND_GET_METADATA,
                Player.COMMAND_GET_TIMELINE,
                Player.COMMAND_GET_AUDIO_ATTRIBUTES,
                Player.COMMAND_RELEASE,
            )
            .build()

        val builder = State.Builder()
            .setAvailableCommands(commands)
            .setPlayWhenReady(current.isPlaying, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)

        if (current.connected && current.hasSong) {
            val metadata = MediaMetadata.Builder()
                .setTitle(current.title)
                .setArtist(current.artist)
                .setAlbumTitle(current.album)
                .setArtworkUri(current.artworkUrl?.let(Uri::parse))
                .build()
            val item = MediaItemData.Builder("current")
                .setMediaItem(MediaItem.Builder().setMediaId("current").setMediaMetadata(metadata).build())
                .setMediaMetadata(metadata)
                .setDurationUs(if (current.durationMs > 0) current.durationMs * 1000 else androidx.media3.common.C.TIME_UNSET)
                .setIsSeekable(current.durationMs > 0)
                .build()
            builder
                .setPlaylist(listOf(item))
                .setCurrentMediaItemIndex(0)
                .setPlaybackState(Player.STATE_READY)
                .setContentPositionMs { current.currentPositionMs() }
        } else {
            builder.setPlaybackState(Player.STATE_IDLE)
        }
        return builder.build()
    }

    override fun handlePrepare(): ListenableFuture<*> = Futures.immediateVoidFuture()

    override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
        if (playWhenReady) {
            pausedByFocusLoss = false
            if (requestFocus() && !current.isPlaying) client.togglePlay()
        } else {
            pausedByFocusLoss = false
            if (current.isPlaying) client.togglePlay()
            abandonFocus()
        }
        return Futures.immediateVoidFuture()
    }

    override fun handleStop(): ListenableFuture<*> {
        client.stop()
        abandonFocus()
        return Futures.immediateVoidFuture()
    }

    override fun handleRelease(): ListenableFuture<*> {
        abandonFocus()
        return Futures.immediateVoidFuture()
    }

    override fun handleSeek(mediaItemIndex: Int, positionMs: Long, seekCommand: Int): ListenableFuture<*> {
        when (seekCommand) {
            Player.COMMAND_SEEK_TO_NEXT, Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM -> client.next()
            Player.COMMAND_SEEK_TO_PREVIOUS, Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM -> client.prev()
            else -> if (positionMs >= 0) client.seek((positionMs / 1000).toInt())
        }
        return Futures.immediateVoidFuture()
    }

    private fun requestFocus(): Boolean {
        if (focusRequest != null) return true
        val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build()
            )
            .setOnAudioFocusChangeListener(focusListener)
            .build()
        val granted = audioManager.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        if (granted) focusRequest = request else Log.w(TAG, "Audio focus denied")
        return granted
    }

    private fun abandonFocus() {
        focusRequest?.let { audioManager.abandonAudioFocusRequest(it) }
        focusRequest = null
    }

    private companion object {
        const val TAG = "rsplayer"
    }
}
