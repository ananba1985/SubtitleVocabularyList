use super::{AppState, background};
use crate::{error::AppError, mobile_video::{self, VideoControl, VideoState, VideoStudy, VideoViewport}, tasks::TaskHandle, playback::PlaybackState};

#[svl_ipc_macros::command]
pub(crate) async fn video_library(app: AppState<'_>) -> Result<PlaybackState, AppError> {
    let app = app.inner().clone(); background(move || app.store.playback_state()).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_learning_get(app: AppState<'_>) -> Result<crate::mobile_learning::VideoLearning, AppError> {
    let app = app.inner().clone(); background(move ||app.video_learning()).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_learning_visible(app: AppState<'_>, revision: u64, visible: bool) -> Result<(), AppError> {
    let app = app.inner().clone(); background(move ||app.video_learning_visible(revision,visible)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_learning_skip(app: AppState<'_>, revision: u64) -> Result<(), AppError> {
    let app = app.inner().clone(); background(move ||app.video_learning_skip(revision)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_pick(app: AppState<'_>) -> Result<PlaybackState, AppError> {
    let app = app.inner().clone(); background(move || mobile_video::pick(&app.store)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_open(app: AppState<'_>, uri: String) -> Result<VideoState, AppError> {
    let app = app.inner().clone(); background(move || mobile_video::open(&app.store,&uri)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_state(app: AppState<'_>) -> Result<VideoState, AppError> {
    let app = app.inner().clone(); background(move || mobile_video::state(&app.store)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_context(app: AppState<'_>) -> Result<Option<crate::watch::WatchContext>, AppError> {
    let app = app.inner().clone(); background(move || mobile_video::context(&app.store,&mobile_video::state(&app.store)?)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_control(app: AppState<'_>, control: VideoControl) -> Result<VideoState, AppError> {
    let app = app.inner().clone(); background(move || mobile_video::control(&app.store,control)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_viewport(viewport: VideoViewport) -> Result<(), AppError> { background(move || mobile_video::viewport(viewport)).await }
#[svl_ipc_macros::command]
pub(crate) async fn video_study(app: AppState<'_>, uri: String) -> Result<Option<VideoStudy>, AppError> {
    let app = app.inner().clone(); background(move || mobile_video::study(&app.store,&uri)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_index_start(app: AppState<'_>, uri: String, operation_id: String) -> Result<TaskHandle<VideoStudy>, AppError> {
    let app = app.inner().clone(); background(move || app.video_index_start(uri,operation_id)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn video_subtitle_add(app: AppState<'_>, uri: String, language: String, operation_id: String) -> Result<TaskHandle<VideoStudy>, AppError> {
    let app = app.inner().clone(); background(move || app.video_subtitle_add(uri,language,operation_id)).await
}
