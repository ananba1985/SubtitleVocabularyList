use super::{AppState, background};
use crate::error::AppError;
use crate::{
    corpus::CandidateExample,
    reviews::{AnswerInput, Attempt, CorrectionInput, Question, ReviewUnit},
    windows_native::SystemVoice,
};
use std::sync::Arc;

#[svl_ipc_macros::command]
pub(crate) async fn review_units(
    app: AppState<'_>,
    mode: String,
    dimension: String,
    offset: u32,
    limit: u32,
) -> Result<Vec<ReviewUnit>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_units(&mode, &dimension, offset, limit)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn review_question(
    app: AppState<'_>,
    unit_id: String,
    expected_revision: i64,
) -> Result<Question, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_question(&unit_id, expected_revision)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn review_hint(
    app: AppState<'_>,
    question_id: String,
) -> Result<String, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.review_hint(&question_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn review_submit(
    app: AppState<'_>,
    input: AnswerInput,
) -> Result<Attempt, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_submit(&input)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn review_correct(
    app: AppState<'_>,
    input: CorrectionInput,
) -> Result<Attempt, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_correct(&input)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn review_history(
    app: AppState<'_>,
    unit_id: Option<String>,
    offset: u32,
    limit: u32,
) -> Result<Vec<Attempt>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_history(unit_id.as_deref(), offset, limit)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn review_audio_start(
    app: AppState<'_>,
    question_id: String,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<crate::tasks::results::ReviewAudio>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.review_audio_start(question_id, operation_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn practice_get(
    app: AppState<'_>,
) -> Result<Option<crate::practice::PracticeState>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.practice_get()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn practice_import(
    app: AppState<'_>,
    document: crate::practice::PracticeDocument,
) -> Result<crate::practice::PracticeState, AppError> {
    let app = app.inner().clone();
    background(move || app.store.practice_import(document)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn practice_progress_save(
    app: AppState<'_>,
    document_id: String,
    progress: crate::practice::PracticeProgress,
) -> Result<(), AppError> {
    let app = app.inner().clone();
    background(move || app.store.practice_progress_save(&document_id, progress)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn practice_audio_save(
    app: AppState<'_>,
    request: tauri::ipc::Request<'_>,
) -> Result<String, AppError> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError::new("invalid_input", "请提供课程原声音频。"));
    };
    let bytes = bytes.clone();
    let app = app.inner().clone();
    background(move || app.store.practice_audio_save(&bytes)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn practice_example(
    app: AppState<'_>,
    document_id: String,
    chapter: usize,
    sentence: usize,
) -> Result<CandidateExample, AppError> {
    let app = app.inner().clone();
    background(move || app.store.practice_example(&document_id, chapter, sentence)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn speech_voices() -> Result<Vec<SystemVoice>, AppError> {
    background(crate::windows_native::system_voices).await
}

#[svl_ipc_macros::command]
pub(crate) async fn speech_start(
    app: AppState<'_>,
    text: String,
    operation_id: String,
    voice_id: Option<String>,
) -> Result<crate::tasks::TaskHandle<crate::tasks::results::SpeechAudio>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.speech_start(text, operation_id, voice_id)).await
}
