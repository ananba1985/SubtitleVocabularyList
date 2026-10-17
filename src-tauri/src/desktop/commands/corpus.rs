use super::{AppState, background};
use crate::error::AppError;
use crate::{
    corpus::{Candidate, CandidateExample, SourceSummary},
    tasks::TaskSnapshot,
};
use std::sync::Arc;

#[tauri::command]
pub(crate) async fn sources_list(app: AppState<'_>) -> Result<Vec<SourceSummary>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.sources()).await
}

#[tauri::command]
pub(crate) async fn import_start(
    app: AppState<'_>,
    paths: Vec<String>,
    operation_id: String,
    options: Option<crate::media::ImportOptions>,
) -> Result<TaskSnapshot, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.import_with_options(paths, operation_id, options.unwrap_or_default()))
        .await
}

#[tauri::command]
pub(crate) async fn media_inspect(
    app: AppState<'_>,
    path: String,
) -> Result<crate::media::MediaInspection, AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        let tools = app.settings()?.tools;
        crate::media::inspect(&tools, std::path::Path::new(&path))
    })
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn candidates_list(
    app: AppState<'_>,
    source_id: String,
    search: Option<String>,
    kind: Option<String>,
    only_pending: Option<bool>,
    show_known: Option<bool>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<Candidate>, AppError> {
    let app = app.inner().clone();
    background(move || {
        app.store.candidate_page(
            &source_id,
            &search.unwrap_or_default(),
            &kind.unwrap_or_default(),
            crate::corpus::CandidateVisibility {
                only_pending: only_pending.unwrap_or(false),
                include_known: show_known.unwrap_or(false),
            },
            offset.unwrap_or(0),
            limit.unwrap_or(50),
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn known_target_get(
    app: AppState<'_>,
    kind: String,
    text: String,
) -> Result<bool, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.is_known_target(&kind, &text)).await
}

#[tauri::command]
pub(crate) async fn known_target_set(
    app: AppState<'_>,
    kind: String,
    text: String,
    known: bool,
) -> Result<(), AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        app.store.set_known_target(&kind, &text, known)?;
        if !known {
            app.preparation_requested
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub(crate) async fn known_targets_list(
    app: AppState<'_>,
    search: Option<String>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<crate::known_targets::KnownTargetPage, AppError> {
    let app = app.inner().clone();
    background(move || {
        app.store.known_targets(
            &search.unwrap_or_default(),
            offset.unwrap_or(0),
            limit.unwrap_or(20),
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn candidate_examples(
    app: AppState<'_>,
    source_id: String,
    key: String,
) -> Result<Vec<CandidateExample>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.candidate_examples(&source_id, &key)).await
}

#[tauri::command]
pub(crate) async fn candidate_decide(
    app: AppState<'_>,
    source_id: String,
    key: String,
    example_id: String,
    decision: String,
) -> Result<(), AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        app.store
            .decide_candidate(&source_id, &key, &example_id, &decision)
    })
    .await
}

#[tauri::command]
pub(crate) async fn source_example_update(
    app: AppState<'_>,
    source_id: String,
    example_id: String,
    revision: i64,
    text: String,
    start_ms: i64,
    end_ms: i64,
) -> Result<CandidateExample, AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        let value = app.store.update_source_example(
            &source_id,
            &example_id,
            revision,
            &text,
            start_ms,
            end_ms,
        )?;
        app.preparation_requested
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(value)
    })
    .await
}
