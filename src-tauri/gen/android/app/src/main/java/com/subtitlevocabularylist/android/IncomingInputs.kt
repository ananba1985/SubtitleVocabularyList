package com.subtitlevocabularylist.android

import android.app.Activity
import android.content.Intent
import android.net.Uri
import app.tauri.plugin.JSObject
import org.json.JSONArray
import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.UUID

/** Durable platform handoff only. Confirmation and vocabulary persistence belong to Rust. */
object IncomingInputs {
    private fun file(activity: Activity) = File(activity.filesDir,"incoming-inputs.json")
    @Synchronized fun list(activity: Activity): JSONArray = file(activity).takeIf { it.isFile }?.let { JSONArray(it.readText()) } ?: JSONArray()
    private fun save(activity: Activity, values: JSONArray) {
        val target = file(activity); val temporary = File(target.parentFile,"incoming-inputs.json.tmp")
        temporary.writeText(values.toString())
        Files.move(temporary.toPath(),target.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING)
    }
    @Synchronized fun ack(activity: Activity,id: String) {
        val before = list(activity); val after = JSONArray()
        for (i in 0 until before.length()) { val item = before.getJSONObject(i); if (item.getString("id") != id) after.put(item) }
        save(activity,after)
    }
    @Suppress("DEPRECATION")
    @Synchronized fun receive(activity: Activity,intent: Intent?) {
        if (intent == null || intent.getBooleanExtra("svl.received",false)) return
        val action = intent.action
        if (action !in listOf(Intent.ACTION_SEND,Intent.ACTION_PROCESS_TEXT,Intent.ACTION_VIEW)) return
        intent.putExtra("svl.received",true)
        val values = list(activity)
        val text = (if (action == Intent.ACTION_PROCESS_TEXT) intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT) else intent.getCharSequenceExtra(Intent.EXTRA_TEXT))?.toString()
        val uri = intent.data ?: intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM) ?: intent.clipData?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.uri
        if (!text.isNullOrBlank()) {
            values.put(JSObject().put("id",UUID.randomUUID().toString()).put("kind","text").put("text",text).put("uri","").put("title","系统文字分享"))
        } else if (uri?.scheme == "content") {
            val mime = intent.type ?: activity.contentResolver.getType(uri) ?: ""
            val kind = if (mime.startsWith("image/")) "image" else if (mime.startsWith("video/")) "video" else return
            if (intent.flags and Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION != 0) {
                try { activity.contentResolver.takePersistableUriPermission(uri,Intent.FLAG_GRANT_READ_URI_PERMISSION) } catch (_: Exception) { }
            }
            values.put(JSObject().put("id",UUID.randomUUID().toString()).put("kind",kind).put("text","").put("uri",uri.toString()).put("title",if(kind=="image") "系统图片分享" else "系统视频分享"))
        } else return
        save(activity,values)
    }
}
