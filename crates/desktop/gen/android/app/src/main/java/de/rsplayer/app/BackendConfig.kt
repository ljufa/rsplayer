package de.rsplayer.app

import android.content.Context
import android.os.Environment
import android.system.Os
import android.util.Log
import java.io.File
import java.io.IOException
import java.net.InetSocketAddress
import java.net.ServerSocket

/**
 * Hands the Rust backend its environment before it starts and tells the
 * Kotlin side where the backend ended up.
 *
 * The Rust wrapper honours `PORT` if free (else probes upwards) and writes
 * the port it finally bound to `<dataDir>/backend.port`; [port] prefers that
 * file, so a stale proposal never strands the media service.
 */
object BackendConfig {
    private const val TAG = "rsplayer"
    private const val PORT_FILE = "backend.port"

    @Volatile
    private var proposedPort: Int = 0
    lateinit var dataDir: File
        private set

    @Synchronized
    fun ensureInitialized(context: Context) {
        if (proposedPort != 0) return
        val appContext = context.applicationContext
        dataDir = File(appContext.filesDir, "rsplayer").apply { mkdirs() }
        // Drop a port file left over from a previous process so we never
        // connect to a dead port while the backend is still starting.
        File(dataDir, PORT_FILE).delete()
        proposedPort = pickFreePort(8001)
        val musicDir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_MUSIC)

        Os.setenv("PORT", proposedPort.toString(), true)
        Os.setenv("RSPLAYER_DESKTOP", "1", true)
        Os.setenv("RSPLAYER_DATA_DIR", dataDir.absolutePath, true)
        Os.setenv("RSPLAYER_DEFAULT_MUSIC_DIR", musicDir.absolutePath, true)
        if (Os.getenv("RUST_LOG") == null) {
            Os.setenv("RUST_LOG", "info", true)
        }
        Log.i(TAG, "Backend env: port=$proposedPort dataDir=$dataDir musicDir=$musicDir")
    }

    /** The port the backend is (or will be) listening on. */
    val port: Int
        get() {
            val fromFile = runCatching { File(dataDir, PORT_FILE).readText().trim().toInt() }.getOrNull()
            return fromFile ?: proposedPort
        }

    val baseUrl: String get() = "http://127.0.0.1:$port"
    val wsUrl: String get() = "ws://127.0.0.1:$port/api/ws"

    private fun pickFreePort(preferred: Int): Int {
        for (candidate in preferred..9000) {
            try {
                ServerSocket().use {
                    it.reuseAddress = false
                    it.bind(InetSocketAddress("127.0.0.1", candidate))
                    return candidate
                }
            } catch (_: IOException) {
                // taken, try the next one
            }
        }
        ServerSocket(0).use { return it.localPort }
    }
}
