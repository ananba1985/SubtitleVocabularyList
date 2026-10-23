package com.subtitlevocabularylist.android

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest

/** Installs only versioned, generic resources. Never opens the personal SQLite DB. */
internal object OfflineAssets {
    private fun digest(bytes: ByteArray) = MessageDigest.getInstance("SHA-256")
        .digest(bytes).joinToString("") { "%02x".format(it) }

    fun install(context: Context) {
        val manifest = context.assets.open("svl/manifest.json").use { it.readBytes() }
        val version = digest(manifest)
        val resources = JSONObject(String(manifest, Charsets.UTF_8)).getJSONArray("files")
        val root = context.filesDir.canonicalFile
        val marker = File(root, "resources-installed.sha256")
        val current = marker.isFile && marker.readText() == version
        for (index in 0 until resources.length()) {
            val entry = resources.getJSONObject(index)
            val path = entry.getString("path")
            val target = File(root, path).canonicalFile
            require(target.path.startsWith(root.path + File.separator)) { "Invalid resource path" }
            val size = entry.getLong("size")
            if (current && target.isFile && target.length() == size) continue
            check(target.parentFile!!.mkdirs() || target.parentFile!!.isDirectory)
            val temporary = File(target.parentFile, target.name + ".installing")
            try {
                val hash = MessageDigest.getInstance("SHA-256")
                var copied = 0L
                context.assets.open("svl/$path").use { source ->
                    temporary.outputStream().use { output ->
                        val buffer = ByteArray(128 * 1024)
                        while (true) {
                            val count = source.read(buffer)
                            if (count < 0) break
                            output.write(buffer, 0, count)
                            hash.update(buffer, 0, count)
                            copied += count
                        }
                        output.fd.sync()
                    }
                }
                val actual = hash.digest().joinToString("") { "%02x".format(it) }
                check(copied == size && actual == entry.getString("sha256")) { "Resource checksum mismatch" }
                Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            } finally {
                temporary.delete()
            }
        }
        val nextMarker = File(root, "resources-installed.sha256.installing")
        nextMarker.writeText(version)
        Files.move(nextMarker.toPath(), marker.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
    }
}
