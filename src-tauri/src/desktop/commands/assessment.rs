use super::{AppState, background};
use crate::error::AppError;
use crate::tasks::TaskSnapshot;
use std::sync::Arc;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssessmentBankAudioInput {
    content: crate::assessment_bank::QuestionDraft,
    operation_id: String,
}

#[tauri::command]
pub(crate) async fn assessment_preview(
    app: AppState<'_>,
    config: crate::assessment_bank::AssessmentConfig,
) -> Result<crate::assessment_sampling::AssessmentPreview, AppError> {
    let app = Arc::clone(app.inner());
    tauri::async_runtime::spawn_blocking(move || app.store.assessment_preview(&config))
        .await
        .map_err(|_| AppError::new("internal_error", "测试预览中断，请重试。"))?
}

#[tauri::command]
pub(crate) async fn assessment_start(
    app: AppState<'_>,
    input: crate::assessment::StartAssessment,
) -> Result<crate::assessment::AssessmentSession, AppError> {
    let app = Arc::clone(app.inner());
    tauri::async_runtime::spawn_blocking(move || app.store.assessment_start(&input))
        .await
        .map_err(|_| AppError::new("internal_error", "测试启动中断，请重新读取未完成测试。"))?
}

#[tauri::command]
pub(crate) async fn assessment_session_get(
    app: AppState<'_>,
    session_id: String,
) -> Result<crate::assessment::AssessmentSession, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_session(&session_id)).await
}

#[tauri::command]
pub(crate) async fn assessment_session_list(
    app: AppState<'_>,
    unfinished_only: bool,
    offset: usize,
    limit: usize,
) -> Result<serde_json::Value, AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        app.store
            .assessment_session_list(unfinished_only, offset, limit)
    })
    .await
}

#[tauri::command]
pub(crate) async fn assessment_answer_submit(
    app: AppState<'_>,
    input: crate::assessment::AssessmentAnswerInput,
) -> Result<crate::assessment::AssessmentSession, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_answer_submit(&input)).await
}

#[tauri::command]
pub(crate) async fn assessment_session_transition(
    app: AppState<'_>,
    input: crate::assessment::TransitionAssessment,
) -> Result<crate::assessment::AssessmentSession, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_transition(&input)).await
}

#[tauri::command]
pub(crate) async fn assessment_audio_start(
    app: AppState<'_>,
    input: crate::assessment::AssessmentAudioStart,
) -> Result<crate::assessment::AssessmentPlayback, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_audio_start(&input)).await
}

#[tauri::command]
pub(crate) async fn assessment_audio_event(
    app: AppState<'_>,
    input: crate::assessment::AssessmentAudioEvent,
) -> Result<serde_json::Value, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_audio_event(&input)).await
}

#[tauri::command]
pub(crate) async fn assessment_result_correct(
    app: AppState<'_>,
    input: crate::assessment::CorrectAssessment,
) -> Result<serde_json::Value, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_correct(&input)).await
}

#[tauri::command]
pub(crate) async fn assessment_report_get(
    app: AppState<'_>,
    session_id: String,
    report_revision: Option<i64>,
) -> Result<serde_json::Value, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_report(&session_id, report_revision)).await
}

#[tauri::command]
pub(crate) async fn assessment_report_items(
    app: AppState<'_>,
    session_id: String,
    report_revision: i64,
    offset: usize,
    limit: usize,
) -> Result<serde_json::Value, AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        app.store
            .assessment_report_items(&session_id, report_revision, offset, limit)
    })
    .await
}

#[tauri::command]
pub(crate) async fn assessment_history_list(
    app: AppState<'_>,
    offset: usize,
    limit: usize,
    comparison_key: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        app.store
            .assessment_history(offset, limit, comparison_key.as_deref())
    })
    .await
}

#[tauri::command]
pub(crate) async fn assessment_compare(
    app: AppState<'_>,
    first: String,
    second: String,
) -> Result<serde_json::Value, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_compare(&first, &second)).await
}

#[tauri::command]
pub(crate) async fn assessment_collection_prepare(
    app: AppState<'_>,
    input: crate::assessment_reports::PrepareAssessmentCollection,
) -> Result<crate::assessment_reports::AssessmentCollectionDraft, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_collection_prepare(&input)).await
}

#[tauri::command]
pub(crate) async fn assessment_collection_commit(
    app: AppState<'_>,
    input: crate::assessment_reports::CommitAssessmentCollection,
) -> Result<TaskSnapshot, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.assessment_collection_commit(input)).await
}

#[tauri::command]
pub(crate) async fn assessment_catalog_get(
    app: AppState<'_>,
) -> Result<crate::assessment_bank::CatalogInventory, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_inventory()).await
}

#[tauri::command]
pub(crate) async fn assessment_settings_get(
    app: AppState<'_>,
) -> Result<crate::assessment_bank::AssessmentSettings, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_settings()).await
}

#[tauri::command]
pub(crate) async fn assessment_settings_update(
    app: AppState<'_>,
    settings: crate::assessment_bank::AssessmentSettings,
    operation_id: String,
) -> Result<crate::assessment_bank::AssessmentSettings, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_save_settings(settings, &operation_id)).await
}

#[tauri::command]
pub(crate) async fn assessment_bank_prepare(
    app: AppState<'_>,
    input: crate::assessment_bank::PrepareBank,
) -> Result<TaskSnapshot, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.assessment_bank_prepare(input)).await
}

#[tauri::command]
pub(crate) async fn assessment_bank_review(
    app: AppState<'_>,
    input: crate::assessment_bank::ReviewBank,
) -> Result<serde_json::Value, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.assessment_bank_review(&input)).await
}

#[tauri::command]
pub(crate) async fn assessment_bank_items(
    app: AppState<'_>,
    bank_id: String,
    state: Option<String>,
    offset: usize,
    limit: usize,
) -> Result<crate::assessment_bank::BankPage, AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        app.store
            .assessment_bank_items(&bank_id, state.as_deref(), offset, limit)
    })
    .await
}

#[tauri::command]
pub(crate) async fn assessment_bank_audio_prepare(
    app: AppState<'_>,
    input: AssessmentBankAudioInput,
) -> Result<TaskSnapshot, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.assessment_bank_audio_prepare(input.content, input.operation_id)).await
}

#[tauri::command]
pub(crate) async fn assessment_sync_status(
    app: AppState<'_>,
) -> Result<crate::assessment_sync::AssessmentSyncStatus, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.assessment_sync_status()).await
}

#[tauri::command]
pub(crate) async fn assessment_sync_conflicts(
    app: AppState<'_>,
) -> Result<Vec<serde_json::Value>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.assessment_sync_conflicts()).await
}

#[tauri::command]
pub(crate) async fn assessment_sync_keep_both(
    app: AppState<'_>,
    input: crate::assessment_sync::KeepAssessmentBoth,
) -> Result<serde_json::Value, AppError> {
    let app = Arc::clone(app.inner());
    tauri::async_runtime::spawn_blocking(move || app.assessment_sync_keep_both(&input))
        .await
        .map_err(|_| AppError::new("internal_error", "冲突处理未完成，请刷新记录。"))?
}

#[tauri::command]
pub(crate) async fn assessment_audio_prepare(
    app: AppState<'_>,
    session_id: String,
    item_id: String,
    operation_id: String,
) -> Result<TaskSnapshot, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.assessment_audio_prepare(session_id, item_id, operation_id)).await
}
