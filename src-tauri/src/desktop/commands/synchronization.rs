use super::{AppState, background, background_project};
use crate::error::AppError;
use crate::{site_connection::ConnectionStatus, tasks::TaskSnapshot};
use std::sync::Arc;

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
) -> Result<TaskSnapshot, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.connection_start(operation_id, replace)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn connection_check(
    app: AppState<'_>,
    operation_id: String,
) -> Result<TaskSnapshot, AppError> {
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
) -> Result<TaskSnapshot, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.sync_start(operation_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn sync_resolve(
    app: AppState<'_>,
    input: crate::synchronization::ResolutionInput,
    operation_id: String,
) -> Result<TaskSnapshot, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.sync_resolve(input, operation_id)).await
}
