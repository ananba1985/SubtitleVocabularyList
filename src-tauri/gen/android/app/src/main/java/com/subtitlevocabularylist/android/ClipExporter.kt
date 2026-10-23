package com.subtitlevocabularylist.android

import android.app.Activity
import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.TrackSelectionOverride
import androidx.media3.common.TrackGroup
import androidx.media3.common.util.Clock
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.exoplayer.trackselection.DefaultTrackSelector
import androidx.media3.exoplayer.trackselection.TrackSelector
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
}

class ClipExporter(private val activity: Activity, private val video: VideoPlayer) {
    private var transformer: Transformer? = null
    private var pending: Invoke? = null
    fun start(invoke: Invoke) {
        val args = invoke.parseArgs(ExportArgs::class.java)
        require(transformer == null && Uri.parse(args.uri).scheme == "content")
        require(args.uri == video.snapshot().getString("uri")) { "请先打开对应视频，再截取原声。" }
        require(args.startMs >= 0 && args.endMs > args.startMs)
        val output = File(args.path).canonicalFile
        require(output.parentFile == File(activity.filesDir, "jobs").canonicalFile && output.extension == "m4a")
        val audio = video.selectedAudio() ?: throw IllegalArgumentException("当前没有可截取音轨。")
        if (!video.snapshot().getBoolean("paused")) video.pause()
        // ExoPlayer prefixes the base source's group/format IDs with 0: when external
        // subtitle sources are merged. Transformer reads the base video by itself.
        val merged = audio.first.getFormat(audio.second).id?.startsWith("0:") == true
        val exportGroup = if (merged) TrackGroup(audio.first.id.removePrefix("0:"),
            *(0 until audio.first.length).map { audio.first.getFormat(it).buildUpon().setId(audio.first.getFormat(it).id?.removePrefix("0:")).build() }.toTypedArray()) else audio.first
        val factory = TrackSelector.Factory { context -> DefaultTrackSelector(context).apply {
            parameters = buildUponParameters().addOverride(TrackSelectionOverride(exportGroup, listOf(audio.second))).build()
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
