package com.subtitlevocabularylist.android

import android.app.Activity
import android.net.Uri
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.DataReader
import androidx.media3.common.util.ParsableByteArray
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.extractor.*
import androidx.media3.extractor.text.CueDecoder
import androidx.media3.extractor.text.CuesWithTiming
import androidx.media3.extractor.text.DefaultSubtitleParserFactory
import androidx.media3.extractor.text.SubtitleParser
import app.tauri.plugin.JSObject
import org.json.JSONArray
import java.io.ByteArrayOutputStream
import java.security.MessageDigest

/** Media3 applies sample/subsample offsets before the transcoded cue reaches TrackOutput. */
class MediaIndex(private val activity: Activity) {
    private class TextTrack(val id: Int) : TrackOutput {
        var format: Format = Format.Builder().build()
        val samples = ByteArrayOutputStream()
        val cues = mutableListOf<CuesWithTiming>()
        override fun format(format: Format) { this.format = format }
        override fun sampleData(input: DataReader, length: Int, allowEndOfInput: Boolean, part: Int): Int {
            val bytes = ByteArray(minOf(length, 16384)); val count = input.read(bytes, 0, bytes.size)
            if (count > 0) { require(samples.size() + count <= 4 * 1024 * 1024); samples.write(bytes, 0, count) }
            return count
        }
        override fun sampleData(data: ParsableByteArray, length: Int, part: Int) {
            require(samples.size() + length <= 4 * 1024 * 1024)
            val bytes = ByteArray(length); data.readBytes(bytes, 0, length); samples.write(bytes)
        }
        override fun sampleMetadata(timeUs: Long, flags: Int, size: Int, offset: Int, crypto: TrackOutput.CryptoData?) {
            val bytes = samples.toByteArray(); val start = bytes.size - offset - size
            if (format.sampleMimeType == MimeTypes.APPLICATION_MEDIA3_CUES && size > 0) {
                require(cues.size < 100000)
                cues.add(CueDecoder().decode(timeUs, bytes, start, size))
            }
            samples.reset()
            if (offset > 0) samples.write(bytes, bytes.size - offset, offset)
        }
    }

    fun video(uri: Uri): JSObject {
        require(uri.scheme == "content")
        val digest = MessageDigest.getInstance("SHA-256")
        activity.contentResolver.openInputStream(uri)!!.use { stream ->
            val buffer = ByteArray(256 * 1024)
            while (true) { val n = stream.read(buffer); if (n < 0) break; digest.update(buffer, 0, n) }
        }
        val hash = digest.digest().joinToString("") { "%02x".format(it.toInt() and 255) }
        val source = DefaultDataSource.Factory(activity).createDataSource()
        var extractor: Extractor? = null
        val tracks = linkedMapOf<Int, TextTrack>()
        var durationUs = C.TIME_UNSET
        fun open(position: Long): DefaultExtractorInput {
            source.close()
            val remaining = source.open(DataSpec.Builder().setUri(uri).setPosition(position).build())
            return DefaultExtractorInput(source, position, if (remaining == C.LENGTH_UNSET.toLong()) C.LENGTH_UNSET.toLong() else position + remaining)
        }
        try {
            var input = open(0)
            for (candidate in DefaultExtractorsFactory().createExtractors(uri, emptyMap())) {
                val recognized = try { candidate.sniff(input) } catch (_: Exception) { false }
                input.resetPeekPosition()
                if (recognized) { extractor = candidate; break } else candidate.release()
            }
            val selected = extractor ?: throw IllegalArgumentException("无法读取此媒体的字幕索引。")
            selected.init(object : ExtractorOutput {
                override fun track(id: Int, type: Int): TrackOutput = if (type == C.TRACK_TYPE_TEXT)
                    tracks.getOrPut(id) { TextTrack(id) } else DiscardingTrackOutput()
                override fun endTracks() = Unit
                override fun seekMap(map: SeekMap) { durationUs = map.durationUs }
            })
            val position = PositionHolder()
            while (true) {
                when (selected.read(input, position)) {
                    Extractor.RESULT_END_OF_INPUT -> break
                    Extractor.RESULT_SEEK -> { input = open(position.position) }
                }
            }
        } finally { extractor?.release(); source.close() }
        val assets = JSONArray()
        tracks.values.forEach { track -> assets.put(asset(track.format, track.id.toString(), track.cues)) }
        return JSObject().put("sha256", hash).put("durationMs", if (durationUs > 0) durationUs / 1000 else 0)
            .put("subtitles", assets)
    }

    fun subtitle(uri: Uri, title: String): JSObject {
        val bytes = activity.contentResolver.openInputStream(uri)!!.use { stream ->
            val output = ByteArrayOutputStream(); val buffer = ByteArray(8192)
            while (true) { val n = stream.read(buffer); if (n < 0) break; require(output.size() + n <= 16 * 1024 * 1024); output.write(buffer, 0, n) }
            output.toByteArray()
        }
        require(bytes.size <= 16 * 1024 * 1024)
        val mime = when (title.substringAfterLast('.').lowercase()) {
            "srt" -> MimeTypes.APPLICATION_SUBRIP
            "ass", "ssa" -> MimeTypes.TEXT_SSA
            "vtt" -> MimeTypes.TEXT_VTT
            else -> throw IllegalArgumentException("请选择 SRT、ASS 或 WebVTT 文本字幕。")
        }
        val format = Format.Builder().setSampleMimeType(mime).setLabel(title).build()
        val parser = DefaultSubtitleParserFactory().create(format)
        val cues = mutableListOf<CuesWithTiming>()
        parser.parse(bytes, SubtitleParser.OutputOptions.allCues()) { cues.add(it) }
        return asset(format, uri.toString(), cues)
    }

    private fun asset(format: Format, id: String, values: List<CuesWithTiming>): JSObject {
        val segments = JSONArray()
        for (value in values.sortedBy { it.startTimeUs }) {
            val text = value.cues.mapNotNull { it.text?.toString() }.joinToString("\n").trim()
            if (text.isNotEmpty() && value.startTimeUs >= 0 && value.durationUs > 0) {
                segments.put(JSObject().put("locationKey", "cue:${segments.length()}").put("text", text)
                    .put("startMs", value.startTimeUs / 1000).put("endMs", value.endTimeUs / 1000))
            }
        }
        return JSObject().put("id", format.id ?: id).put("language", format.language ?: "und")
            .put("title", format.label ?: format.language ?: "文本字幕 $id")
            .put("mime", format.sampleMimeType ?: "").put("cues", segments)
    }
}
