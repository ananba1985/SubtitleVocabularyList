use super::{AppState, background};
use crate::{
    episode_sharing::{PreparedEpisodeApplied, PreparedEpisodeLocal, PreparedEpisodeMeta},
    error::AppError,
    tasks::TaskHandle,
};

#[svl_ipc_macros::command]
pub(crate) async fn prepared_episode_local(
    app: AppState<'_>,
    source_id: String,
) -> Result<PreparedEpisodeLocal, AppError> {
    let app = app.inner().clone();
    background(move || app.prepared_episode_local(&source_id)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn prepared_episode_check(
    app: AppState<'_>,
    source_id: String,
    operation_id: String,
) -> Result<TaskHandle<Option<PreparedEpisodeMeta>>, AppError> {
    let app = app.inner().clone();
    background(move || app.prepared_episode_check(source_id, operation_id)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn prepared_episode_publish(
    app: AppState<'_>,
    source_id: String,
    expected_revision: i64,
    operation_id: String,
) -> Result<TaskHandle<PreparedEpisodeMeta>, AppError> {
    let app = app.inner().clone();
    background(move || app.prepared_episode_publish(source_id, expected_revision, operation_id))
        .await
}
#[svl_ipc_macros::command]
pub(crate) async fn prepared_episode_download(
    app: AppState<'_>,
    source_id: String,
    expected_revision: i64,
    operation_id: String,
) -> Result<TaskHandle<PreparedEpisodeApplied>, AppError> {
    let app = app.inner().clone();
    background(move || app.prepared_episode_download(source_id, expected_revision, operation_id))
        .await
}
