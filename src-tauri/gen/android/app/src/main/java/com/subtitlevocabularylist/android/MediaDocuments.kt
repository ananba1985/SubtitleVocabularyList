package com.subtitlevocabularylist.android

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import app.tauri.plugin.JSObject
import org.json.JSONArray

object MediaDocuments {
    fun intent(kind: String): Intent = (if(kind=="folder")Intent(Intent.ACTION_OPEN_DOCUMENT_TREE) else Intent(Intent.ACTION_OPEN_DOCUMENT)).apply {
        if(kind!="folder"){addCategory(Intent.CATEGORY_OPENABLE)
        type = when (kind) { "video" -> "video/*"; "image" -> "image/*"; else -> "*/*" }
        putExtra(Intent.EXTRA_ALLOW_MULTIPLE, kind == "video")
        }
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
    }

    fun picked(activity: Activity, data: Intent?): JSObject {
        val uris = mutableListOf<Uri>()
        data?.clipData?.let { clips -> repeat(clips.itemCount) { uris.add(clips.getItemAt(it).uri) } }
            ?: data?.data?.let { uris.add(it) }
        val files = JSONArray()
        if(data?.data!=null && data.clipData==null && android.provider.DocumentsContract.isTreeUri(data.data!!)){
            val tree=data.data!!;activity.contentResolver.takePersistableUriPermission(tree,Intent.FLAG_GRANT_READ_URI_PERMISSION)
            val parent=android.provider.DocumentsContract.getTreeDocumentId(tree)
            val children=android.provider.DocumentsContract.buildChildDocumentsUriUsingTree(tree,parent)
            activity.contentResolver.query(children,arrayOf(android.provider.DocumentsContract.Document.COLUMN_DOCUMENT_ID,android.provider.DocumentsContract.Document.COLUMN_DISPLAY_NAME,android.provider.DocumentsContract.Document.COLUMN_MIME_TYPE),null,null,null)?.use{cursor->
                while(cursor.moveToNext()){
                    val name=cursor.getString(1) ?: "";val mime=cursor.getString(2) ?: ""
                    if(mime.startsWith("video/") || name.substringAfterLast('.').lowercase() in listOf("mp4","mkv","webm","mov","m4v")){
                        require(files.length()<100){"目录视频超过 100 个，请选择较小的同剧目录。"}
                        val uri=android.provider.DocumentsContract.buildDocumentUriUsingTree(tree,cursor.getString(0));files.put(describe(activity,uri))
                    }
                }
            }
            return JSObject().put("files",files)
        }
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
