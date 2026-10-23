package com.subtitlevocabularylist.android

import android.app.Activity
import android.os.Bundle
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.speech.tts.Voice
import app.tauri.annotation.InvokeArg
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import org.json.JSONArray
import java.io.File

@InvokeArg
class SpeechArgs {
    var text: String = ""
    var voiceId: String = ""
    var path: String = ""
    var taskId: String = ""
}

/** Offline engine only; Rust owns caching, task history and the review policy. */
class OfflineSpeech(private val activity: Activity) {
    private var engine: TextToSpeech? = null
    private var initialized = false
    private var unavailable = false
    private val waiting = mutableListOf<() -> Unit>()
    private data class Job(val args: SpeechArgs, val invoke: Invoke)
    private val jobs = linkedMapOf<String, Job>()
    private var active: String? = null

    private fun ready(action: () -> Unit) {
        if (initialized || unavailable) { action(); return }
        waiting.add(action)
        if (engine != null) return
        engine = TextToSpeech(activity.applicationContext) { status ->
            activity.runOnUiThread {
                initialized = status == TextToSpeech.SUCCESS
                unavailable = !initialized
                engine?.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                    override fun onStart(id: String?) = Unit
                    override fun onDone(id: String?) { finish(id, null) }
                    @Deprecated("Legacy engine callback")
                    override fun onError(id: String?) { finish(id, "英语语音生成失败，请检查已安装的离线语音包。") }
                    override fun onError(id: String?, errorCode: Int) { finish(id, "英语语音生成失败，请检查已安装的离线语音包。") }
                    override fun onStop(id: String?, interrupted: Boolean) { finish(id, "语音准备已取消。") }
                })
                val actions = waiting.toList()
                waiting.clear()
                actions.forEach { it() }
            }
        }
    }

    private fun voices(): List<Voice> = if (!initialized) emptyList() else
        engine?.voices.orEmpty().filter {
            it.locale.language == "en" && !it.isNetworkConnectionRequired &&
                !it.features.orEmpty().contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED)
        }.sortedBy { it.name }

    fun list(invoke: Invoke) = ready {
        try {
            val values = JSONArray()
            voices().forEach { values.put(JSObject().put("id", it.name).put("name", "${it.locale.displayName} · ${it.name}")) }
            invoke.resolve(JSObject().put("voices", values))
        } catch (_: Exception) { invoke.reject("无法读取离线英语声音，请检查系统语音设置。", "resource_missing") }
    }

    fun synthesize(invoke: Invoke) {
        val args = invoke.parseArgs(SpeechArgs::class.java)
        if (args.text.isBlank() || args.text.length > TextToSpeech.getMaxSpeechInputLength()) {
            invoke.reject("英语文本过长，请分段朗读。", "invalid_input"); return
        }
        val output = File(args.path).canonicalFile
        val jobsDirectory = File(activity.filesDir, "jobs").canonicalFile
        if (output.parentFile != jobsDirectory || !output.name.endsWith(".wav")) {
            invoke.reject("语音保存位置无效。", "invalid_input"); return
        }
        jobs[args.taskId] = Job(args, invoke)
        ready { next() }
    }

    private fun next() {
        if (active != null) return
        val job = jobs.values.firstOrNull() ?: return
        val voice = voices().find { it.name == job.args.voiceId }
        if (voice == null || engine?.setVoice(voice) != TextToSpeech.SUCCESS) {
            jobs.remove(job.args.taskId)
            job.invoke.reject("没有可用的离线英语声音。请在系统文字转语音设置中安装英语语音包；仍可继续词义练习和播放原声。", "resource_missing")
            next(); return
        }
        active = job.args.taskId
        val result = engine?.synthesizeToFile(job.args.text, Bundle(), File(job.args.path), job.args.taskId)
        if (result != TextToSpeech.SUCCESS) finish(job.args.taskId, "离线英语语音未能启动，请检查语音包。")
    }

    private fun finish(id: String?, error: String?) = activity.runOnUiThread {
        val job = jobs.remove(id) ?: return@runOnUiThread
        if (active == id) active = null
        if (error == null) job.invoke.resolve() else job.invoke.reject(error, "audio_unavailable")
        next()
    }

    fun cancel(invoke: Invoke) {
        val id = invoke.parseArgs(SpeechArgs::class.java).taskId
        val job = jobs.remove(id)
        if (active == id) { active = null; engine?.stop() }
        job?.invoke?.reject("语音准备已取消。", "cancelled")
        invoke.resolve()
        if (initialized || unavailable) next()
    }

    fun destroy() {
        jobs.values.forEach { it.invoke.reject("应用已关闭，语音准备中断。", "cancelled") }
        jobs.clear(); waiting.clear(); active = null
        engine?.stop(); engine?.shutdown(); engine = null
    }
}
