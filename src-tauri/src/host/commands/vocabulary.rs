use super::{AppState, background};
use crate::error::AppError;
use crate::{
    application::{PreparedCollection, example_input_from_corpus},
    vocabulary::{CollectionInput, Entry, EntryUpdate},
};
use std::path::PathBuf;
use std::sync::Arc;

#[svl_ipc_macros::command]
pub(crate) async fn entries_list(
    app: AppState<'_>,
    search: Option<String>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<Entry>, AppError> {
    let app = app.inner().clone();
    background(move || {
        app.store.list_entries(
            &search.unwrap_or_default(),
            offset.unwrap_or(0),
            limit.unwrap_or(50),
        )
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) async fn entry_get(app: AppState<'_>, entry_id: String) -> Result<Entry, AppError> {
    let app = app.inner().clone();
    background(move || app.store.get_entry(&entry_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn entry_update(app: AppState<'_>, input: EntryUpdate) -> Result<Entry, AppError> {
    let app = app.inner().clone();
    background(move || app.store.update_entry(&input)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn collection_prepare(
    app: AppState<'_>,
    input: CollectionInput,
) -> Result<PreparedCollection, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.prepare(input)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn collection_from_example(
    app: AppState<'_>,
    source_id: String,
    example_id: String,
    text: String,
    kind: String,
    meaning: String,
    operation_id: String,
) -> Result<PreparedCollection, AppError> {
    let app = Arc::clone(app.inner());
    background(move || {
        let example = example_input_from_corpus(&app.store, &source_id, &example_id, &meaning)?;
        app.prepare(CollectionInput {
            operation_id,
            kind,
            text,
            meaning,
            examples: vec![example],
            target_entry_id: None,
            expected_revision: None,
        })
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) async fn collection_commit(
    app: AppState<'_>,
    draft_id: String,
    target_entry_id: Option<String>,
    expected_revision: Option<i64>,
    save_audio: bool,
) -> Result<crate::tasks::TaskHandle<crate::vocabulary::CollectionResult>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.commit(draft_id, target_entry_id, expected_revision, save_audio)).await
}

#[cfg(all(windows, feature = "desktop"))]
#[svl_ipc_macros::command]
pub(crate) async fn preview_start(
    app: AppState<'_>,
    source_id: String,
    example_id: String,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<crate::tasks::results::PreviewAudio>, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.preview_start(source_id, example_id, operation_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn media_path(app: AppState<'_>, asset_id: String) -> Result<PathBuf, AppError> {
    let app = Arc::clone(app.inner());
    background(move || app.store.media_file(&asset_id)).await
}
