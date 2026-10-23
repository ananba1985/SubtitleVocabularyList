package com.subtitlevocabularylist.android

import android.app.Activity
import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.C
import androidx.media3.common.util.Clock
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.exoplayer.trackselection.DefaultTrackSelector
import androidx.media3.exoplayer.trackselection.TrackSelector
import androidx.media3.exoplayer.trackselection.MappingTrackSelector
import androidx.media3.exoplayer.trackselection.ExoTrackSelection
import androidx.media3.transformer.*
import app.tauri.annotation.InvokeArg
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import java.io.File

@InvokeArg
class ExportArgs {
    var uri: String = ""
    var path: String = ""
    var startMs: Long = 0
    var endMs: Long = 0
    var audioTrackId: String = ""
}

class ClipExporter(private val activity: Activity, private val video: VideoPlayer) {
    private var transformer: Transformer? = null
    private var pending: Invoke? = null
    fun start(invoke: Invoke) {
        val args = invoke.parseArgs(ExportArgs::class.java)
        require(transformer == null && Uri.parse(args.uri).scheme == "content")
        require(args.startMs >= 0 && args.endMs > args.startMs)
        val output = File(args.path).canonicalFile
        require(output.parentFile == File(activity.filesDir, "jobs").canonicalFile && output.extension == "m4a")
        val current = video.snapshot()
        val selected = if (current.getString("uri") == args.uri) video.selectedAudio() else null
        val liveId = selected?.let { it.first.getFormat(it.second).id }
        val desired = args.audioTrackId.ifBlank { liveId ?: "" }.removePrefix("0:")
        if (current.getString("uri") == args.uri && !current.getBoolean("paused")) video.pause()
        val factory = TrackSelector.Factory { context -> object : DefaultTrackSelector(context) {
            override fun selectAudioTrack(mapped: MappingTrackSelector.MappedTrackInfo, support: Array<Array<IntArray>>, mixed: IntArray, parameters: DefaultTrackSelector.Parameters): android.util.Pair<ExoTrackSelection.Definition,Int>? {
                if (desired.isBlank()) return super.selectAudioTrack(mapped,support,mixed,parameters)
                for (renderer in 0 until mapped.rendererCount) if (mapped.getRendererType(renderer) == C.TRACK_TYPE_AUDIO) {
                    val groups = mapped.getTrackGroups(renderer)
                    for (g in 0 until groups.length) for (track in 0 until groups[g].length) {
                        if (groups[g].getFormat(track).id?.removePrefix("0:") == desired)
                            return android.util.Pair(ExoTrackSelection.Definition(groups[g],track),renderer)
                    }
                }
                throw IllegalArgumentException("所选音轨已变化，请重新选择后截取。")
            }
        } }
        val loader = DefaultAssetLoaderFactory(activity, DefaultDecoderFactory.Builder(activity).build(), Clock.DEFAULT,
            null, DataSourceBitmapLoader(activity), factory)
        val item = MediaItem.Builder().setUri(args.uri).setClippingConfiguration(MediaItem.ClippingConfiguration.Builder()
            .setStartPositionMs(args.startMs).setEndPositionMs(args.endMs).build()).build()
        pending = invoke
        transformer = Transformer.Builder(activity).setAssetLoaderFactory(loader).setAudioMimeType(MimeTypes.AUDIO_AAC)
            .setMuxerFactory(InAppMp4Muxer.Factory().setAttemptStreamableOutputEnabled(false))
            .setEncoderFactory(DefaultEncoderFactory.Builder(activity).setRequestedAudioEncoderSettings(
                AudioEncoderSettings.Builder().setBitrate(128000).build()).build())
            .addListener(object : Transformer.Listener {
                override fun onCompleted(composition: Composition, result: ExportResult) {
                    pending?.resolve(JSObject().put("durationMs", result.durationMs)); pending = null; transformer = null
                }
                override fun onError(composition: Composition, result: ExportResult, error: ExportException) {
                    pending?.reject("原声截取失败，请核对所选音轨和区间。", "audio_unavailable"); pending = null; transformer = null
                }
            }).build()
        transformer!!.start(EditedMediaItem.Builder(item).setRemoveVideo(true).build(), output.path)
    }
    fun destroy() { transformer?.cancel(); pending?.reject("原声截取中断，已收录资料保留。", "cancelled"); pending = null; transformer = null }
}
