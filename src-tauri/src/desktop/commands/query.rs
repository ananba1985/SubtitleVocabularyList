use super::{AppState, background};
use crate::error::AppError;
use crate::desktop_capture;
use std::sync::Arc;
use tauri::{Emitter, Manager};

#[svl_ipc_macros::command]
pub(crate) fn translation_session_open(
    app: AppState<'_>,
    input: crate::queries::QueryInput,
    session_id: Option<String>,
) -> Result<crate::queries::QuerySession, AppError> {
    app.queries.open(input, session_id.as_deref())
}

#[svl_ipc_macros::command]
pub(crate) fn translation_session_get(
    app: AppState<'_>,
    session_id: Option<String>,
) -> Result<Option<crate::queries::QuerySession>, AppError> {
    app.queries.get(session_id.as_deref())
}

#[svl_ipc_macros::command]
pub(crate) async fn translation_query_start(
    app: AppState<'_>,
    session_id: String,
    expected_input_revision: u64,
    provider_ids: Vec<String>,
    automatic: bool,
    persist: bool,
) -> Result<crate::queries::QuerySession, AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        app.query_start(
            session_id,
            expected_input_revision,
            provider_ids,
            automatic,
            persist,
        )
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) fn translation_query_cancel(
    app: AppState<'_>,
    session_id: String,
    query_id: Option<String>,
) -> Result<(), AppError> {
    app.queries.cancel(&session_id, query_id.as_deref())
}

#[svl_ipc_macros::command]
pub(crate) fn translation_window_hide(handle: tauri::AppHandle) {
    crate::desktop_query::hide_query(&handle);
}

#[svl_ipc_macros::command]
pub(crate) fn translation_window_pin(
    handle: tauri::AppHandle,
    pinned: bool,
) -> Result<(), AppError> {
    handle
        .get_webview_window("translator")
        .ok_or_else(|| AppError::new("not_found", "查询窗口未打开。"))?
        .set_always_on_top(pinned)
        .map_err(|_| AppError::new("internal_error", "无法固定查询窗口。"))
}

#[svl_ipc_macros::command]
pub(crate) fn translation_open(handle: tauri::AppHandle) -> Result<(), AppError> {
    crate::desktop_query::open_input(&handle)
}

#[svl_ipc_macros::command]
pub(crate) async fn translation_collect(
    app: AppState<'_>,
    handle: tauri::AppHandle,
    session_id: String,
    input_revision: u64,
    provider_id: String,
) -> Result<(), AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        let seed = app.query_collection(&session_id, input_revision, &provider_id)?;
        handle
            .emit_to("main", "capture_completed", seed)
            .map_err(|_| AppError::new("internal_error", "无法打开收录确认。"))?;
        crate::desktop_query::hide_query(&handle);
        desktop_capture::show_main(&handle);
        Ok(())
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) fn query_copy(handle: tauri::AppHandle, text: String) -> Result<(), AppError> {
    handle
        .state::<crate::desktop_query::ClipboardWatcher>()
        .copy(&text)
}

#[svl_ipc_macros::command]
pub(crate) fn clipboard_listener_get(
    handle: tauri::AppHandle,
) -> crate::desktop_query::ClipboardStatus {
    handle
        .state::<crate::desktop_query::ClipboardWatcher>()
        .status()
}

#[svl_ipc_macros::command]
pub(crate) fn clipboard_listener_set(
    handle: tauri::AppHandle,
    enabled: bool,
    preferences: crate::desktop_query::ClipboardPreferences,
) -> Result<crate::desktop_query::ClipboardStatus, AppError> {
    let status = handle
        .state::<crate::desktop_query::ClipboardWatcher>()
        .set(&handle, enabled, preferences)?;
    crate::desktop_query::refresh_clipboard_menu(&handle);
    Ok(status)
}

#[svl_ipc_macros::command]
pub(crate) async fn providers_list(
    app: AppState<'_>,
) -> Result<Vec<crate::providers::Provider>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.providers()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn providers_update(
    app: AppState<'_>,
    update: crate::providers::ProviderUpdate,
) -> Result<Vec<crate::providers::Provider>, AppError> {
    let app = app.inner().clone();
    background(move || {
        let id = update.id.clone();
        let providers = app.store.provider_update(update)?;
        app.queries.cancel_provider(&id);
        Ok(providers)
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) async fn plugin_install(
    app: AppState<'_>,
    path: String,
) -> Result<crate::providers::Provider, AppError> {
    let app = app.inner().clone();
    background(move || {
        let metadata = std::fs::metadata(&path)?;
        if !metadata.is_file() || metadata.len() > 512_000 {
            return Err(AppError::new(
                "invalid_input",
                "请选择小于 512 KB 的插件 JSON 配置。",
            ));
        }
        let manifest = serde_json::from_str(&std::fs::read_to_string(path)?)
            .map_err(|_| AppError::new("invalid_data", "插件配置不是支持的 JSON 格式。"))?;
        let provider = app.store.plugin_install_manifest(manifest)?;
        app.queries.cancel_provider(&provider.id);
        Ok(provider)
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) async fn plugin_remove(
    app: AppState<'_>,
    plugin_id: String,
) -> Result<Vec<crate::providers::Provider>, AppError> {
    let app = app.inner().clone();
    background(move || {
        let providers = app.store.plugin_remove(&plugin_id)?;
        app.queries.cancel_provider(&plugin_id);
        Ok(providers)
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) async fn explanations_status(
    app: AppState<'_>,
) -> Result<crate::preparation::PreparationStatus, AppError> {
    let app = app.inner().clone();
    background(move || app.preparation_status()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn explanations_prepare(
    app: AppState<'_>,
    source_id: Option<String>,
) -> Result<crate::preparation::PreparationReply, AppError> {
    let app = app.inner().clone();
    background(move || app.prepare_explanations(source_id, true)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn explanation_get(
    app: AppState<'_>,
    text: String,
    context: String,
) -> Result<Option<crate::explanations::PreparedExplanation>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.prepared_explanation(&text, &context)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn explanation_import(
    app: AppState<'_>,
    text: String,
    context: String,
    value: crate::explanations::Explanation,
    model_url: String,
    model_name: String,
) -> Result<crate::explanations::Explanation, AppError> {
    let app = app.inner().clone();
    background(move || {
        app.store
            .save_explanation(&text, &context, &value, &model_url, &model_name)
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) async fn explain_start(
    app: AppState<'_>,
    text: String,
    context: String,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<crate::explanations::Explanation>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.explain_start(text, context, operation_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn online_query_start(
    app: AppState<'_>,
    text: String,
    provider: Option<String>,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<crate::lookup::LookupResult>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        app.online_query_start(
            text,
            provider.unwrap_or_else(|| "dictionary".into()),
            operation_id,
        )
    })
    .await
}
