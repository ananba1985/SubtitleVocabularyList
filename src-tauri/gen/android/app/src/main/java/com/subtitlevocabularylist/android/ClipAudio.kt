package com.subtitlevocabularylist.android

import android.app.Activity
import android.net.Uri
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.analytics.AnalyticsListener
import app.tauri.annotation.InvokeArg
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import org.json.JSONObject
import java.io.File

@InvokeArg
class AudioArgs {
    var path: String = ""
    var sessionId: String = ""
    var action: String = ""
    var positionMs: Long? = null
}

/** One foreground player, with Media3 audio focus and headphone handling. */
class ClipAudio(private val activity: Activity) {
    private var player: ExoPlayer? = null
    private var sessionId = ""
    private var error: String? = null
    private var played = false

    private fun snapshot(id: String): JSObject {
        val current = if (id == sessionId) player else null
        return JSObject().put("sessionId", id)
            .put("active", current != null)
            .put("positionMs", current?.currentPosition?.coerceAtLeast(0) ?: 0)
            .put("durationMs", current?.duration?.coerceAtLeast(0) ?: 0)
            .put("playing", current?.isPlaying == true)
            .put("played", current != null && played)
            .put("ended", current == null || current.playbackState == Player.STATE_ENDED)
            .put("error", if (id == sessionId) error ?: JSONObject.NULL else JSONObject.NULL)
    }

    fun open(invoke: Invoke, foreground: Boolean) {
        val args = invoke.parseArgs(AudioArgs::class.java)
        val file = File(args.path).canonicalFile
        val root = activity.filesDir.canonicalFile
        require(file.isFile && (file.path.startsWith(File(root, "media").path + File.separator) ||
            file.path.startsWith(File(root, "jobs").path + File.separator)))
        player?.release()
        sessionId = args.sessionId; error = null; played = false
        player = ExoPlayer.Builder(activity).build().apply {
            setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_SPEECH).build(), true)
            setHandleAudioBecomingNoisy(true)
            addAnalyticsListener(object : AnalyticsListener {
                override fun onAudioPositionAdvancing(eventTime: AnalyticsListener.EventTime, playoutStartSystemTimeMs: Long) {
                    if (sessionId == args.sessionId) played = true
                }
            })
            addListener(object : Player.Listener {
                override fun onPlayerError(failure: PlaybackException) {
                    if (sessionId == args.sessionId) error = "音频无法播放，请检查已保存的原声或重新准备语音。"
                }
                override fun onPlaybackSuppressionReasonChanged(reason: Int) {
                    if (reason != Player.PLAYBACK_SUPPRESSION_REASON_NONE && sessionId == args.sessionId) pause()
                }
            })
            setMediaItem(MediaItem.fromUri(Uri.fromFile(file)))
            prepare()
            if (foreground) play()
        }
        invoke.resolve(snapshot(args.sessionId))
    }

    fun state(invoke: Invoke) { invoke.resolve(snapshot(invoke.parseArgs(AudioArgs::class.java).sessionId)) }

    fun control(invoke: Invoke) {
        val args = invoke.parseArgs(AudioArgs::class.java)
        if (args.sessionId == sessionId) when (args.action) {
            "play" -> player?.apply { if (playbackState == Player.STATE_ENDED) seekTo(0); play() }
            "pause" -> player?.pause()
            "seek" -> player?.seekTo((args.positionMs ?: 0).coerceAtLeast(0))
            "close" -> { player?.release(); player = null }
        }
        invoke.resolve(snapshot(args.sessionId))
    }

    fun pause() { player?.pause() }
    fun destroy() { player?.release(); player = null }
}
