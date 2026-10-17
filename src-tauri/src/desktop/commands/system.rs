use super::{AppState, background};
use crate::error::AppError;
use crate::{
    application::Settings,
    desktop_capture::{self, NativeDesktop},
    tasks::TaskSnapshot,
};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{Manager, State};

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

#[svl_ipc_macros::command]
pub(crate) async fn settings_get(app: AppState<'_>) -> Result<Settings, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.settings()).await
}

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
        app.tasks.cancel(&task_id)
    })
    .await
}

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

#[svl_ipc_macros::command]
pub(crate) fn app_quit(handle: tauri::AppHandle) {
    desktop_capture::quit(&handle);
}
