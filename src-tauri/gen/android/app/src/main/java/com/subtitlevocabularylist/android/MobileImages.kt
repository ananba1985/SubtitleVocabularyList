package com.subtitlevocabularylist.android

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import app.tauri.annotation.InvokeArg
import app.tauri.plugin.JSObject
import java.io.File
import java.util.UUID

@InvokeArg
class ImageArgs { var uri:String=""; var id:String="" }

object MobileImages {
    fun prepare(activity: Activity,args: ImageArgs): JSObject {
        val uri = Uri.parse(args.uri); require(uri.scheme == "content")
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        activity.contentResolver.openInputStream(uri)!!.use { BitmapFactory.decodeStream(it,null,bounds) }
        require(bounds.outWidth > 0 && bounds.outHeight > 0)
        var scale = 1
        while (bounds.outWidth.toLong() * bounds.outHeight / scale / scale > 16_000_000) scale *= 2
        val options = BitmapFactory.Options().apply { inSampleSize = scale }
        val bitmap = activity.contentResolver.openInputStream(uri)!!.use { BitmapFactory.decodeStream(it,null,options) } ?: throw IllegalArgumentException("图片无法解码。")
        return save(activity,bitmap,args.id.ifBlank { UUID.randomUUID().toString() },if(scale>1) "图片（已按处理尺寸缩放）" else "图片")
    }
    fun save(activity: Activity,bitmap: Bitmap,id:String,origin:String): JSObject {
        require(runCatching { UUID.fromString(id) }.isSuccess)
        val root = File(activity.filesDir,"jobs"); root.mkdirs()
        val target = File(root,"image-$id.png"); val temporary = File(root,"image-$id.png.tmp")
        val width=bitmap.width; val height=bitmap.height
        try {
            temporary.outputStream().use { require(bitmap.compress(Bitmap.CompressFormat.PNG,100,it)); it.fd.sync() }
            java.nio.file.Files.move(temporary.toPath(),target.toPath(),java.nio.file.StandardCopyOption.ATOMIC_MOVE,java.nio.file.StandardCopyOption.REPLACE_EXISTING)
        } finally { bitmap.recycle(); temporary.delete() }
        return JSObject().put("id",id).put("path",target.path).put("width",width).put("height",height).put("origin",origin)
    }
}
