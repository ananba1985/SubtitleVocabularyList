package com.subtitlevocabularylist.android

import android.app.Activity
import android.content.Intent
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.util.Base64
import androidx.appcompat.app.AppCompatActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.util.concurrent.Executors

@InvokeArg
class CredentialArgs { var data: String = ""; var protect: Boolean = false }
@InvokeArg
class BrowserArgs { var url: String = "" }

/** One Android adapter; business data and rules belong to the shared Rust core. */
@TauriPlugin
class MobilePlatformPlugin(private val activity: Activity) : Plugin(activity) {
    private val io = Executors.newSingleThreadExecutor()
    @Volatile private var foreground = true

    override fun onResume(activity: AppCompatActivity) { foreground = true }
    override fun onPause(activity: AppCompatActivity) { foreground = false }

    @Command
    fun credentialTransform(invoke: Invoke) {
        io.execute {
            try {
                val args = invoke.parseArgs(CredentialArgs::class.java)
                val output = CredentialCipher.transform(Base64.decode(args.data, Base64.NO_WRAP), args.protect)
                invoke.resolve(JSObject().put("data", Base64.encodeToString(output, Base64.NO_WRAP)))
            } catch (_: Exception) {
                invoke.reject("手机受保护凭据不可用，请重新连接账号。", "credentials_unavailable")
            }
        }
    }

    @Command
    fun openBrowser(invoke: Invoke) {
        try {
            val uri = Uri.parse(invoke.parseArgs(BrowserArgs::class.java).url)
            require(uri.scheme == "https" && !uri.host.isNullOrBlank() && uri.userInfo == null)
            activity.startActivity(Intent(Intent.ACTION_VIEW, uri).addCategory(Intent.CATEGORY_BROWSABLE))
            invoke.resolve()
        } catch (_: Exception) {
            invoke.reject("无法打开系统浏览器，请检查浏览器设置。", "provider_unavailable")
        }
    }

    @Command
    fun networkState(invoke: Invoke) {
        try {
            val manager = activity.getSystemService(ConnectivityManager::class.java)
            val capabilities = manager.getNetworkCapabilities(manager.activeNetwork)
            val connected = capabilities?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true
            invoke.resolve(JSObject().put("connected", connected).put("metered", manager.isActiveNetworkMetered).put("foreground", foreground))
        } catch (_: Exception) {
            invoke.reject("无法读取手机网络状态。", "provider_unavailable")
        }
    }

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
