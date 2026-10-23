//! Android system APIs. Vocabulary, review and synchronization stay in Rust.
use crate::{error::AppError, store::Store};
use std::sync::{Mutex, OnceLock};
use tauri::{plugin::{PluginHandle, TauriPlugin}, Emitter, RunEvent, WindowEvent, Wry};

static PLATFORM: OnceLock<PluginHandle<Wry>> = OnceLock::new();
static RESOURCES_READY: Mutex<bool> = Mutex::new(false);

pub(crate) fn init() -> TauriPlugin<Wry> {
    tauri::plugin::Builder::new("svl-mobile")
        .setup(|_, api| {
            let handle = api.register_android_plugin(
                "com.subtitlevocabularylist.android", "MobilePlatformPlugin",
            )?;
            PLATFORM.set(handle).map_err(|_| AppError::new("internal_error", "手机系统接口重复初始化。"))?;
            Ok(())
        })
        // Native credential commands are callable only by Rust's PluginHandle.
        .invoke_handler(|invoke| { invoke.resolver.reject("此系统接口仅供本机核心使用。"); true })
        .on_event(|app, event| {
            if matches!(event, RunEvent::Resumed | RunEvent::WindowEvent { event: WindowEvent::Resumed, .. }) {
                let _ = app.emit("mobile_resumed", ());
            }
        })
        .build()
}

fn run<T: serde::de::DeserializeOwned>(command: &str, payload: serde_json::Value) -> Result<T, AppError> {
    PLATFORM.get().ok_or_else(|| AppError::new("provider_unavailable", "手机系统接口尚未就绪。"))?
        .run_mobile_plugin(command, payload)
        .map_err(|_| AppError::new("provider_unavailable", "手机系统操作未完成，请重试。"))
}

pub(crate) fn credential_transform(bytes: &[u8], protect: bool) -> Result<Vec<u8>, AppError> {
    use base64::Engine;
    if bytes.is_empty() || bytes.len() > 65536 {
        return Err(AppError::new("invalid_input", "连接凭据大小无效。"));
    }
    #[derive(serde::Deserialize)]
    struct Output { data: String }
    let result: Output = run("credentialTransform", serde_json::json!({
        "data": base64::engine::general_purpose::STANDARD.encode(bytes), "protect": protect,
    })).map_err(|_| AppError::new("credentials_unavailable", "手机无法读取或保存受保护凭据，请重新连接账号。"))?;
    base64::engine::general_purpose::STANDARD.decode(result.data)
        .map_err(|_| AppError::new("credentials_unavailable", "受保护凭据格式无效。"))
}

pub(crate) fn open_browser(url: &str) -> Result<(), AppError> {
    run::<serde_json::Value>("openBrowser", serde_json::json!({ "url": url }))?;
    Ok(())
}

pub(crate) fn network() -> Result<crate::device_sync::DeviceNetwork, AppError> {
    run("networkState", serde_json::json!({}))
}

/// Called on an IPC worker, never on the Android UI thread or with a DB lock.
pub(crate) fn prepare_resources(store: &Store) -> Result<(), AppError> {
    let mut ready = RESOURCES_READY.lock()
        .map_err(|_| AppError::new("resource_missing", "离线资源准备中断，请重新打开应用。"))?;
    if *ready { return Ok(()); }
    let platform = PLATFORM.get()
        .ok_or_else(|| AppError::new("resource_missing", "手机系统接口尚未就绪。"))?;
    platform.run_mobile_plugin::<serde_json::Value>("prepareResources", serde_json::json!({}))
        .map_err(|error| AppError::new("resource_missing", format!("手机离线词库准备失败：{error}")))?;
    store.reload_base_dictionary()?;
    *ready = true;
    Ok(())
}
