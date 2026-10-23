//! Android system APIs. Vocabulary, review and synchronization stay in Rust.
use crate::{error::AppError, store::Store};
use std::sync::{Mutex, OnceLock};
use tauri::{plugin::{PluginHandle, TauriPlugin}, Wry};

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
        .build()
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
