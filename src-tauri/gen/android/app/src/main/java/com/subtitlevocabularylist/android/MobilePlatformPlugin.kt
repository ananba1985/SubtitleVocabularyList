package com.subtitlevocabularylist.android

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.util.concurrent.Executors

/** One Android adapter; business data and rules belong to the shared Rust core. */
@TauriPlugin
class MobilePlatformPlugin(private val activity: Activity) : Plugin(activity) {
    private val io = Executors.newSingleThreadExecutor()

    @Command
    fun prepareResources(invoke: Invoke) {
        io.execute {
            try {
                OfflineAssets.install(activity)
                invoke.resolve(JSObject().put("ready", true))
            } catch (error: Exception) {
                invoke.reject("离线词库安装失败，请重试或重新安装应用。", "resource_missing", error)
            }
        }
    }
}
