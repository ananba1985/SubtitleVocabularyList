use super::{AppState, background};
use crate::error::AppError;
use crate::{
    application::Settings,
    tasks::TaskSnapshot,
};
#[cfg(all(windows, feature = "desktop"))]
use crate::desktop_capture::{self, NativeDesktop};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{Manager, State};

#[derive(serde::Serialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppCapabilities {
    platform: String,
    offline_core: bool,
    independent_runtime: bool,
    synchronization: bool,
    native_playback: bool,
    screen_capture: bool,
    system_speech: bool,
    known_target_preferences: bool,
}

#[svl_ipc_macros::command]
pub(crate) async fn app_ready(startup: State<'_, crate::host::HostStartup>) -> Result<(), AppError> {
    startup.wait().await
}

#[svl_ipc_macros::command]
pub(crate) fn app_capabilities() -> AppCapabilities {
    AppCapabilities {
        platform: std::env::consts::OS.into(),
        offline_core: true,
        independent_runtime: true,
        synchronization: cfg!(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile"))),
        native_playback: cfg!(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile"))),
        screen_capture: cfg!(all(windows, feature = "desktop")),
        system_speech: cfg!(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile"))),
        known_target_preferences: true,
    }
}

#[cfg(not(all(windows, feature = "desktop")))]
#[svl_ipc_macros::command]
pub(crate) async fn settings_update(app: AppState<'_>, settings: Settings) -> Result<Settings, AppError> {
    let app = app.inner().clone();
    background(move || app.save_settings(settings)).await
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppInfo {
    version: String,
    data_directory: String,
    schema_version: i64,
    entry_count: i64,
    source_count: i64,
    network_unavailable: bool,
    dictionary_warnings: Vec<String>,
}

#[derive(Default)]
pub(crate) struct TrayAction(pub(crate) Mutex<Option<TrayNavigation>>);

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub(crate) enum TrayNavigation {
    Library,
    Collect,
    Import,
    Episodes,
    Watch,
    Practice,
    Reviews,
    Leech,
    Sync,
    Tasks,
    Settings,
}

#[svl_ipc_macros::command]
pub(crate) async fn app_info(app: AppState<'_>) -> Result<AppInfo, AppError> {
    let app = app.inner().clone();
    background(move || {
        #[cfg(all(target_os = "android", feature = "mobile"))]
        crate::mobile_platform::prepare_resources(&app.store)?;
        let connection = app.store.connection()?;
        Ok(AppInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            network_unavailable: app.network_unavailable(),
            dictionary_warnings: app.store.dictionary_warnings(),
            data_directory: app.store.root().display().to_string(),
            schema_version: connection
                .pragma_query_value(None, "user_version", |row| row.get(0))?,
            entry_count: connection.query_row(
                "SELECT COUNT(*) FROM entries WHERE archived=0",
                [],
                |row| row.get(0),
            )?,
            source_count: connection.query_row(
                "SELECT COUNT(*) FROM sources WHERE imported_at IS NOT NULL",
                [],
                |row| row.get(0),
            )?,
        })
    })
    .await
}

#[cfg(all(windows, feature = "desktop"))]
#[svl_ipc_macros::command]
pub(crate) async fn autostart_get() -> Result<bool, AppError> {
    background(crate::autostart::enabled).await
}

#[cfg(all(windows, feature = "desktop"))]
#[svl_ipc_macros::command]
pub(crate) async fn autostart_set(app: AppState<'_>, enabled: bool) -> Result<bool, AppError> {
    let root = app.store.root().to_path_buf();
    background(move || crate::autostart::set_enabled(enabled, &root)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn settings_get(app: AppState<'_>) -> Result<Settings, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.settings()).await
}

#[cfg(all(windows, feature = "desktop"))]
#[svl_ipc_macros::command]
pub(crate) async fn settings_update(
    app: AppState<'_>,
    handle: tauri::AppHandle,
    settings: Settings,
) -> Result<Settings, AppError> {
    let app = app.inner().clone();
    background(move || {
        let previous = app.settings()?;
        let native = handle.state::<NativeDesktop>();
        native.configure(
            &handle,
            &settings.selection_shortcut,
            &settings.ocr_shortcut,
            &settings.input_shortcut,
            &settings.replacement_shortcut,
        )?;
        match app.save_settings(settings) {
            Ok(settings) => {
                if settings.replacement_shortcut != previous.replacement_shortcut
                    || settings.replacement_provider_id != previous.replacement_provider_id
                {
                    handle
                        .state::<crate::desktop_replacement::NativeReplacement>()
                        .cancel();
                }
                Ok(settings)
            }
            Err(error) => {
                let _ = native.configure(
                    &handle,
                    &previous.selection_shortcut,
                    &previous.ocr_shortcut,
                    &previous.input_shortcut,
                    &previous.replacement_shortcut,
                );
                Err(error)
            }
        }
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) async fn task_get(app: AppState<'_>, task_id: String) -> Result<TaskSnapshot, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.tasks.get(&task_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn tasks_list(app: AppState<'_>) -> Result<Vec<TaskSnapshot>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.tasks.list()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn tasks_history(
    app: AppState<'_>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<crate::tasks::TaskHistoryPage, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.tasks.history(offset.unwrap_or(0), limit.unwrap_or(10))).await
}

#[svl_ipc_macros::command]
pub(crate) async fn task_cancel(
    app: AppState<'_>,
    task_id: String,
) -> Result<TaskSnapshot, AppError> {
    let app = app.inner().clone();
    background(move || {
        let task = app.tasks.get(&task_id)?;
        if task.kind == "explanation_batch" && !task.terminal() {
            app.pause_preparation()?;
        }
        let result = app.tasks.cancel(&task_id)?;
        #[cfg(all(target_os = "android", feature = "mobile"))]
        if matches!(task.kind.as_str(), "speech" | "review_audio") && !task.terminal() {
            crate::mobile_platform::cancel_speech(&task_id);
        }
        Ok(result)
    })
    .await
}

#[cfg(all(windows, feature = "desktop"))]
#[svl_ipc_macros::command]
pub(crate) fn tray_action_get(
    action: State<'_, TrayAction>,
) -> Result<Option<TrayNavigation>, AppError> {
    Ok(action
        .0
        .lock()
        .map_err(|_| AppError::new("internal_error", "托盘操作暂时不可用。"))?
        .take())
}

#[cfg(all(windows, feature = "desktop"))]
#[svl_ipc_macros::command]
pub(crate) fn app_quit(handle: tauri::AppHandle) {
    desktop_capture::quit(&handle);
}
