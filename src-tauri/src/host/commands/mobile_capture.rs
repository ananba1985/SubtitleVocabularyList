use super::{AppState, background};
use crate::{
    error::AppError, mobile_capture as capture, ocr::PixelRect, tasks::TaskHandle,
    watch::WatchClipRange,
};

#[svl_ipc_macros::command]
pub(crate) async fn mobile_incoming_get() -> Result<Vec<capture::IncomingInput>, AppError> {
    background(capture::incoming).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_incoming_ack(id: String) -> Result<(), AppError> {
    background(move || capture::ack(&id)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_image_prepare(
    app: AppState<'_>,
    id: String,
    uri: String,
) -> Result<capture::CaptureImage, AppError> {
    let app = app.inner().clone();
    background(move || capture::prepare(&app.store, id, uri)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_image_pick(
    app: AppState<'_>,
) -> Result<capture::CaptureImage, AppError> {
    let app = app.inner().clone();
    background(move || capture::pick(&app.store)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_video_frame(
    app: AppState<'_>,
) -> Result<capture::CaptureImage, AppError> {
    let app = app.inner().clone();
    background(move || capture::frame(&app.store)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_ocr_start(
    app: AppState<'_>,
    image_id: String,
    rect: PixelRect,
    operation_id: String,
) -> Result<TaskHandle<capture::OcrText>, AppError> {
    let app = app.inner().clone();
    background(move || app.mobile_ocr_start(image_id, rect, operation_id)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_frame_preview(
    app: AppState<'_>,
    image_id: String,
    text: String,
    range: WatchClipRange,
    operation_id: String,
) -> Result<TaskHandle<crate::tasks::results::PreviewAudio>, AppError> {
    let app = app.inner().clone();
    background(move || app.mobile_frame_preview(image_id, text, range, operation_id)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_frame_draft(
    app: AppState<'_>,
    image_id: String,
    text: String,
    range: Option<WatchClipRange>,
) -> Result<capture::FrameDraft, AppError> {
    let app = app.inner().clone();
    background(move || capture::frame_draft(&app.store, &image_id, &text, range.as_ref())).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_image_draft(
    app: AppState<'_>,
    image_id: String,
    text: String,
) -> Result<capture::FrameDraft, AppError> {
    let app = app.inner().clone();
    background(move || capture::image_draft(&app.store, &image_id, &text)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_text_draft(
    app: AppState<'_>,
    text: String,
    title: String,
) -> Result<capture::FrameDraft, AppError> {
    let app = app.inner().clone();
    background(move || capture::text_draft(&app.store, &text, &title)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_shared_video(
    app: AppState<'_>,
    uri: String,
) -> Result<crate::mobile_video::MediaRef, AppError> {
    let app = app.inner().clone();
    background(move || crate::mobile_video::register_shared(&app.store, &uri)).await
}

// Versioned presentation draft state, kept locally and excluded from business sync.
#[svl_ipc_macros::command]
pub(crate) async fn mobile_drafts_get(app: AppState<'_>) -> Result<Option<String>, AppError> {
    let app = app.inner().clone();
    background(move || crate::mobile_video::setting(&app.store, "android.drafts")).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_drafts_save(app: AppState<'_>, json: String) -> Result<(), AppError> {
    if json.len() > 1024 * 1024
        || serde_json::from_str::<serde_json::Value>(&json)?
            .get("schemaVersion")
            .and_then(|v| v.as_u64())
            != Some(1)
    {
        return Err(AppError::new("invalid_data", "手机草稿格式无效。"));
    }
    let app = app.inner().clone();
    background(move || crate::mobile_video::save(&app.store, "android.drafts", &json)).await
}
