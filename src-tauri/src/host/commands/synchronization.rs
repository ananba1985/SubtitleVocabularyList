use super::{AppState, background, background_project};
use crate::error::AppError;
use crate::site_connection::ConnectionStatus;
use std::sync::Arc;

#[svl_ipc_macros::command]
pub(crate) async fn device_sync_get(app: AppState<'_>) -> Result<crate::device_sync::DeviceSyncState, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.device_sync_state()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn device_sync_update(app: AppState<'_>, input: crate::device_sync::DeviceSyncPreferences) -> Result<crate::device_sync::DeviceSyncPreferences, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.device_sync_update(input)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn device_sync_bind(app: AppState<'_>, expected_account: String) -> Result<crate::device_sync::DeviceSyncState, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.device_sync_bind(expected_account)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn connection_disconnect(app: AppState<'_>) -> Result<(), AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.connection_disconnect()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn connection_status(app: AppState<'_>) -> Result<ConnectionStatus, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.connection_status()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn connection_start(
    app: AppState<'_>,
    operation_id: String,
    replace: bool,
) -> Result<crate::tasks::TaskHandle<crate::site_connection::ConnectionStatus>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.connection_start(operation_id, replace)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn connection_check(
    app: AppState<'_>,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<crate::site_connection::ConnectionStatus>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.connection_check(operation_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn connection_open(app: AppState<'_>) -> Result<(), AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.connection_open()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn sync_status(
    app: AppState<'_>,
) -> Result<crate::synchronization::SyncStatus, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.sync_status()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn sync_conflicts(
    app: AppState<'_>,
) -> Result<Vec<crate::ipc_dto::SyncConflictView>, AppError> {
    let app = Arc::clone(app.inner());
    background_project(move || app.sync_conflicts()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn sync_start(
    app: AppState<'_>,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<crate::tasks::results::SyncTaskResult>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.sync_start(operation_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn sync_resolve(
    app: AppState<'_>,
    input: crate::synchronization::ResolutionInput,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<crate::tasks::results::SyncResolution>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.sync_resolve(input, operation_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn assessment_sync_status(
    app: AppState<'_>,
) -> Result<crate::assessment_sync::AssessmentSyncStatus, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.assessment_sync_status()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn assessment_sync_conflicts(
    app: AppState<'_>,
) -> Result<Vec<crate::ipc_dto::AssessmentSyncConflict>, AppError> {
    let app = Arc::clone(app.inner());
    background_project(move || app.assessment_sync_conflicts()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn assessment_sync_keep_both(
    app: AppState<'_>,
    input: crate::assessment_sync::KeepAssessmentBoth,
) -> Result<crate::ipc_dto::AssessmentKeepBothReply, AppError> {
    let app = Arc::clone(app.inner());
    tauri::async_runtime::spawn_blocking(move || {
        crate::ipc_dto::project(app.assessment_sync_keep_both(&input)?)
    })
    .await
    .map_err(|_| AppError::new("internal_error", "冲突处理未完成，请刷新记录。"))?
}
