package com.subtitlevocabularylist.android

import android.app.Activity
import android.net.Uri
import android.view.View
import android.view.ViewGroup
import android.content.pm.ActivityInfo
import android.widget.FrameLayout
import androidx.activity.OnBackPressedCallback
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.Lifecycle
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.media3.common.*
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import app.tauri.annotation.InvokeArg
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import org.json.JSONArray
import org.json.JSONObject

@InvokeArg
class VideoArgs {
    var uri: String = ""
    var sessionId: String = ""
    var action: String = ""
    var text: String = ""
    var value: Double = 0.0
    var positionMs: Long = 0
    var speed: Float = 1f
    var x: Double = 0.0
    var y: Double = 0.0
    var width: Double = 0.0
    var height: Double = 0.0
    var ratio: Double = 1.0
    var kind: String = ""
    var title: String = ""
    var subtitleUri: String = ""
    var mime: String = ""
    var language: String = ""
    var audioKey: String = ""
    var subtitleKey: String = ""
    var select: Boolean = true
}

/** Rendering and track selection only. Learning state is owned by Rust. */
class VideoPlayer(private val activity: Activity) {
    private var player: ExoPlayer? = null
    private var view: PlayerView? = null
    private var sessionId = ""
    private var uri = ""
    private var error: String? = null
    private var audioKey = ""
    private var subtitleKey = ""
    private var selectionApplied = false
    private var fullscreen = false
    private var lastViewport: VideoArgs? = null
    private var revision = 0L
    private var userPaused = false
    private var pendingPause = ""
    private val back = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() { setFullscreen(false) }
    }
    init { (activity as AppCompatActivity).onBackPressedDispatcher.addCallback(activity as AppCompatActivity, back) }

    fun open(invoke: Invoke, foreground: Boolean) {
        val args = invoke.parseArgs(VideoArgs::class.java)
        require(Uri.parse(args.uri).scheme == "content")
        destroy()
        uri = args.uri; sessionId = args.sessionId; error = null
        revision++; userPaused = false; pendingPause = ""
        audioKey = args.audioKey; subtitleKey = args.subtitleKey; selectionApplied = false
        player = ExoPlayer.Builder(activity).build().apply {
            setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MOVIE).build(), true)
            setHandleAudioBecomingNoisy(true)
            trackSelectionParameters = trackSelectionParameters.buildUpon().setPreferredTextLanguage("en").build()
            addListener(object : Player.Listener {
                override fun onPlayerError(failure: PlaybackException) { if (sessionId == args.sessionId) error = "视频无法播放：${failure.errorCodeName}。请检查文件授权和编码。" }
                override fun onPlaybackSuppressionReasonChanged(reason: Int) { if (reason != Player.PLAYBACK_SUPPRESSION_REASON_NONE) pause() }
                override fun onPlayWhenReadyChanged(ready: Boolean, reason: Int) {
                    if (sessionId != args.sessionId) return
                    revision++
                    userPaused = !ready && reason == Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST && pendingPause != "internal"
                    pendingPause = ""
                }
                override fun onPositionDiscontinuity(old: Player.PositionInfo, new: Player.PositionInfo, reason: Int) { revision++ }
                override fun onTracksChanged(tracks: Tracks) {
                    revision++
                    if (!selectionApplied && tracks.groups.isNotEmpty()) {
                        selectionApplied = true
                        if (audioKey.isNotEmpty()) select(C.TRACK_TYPE_AUDIO, audioKey)
                        if (subtitleKey.isNotEmpty()) select(C.TRACK_TYPE_TEXT, subtitleKey)
                    }
                }
            })
            setMediaItem(MediaItem.fromUri(uri), args.positionMs)
            setPlaybackSpeed(args.speed.coerceIn(0.25f, 3f))
            prepare()
            if (foreground) play()
        }
        view = PlayerView(activity).apply {
            setBackgroundColor(android.graphics.Color.BLACK)
            player = this@VideoPlayer.player; setShowBuffering(PlayerView.SHOW_BUFFERING_WHEN_PLAYING)
            setFullscreenButtonClickListener { setFullscreen(it) }
        }
        val content = activity.findViewById<ViewGroup>(android.R.id.content)
        content.addView(view, FrameLayout.LayoutParams(1, 1))
        invoke.resolve(snapshot())
    }

    private fun trackKey(type: Int, group: TrackGroup, index: Int): String {
        val f = group.getFormat(index)
        return "$type:${f.id ?: group.id}:${f.sampleMimeType}:${f.language}:${f.label}"
    }

    fun snapshot(): JSObject {
        val tracks = JSONArray()
        player?.currentTracks?.groups?.forEach { group ->
            if (group.type == C.TRACK_TYPE_AUDIO || group.type == C.TRACK_TYPE_TEXT) {
                repeat(group.length) { i ->
                    val f = group.getTrackFormat(i)
                    tracks.put(JSObject().put("id", trackKey(group.type, group.mediaTrackGroup, i))
                        .put("kind", if (group.type == C.TRACK_TYPE_AUDIO) "audio" else "subtitle")
                        .put("title", f.label ?: f.language ?: f.sampleMimeType ?: "轨道").put("formatId", f.id ?: "")
                        .put("selected", group.isTrackSelected(i)).put("supported", group.isTrackSupported(i)))
                }
            }
        }
        return JSObject().put("sessionId", sessionId).put("uri", uri)
            .put("revision", revision).put("userPaused", userPaused).put("paused", player?.playWhenReady != true)
            .put("foreground", (activity as AppCompatActivity).lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED))
            .put("windowFocused", activity.hasWindowFocus())
            .put("fullscreen", fullscreen)
            .put("positionMs", player?.currentPosition?.coerceAtLeast(0) ?: 0)
            .put("durationMs", player?.duration?.coerceAtLeast(0) ?: 0)
            .put("playing", player?.isPlaying == true).put("ended", player?.playbackState == Player.STATE_ENDED)
            .put("speed", player?.playbackParameters?.speed ?: 1f).put("tracks", tracks)
            .put("error", error ?: JSONObject.NULL)
    }

    private fun select(type: Int, key: String): Boolean {
        val current = player ?: return false
        val builder = current.trackSelectionParameters.buildUpon().clearOverridesOfType(type)
        if (key == "off") { current.trackSelectionParameters = builder.setTrackTypeDisabled(type, true).build(); return true }
        current.currentTracks.groups.filter { it.type == type }.forEach { group ->
            repeat(group.length) { i ->
                val raw = key.removePrefix("raw:"); val formatId = group.getTrackFormat(i).id ?: ""
                if (trackKey(type, group.mediaTrackGroup, i) == key || (key.startsWith("raw:") && (formatId == raw || formatId.endsWith(":$raw")))) {
                    current.trackSelectionParameters = builder.setTrackTypeDisabled(type, false).addOverride(TrackSelectionOverride(group.mediaTrackGroup, listOf(i))).build()
                    return true
                }
            }
        }
        return false
    }

    fun state(invoke: Invoke) { invoke.resolve(snapshot()) }
    fun control(invoke: Invoke, foreground: Boolean) {
        val args = invoke.parseArgs(VideoArgs::class.java)
        if (args.sessionId != sessionId) { invoke.reject("当前视频已切换，请重新操作。", "conflict"); return }
        when (args.action) {
            "play" -> if (foreground) player?.apply { if (playbackState == Player.STATE_ENDED) seekTo(0); play() }
            "pause" -> pause(true)
            "seek" -> player?.seekTo(args.value.toLong().coerceAtLeast(0))
            "speed" -> player?.setPlaybackSpeed(args.value.toFloat().coerceIn(0.25f, 3f))
            "hide" -> { pause(); if (fullscreen) setFullscreen(false); view?.visibility = View.GONE }
            "fullscreen" -> setFullscreen(!fullscreen)
            "track" -> player?.let { current ->
                val type = if (args.kind == "audio") C.TRACK_TYPE_AUDIO else C.TRACK_TYPE_TEXT
                val key = args.text.ifEmpty { "off" }; require(select(type,key))
                if (type == C.TRACK_TYPE_AUDIO) audioKey = key else subtitleKey = key
            }
        }
        invoke.resolve(snapshot())
    }

    fun viewport(invoke: Invoke) {
        val args = invoke.parseArgs(VideoArgs::class.java)
        if (args.sessionId == sessionId) { lastViewport = args; if (!fullscreen) applyViewport(args) }
        invoke.resolve()
    }
    private fun applyViewport(args: VideoArgs) {
        view?.apply {
            if (args.width <= 0 || args.height <= 0) visibility = View.GONE
            else {
                layoutParams = FrameLayout.LayoutParams((args.width * args.ratio).toInt(), (args.height * args.ratio).toInt()).apply {
                    leftMargin = (args.x * args.ratio).toInt(); topMargin = (args.y * args.ratio).toInt()
                }
                visibility = View.VISIBLE
            }
        }
    }
    private fun setFullscreen(value: Boolean) {
        revision++
        fullscreen = value; back.isEnabled = value
        view?.setFullscreenButtonState(value)
        val bars = WindowInsetsControllerCompat(activity.window, activity.window.decorView)
        bars.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (value) bars.hide(WindowInsetsCompat.Type.systemBars()) else bars.show(WindowInsetsCompat.Type.systemBars())
        activity.findViewById<ViewGroup>(android.R.id.content).setBackgroundColor(if (value) android.graphics.Color.BLACK else android.graphics.Color.TRANSPARENT)
        activity.requestedOrientation = if (value) ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE else ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        if (value) view?.apply { layoutParams = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,ViewGroup.LayoutParams.MATCH_PARENT); visibility = View.VISIBLE }
        else lastViewport?.let { applyViewport(it) }
    }

    fun selectedAudio(): Pair<TrackGroup, Int>? {
        val group = player?.currentTracks?.groups?.find { it.type == C.TRACK_TYPE_AUDIO && it.isSelected } ?: return null
        return (0 until group.length).find { group.isTrackSelected(it) }?.let { Pair(group.mediaTrackGroup, it) }
    }
    fun subtitle(invoke: Invoke) {
        val args = invoke.parseArgs(VideoArgs::class.java)
        require(args.uri == uri && Uri.parse(args.subtitleUri).scheme == "content")
        val current = player ?: throw IllegalArgumentException("请先打开视频。")
        val item = current.currentMediaItem ?: throw IllegalArgumentException("视频尚未就绪。")
        val configs = item.localConfiguration!!.subtitleConfigurations.filter { it.id != args.subtitleUri }.toMutableList()
        configs.add(MediaItem.SubtitleConfiguration.Builder(Uri.parse(args.subtitleUri)).setId(args.subtitleUri)
            .setMimeType(args.mime).setLanguage(args.language).setLabel(args.title).build())
        val position = current.currentPosition; val playing = current.playWhenReady
        if (args.language == "en" && args.select) subtitleKey = "raw:${args.subtitleUri}"
        selectionApplied = false
        current.setMediaItem(item.buildUpon().setSubtitleConfigurations(configs).build(),position)
        current.prepare(); current.playWhenReady = playing
        invoke.resolve(snapshot())
    }
    fun pause(user: Boolean = false) { revision++; pendingPause = if (user) "user" else "internal"; userPaused = user; player?.pause() }
    fun destroy() { if (fullscreen) setFullscreen(false); (view?.parent as? ViewGroup)?.removeView(view); view?.player = null; view = null; player?.release(); player = null }
}
