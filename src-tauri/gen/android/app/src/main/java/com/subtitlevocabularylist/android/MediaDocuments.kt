package com.subtitlevocabularylist.android

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import app.tauri.plugin.JSObject
import org.json.JSONArray

object MediaDocuments {
    fun intent(kind: String): Intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = when (kind) { "video" -> "video/*"; "image" -> "image/*"; else -> "*/*" }
        putExtra(Intent.EXTRA_ALLOW_MULTIPLE, kind == "video")
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
    }

    fun picked(activity: Activity, data: Intent?): JSObject {
        val uris = mutableListOf<Uri>()
        data?.clipData?.let { clips -> repeat(clips.itemCount) { uris.add(clips.getItemAt(it).uri) } }
            ?: data?.data?.let { uris.add(it) }
        val files = JSONArray()
        for (uri in uris.distinct()) {
            activity.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            files.put(describe(activity, uri))
        }
        return JSObject().put("files", files)
    }

    fun describe(activity: Activity, uri: Uri): JSObject {
        require(uri.scheme == "content")
        var title = "本地媒体"; var size = 0L
        activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use {
            if (it.moveToFirst()) { title = it.getString(0) ?: title; if (!it.isNull(1)) size = it.getLong(1) }
        }
        return JSObject().put("uri", uri.toString()).put("title", title).put("size", size)
            .put("mime", activity.contentResolver.getType(uri) ?: "application/octet-stream")
    }
}
