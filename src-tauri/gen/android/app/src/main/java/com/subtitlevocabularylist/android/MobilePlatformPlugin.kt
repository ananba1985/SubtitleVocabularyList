package com.subtitlevocabularylist.android

import android.app.Activity
import android.content.Intent
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.util.Base64
import androidx.activity.result.ActivityResult
import androidx.appcompat.app.AppCompatActivity
import app.tauri.annotation.Command
import app.tauri.annotation.ActivityCallback
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
@InvokeArg
class CopyArgs { var text: String = "" }

/** One Android adapter; business data and rules belong to the shared Rust core. */
@TauriPlugin
class MobilePlatformPlugin(private val activity: Activity) : Plugin(activity) {
    private val io = Executors.newSingleThreadExecutor()
    private val speech = OfflineSpeech(activity)
    private val audio = ClipAudio(activity)
    private val video = VideoPlayer(activity)
    private val exporter = ClipExporter(activity, video)
    @Volatile private var foreground = true
    private val rotation=object:android.content.BroadcastReceiver(){override fun onReceive(context:android.content.Context,intent:Intent){video.interruptLearning()}}
    init {androidx.core.content.ContextCompat.registerReceiver(activity,rotation,android.content.IntentFilter("com.subtitlevocabularylist.android.CONFIGURATION_CHANGED"),androidx.core.content.ContextCompat.RECEIVER_NOT_EXPORTED)}

    override fun onResume(activity: AppCompatActivity) { foreground = true }
    override fun onPause(activity: AppCompatActivity) { foreground = false; audio.pause(); video.pause() }

    override fun onDestroy(activity: AppCompatActivity) { activity.unregisterReceiver(rotation);exporter.destroy(); video.destroy(); audio.destroy(); speech.destroy(); io.shutdown() }

    private fun sound(invoke: Invoke, action: () -> Unit) = activity.runOnUiThread {
        try { action() } catch (_: Exception) { invoke.reject("手机系统接口暂不可用，请检查本机资源或文件授权。", "provider_unavailable") }
    }

    @Command fun speechVoices(invoke: Invoke) = sound(invoke) { speech.list(invoke) }
    @Command fun copyText(invoke:Invoke) = sound(invoke) {
        val text=invoke.parseArgs(CopyArgs::class.java).text
        require(text.length<=100000)
        (activity.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager).setPrimaryClip(android.content.ClipData.newPlainText("SubtitleVocabularyList",text))
        invoke.resolve()
    }
    @Command fun videoRenderSubtitle(invoke:Invoke) = sound(invoke) { video.renderSubtitle(invoke) }
    @Command fun speechSynthesize(invoke: Invoke) = sound(invoke) { speech.synthesize(invoke) }
    @Command fun speechCancel(invoke: Invoke) = sound(invoke) { speech.cancel(invoke) }
    @Command fun speechSettingsOpen(invoke: Invoke) = sound(invoke) {
        try { activity.startActivity(Intent("com.android.settings.TTS_SETTINGS")) }
        catch (_: Exception) { activity.startActivity(Intent(android.provider.Settings.ACTION_ACCESSIBILITY_SETTINGS)) }
        invoke.resolve()
    }
    @Command fun audioOpen(invoke: Invoke) = sound(invoke) { video.pause(); audio.open(invoke, foreground) }
    @Command fun audioState(invoke: Invoke) = sound(invoke) { audio.state(invoke) }
    @Command fun audioControl(invoke: Invoke) = sound(invoke) { audio.control(invoke) }
    @Command fun videoOpen(invoke: Invoke) = sound(invoke) { audio.pause(); video.open(invoke, foreground) }
    @Command fun videoState(invoke: Invoke) = sound(invoke) { video.state(invoke) }
    @Command fun videoControl(invoke: Invoke) = sound(invoke) { if (foreground) audio.pause(); video.control(invoke, foreground) }
    @Command fun videoViewport(invoke: Invoke) = sound(invoke) { video.viewport(invoke) }
    @Command fun videoSubtitle(invoke: Invoke) = sound(invoke) { video.subtitle(invoke) }
    @Command fun videoExport(invoke: Invoke) = sound(invoke) { exporter.start(invoke) }
    @Command fun incomingInputs(invoke: Invoke) = sound(invoke) { invoke.resolve(JSObject().put("inputs",IncomingInputs.list(activity))) }
    @Command fun incomingAck(invoke: Invoke) = sound(invoke) { IncomingInputs.ack(activity,invoke.parseArgs(ImageArgs::class.java).id); invoke.resolve() }
    @Command fun prepareImage(invoke: Invoke) {
        val args = invoke.parseArgs(ImageArgs::class.java)
        io.execute {
            try { invoke.resolve(MobileImages.prepare(activity,args)) }
            catch (_: Exception) { invoke.reject("图片无法读取，请重新选择或分享图片。", "resource_missing") }
        }
    }
    @Command fun videoFrame(invoke: Invoke) = sound(invoke) { video.capture(invoke,io) }
    @Command fun describeDocument(invoke: Invoke) {
        val args = invoke.parseArgs(ImageArgs::class.java)
        io.execute { try { invoke.resolve(MediaDocuments.describe(activity,Uri.parse(args.uri))) } catch (_:Exception) { invoke.reject("文件授权已失效，请通过文件选择器重新选择。","resource_missing") } }
    }

    @Command fun pickDocuments(invoke: Invoke) {
        val args = invoke.parseArgs(VideoArgs::class.java)
        startActivityForResult(invoke, MediaDocuments.intent(args.kind), "documentsPicked")
    }
    @ActivityCallback fun documentsPicked(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) { invoke.resolve(JSObject().put("files", org.json.JSONArray())); return }
        io.execute {
            try { invoke.resolve(MediaDocuments.picked(activity, result.data)) }
            catch (_: Exception) { invoke.reject("文件读取授权未能保存，请重新选择本地文件。", "resource_missing") }
        }
    }
    @Command fun mediaIndex(invoke: Invoke) {
        val args = invoke.parseArgs(VideoArgs::class.java)
        io.execute {
            try { invoke.resolve(MediaIndex(activity).video(Uri.parse(args.uri))) }
            catch (_: Exception) { invoke.reject("字幕索引未完成，视频仍可播放；可选择外置文本字幕。", "invalid_data") }
        }
    }
    @Command fun subtitleRead(invoke: Invoke) {
        val args = invoke.parseArgs(VideoArgs::class.java)
        io.execute {
            try { invoke.resolve(MediaIndex(activity).subtitle(Uri.parse(args.uri), args.title)) }
            catch (_: Exception) { invoke.reject("字幕读取失败，请选择有效的 SRT、ASS 或 WebVTT 文件。", "invalid_data") }
        }
    }

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
            // Reachability of the configured site is decided by the actual HTTP request.
            val connected = capabilities?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true
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
