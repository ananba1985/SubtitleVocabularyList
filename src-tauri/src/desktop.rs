use crate::{
    application::{Application, PreparedCollection, Settings, example_input_from_corpus},
    corpus::{Candidate, CandidateExample, SourceSummary},
    error::AppError,
    store::Store,
    tasks::{TaskManager, TaskSnapshot},
    vocabulary::{CollectionInput, Entry, EntryUpdate},
};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};

type AppState<'a> = State<'a, Arc<Application>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    data_directory: String,
    schema_version: i64,
    entry_count: i64,
    source_count: i64,
}

async fn background<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| AppError::new("internal_error", "本次操作异常中断，已保存内容保留。"))?
}

#[tauri::command]
async fn app_info(app: AppState<'_>) -> Result<AppInfo, AppError> {
    let app = app.inner().clone();
    background(move || {
        let connection = app.store.connection()?;
        Ok(AppInfo {
            data_directory: app.store.root().display().to_string(),
            schema_version: connection
                .pragma_query_value(None, "user_version", |row| row.get(0))?,
            entry_count: connection
                .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?,
            source_count: connection.query_row(
                "SELECT COUNT(*) FROM sources WHERE imported_at IS NOT NULL",
                [],
                |row| row.get(0),
            )?,
        })
    })
    .await
}

#[tauri::command]
async fn sources_list(app: AppState<'_>) -> Result<Vec<SourceSummary>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.sources()).await
}
#[tauri::command]
fn import_start(
    app: AppState<'_>,
    paths: Vec<String>,
    operation_id: String,
) -> Result<TaskSnapshot, AppError> {
    app.import_start(paths, operation_id)
}
#[tauri::command]
fn task_get(app: AppState<'_>, task_id: String) -> Result<TaskSnapshot, AppError> {
    app.tasks.get(&task_id)
}
#[tauri::command]
fn tasks_list(app: AppState<'_>) -> Result<Vec<TaskSnapshot>, AppError> {
    app.tasks.list()
}
#[tauri::command]
fn task_cancel(app: AppState<'_>, task_id: String) -> Result<TaskSnapshot, AppError> {
    app.tasks.cancel(&task_id)
}
#[tauri::command]
async fn candidates_list(
    app: AppState<'_>,
    source_id: String,
    search: Option<String>,
    kind: Option<String>,
    only_pending: Option<bool>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<Candidate>, AppError> {
    let app = app.inner().clone();
    background(move || {
        app.store.candidates(
            &source_id,
            &search.unwrap_or_default(),
            &kind.unwrap_or_default(),
            only_pending.unwrap_or(false),
            offset.unwrap_or(0),
            limit.unwrap_or(50),
        )
    })
    .await
}
#[tauri::command]
async fn candidate_examples(
    app: AppState<'_>,
    source_id: String,
    key: String,
) -> Result<Vec<CandidateExample>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.candidate_examples(&source_id, &key)).await
}
#[tauri::command]
fn candidate_decide(
    app: AppState<'_>,
    source_id: String,
    key: String,
    example_id: String,
    decision: String,
) -> Result<(), AppError> {
    app.store
        .decide_candidate(&source_id, &key, &example_id, &decision)
}
#[tauri::command]
fn source_example_update(
    app: AppState<'_>,
    source_id: String,
    example_id: String,
    revision: i64,
    text: String,
    start_ms: i64,
    end_ms: i64,
) -> Result<CandidateExample, AppError> {
    app.store
        .update_source_example(&source_id, &example_id, revision, &text, start_ms, end_ms)
}
#[tauri::command]
async fn entries_list(
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
#[tauri::command]
async fn entry_get(app: AppState<'_>, entry_id: String) -> Result<Entry, AppError> {
    let app = app.inner().clone();
    background(move || app.store.get_entry(&entry_id)).await
}
#[tauri::command]
async fn entry_update(app: AppState<'_>, input: EntryUpdate) -> Result<Entry, AppError> {
    let app = app.inner().clone();
    background(move || app.store.update_entry(&input)).await
}
#[tauri::command]
fn collection_prepare(
    app: AppState<'_>,
    input: CollectionInput,
) -> Result<PreparedCollection, AppError> {
    app.prepare(input)
}
#[tauri::command]
fn collection_from_example(
    app: AppState<'_>,
    source_id: String,
    example_id: String,
    text: String,
    kind: String,
    meaning: String,
    operation_id: String,
) -> Result<PreparedCollection, AppError> {
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
}
#[tauri::command]
fn collection_commit(
    app: AppState<'_>,
    draft_id: String,
    target_entry_id: Option<String>,
    expected_revision: Option<i64>,
    save_audio: bool,
) -> Result<TaskSnapshot, AppError> {
    app.commit(draft_id, target_entry_id, expected_revision, save_audio)
}
#[tauri::command]
fn preview_start(
    app: AppState<'_>,
    source_id: String,
    example_id: String,
    operation_id: String,
) -> Result<TaskSnapshot, AppError> {
    app.preview_start(source_id, example_id, operation_id)
}
#[tauri::command]
fn media_path(app: AppState<'_>, asset_id: String) -> Result<PathBuf, AppError> {
    app.store.media_file(&asset_id)
}
#[tauri::command]
fn explain_start(
    app: AppState<'_>,
    text: String,
    context: String,
    operation_id: String,
) -> Result<TaskSnapshot, AppError> {
    app.explain_start(text, context, operation_id)
}
#[tauri::command]
fn settings_get(app: AppState<'_>) -> Result<Settings, AppError> {
    app.settings()
}
#[tauri::command]
fn settings_update(app: AppState<'_>, settings: Settings) -> Result<Settings, AppError> {
    app.save_settings(settings)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let root = std::env::var_os("SVL_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            let store = Arc::new(Store::open(root)?);
            app.asset_protocol_scope()
                .allow_directory(store.root().join("media"), true)?;
            app.asset_protocol_scope()
                .allow_directory(store.root().join("jobs"), true)?;
            let handle = app.handle().clone();
            let tasks = TaskManager::new(
                Arc::clone(&store),
                Arc::new(move |snapshot| {
                    let _ = handle.emit("task_updated", snapshot);
                }),
            );
            tasks.recover_interrupted()?;
            #[cfg(debug_assertions)]
            let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .ok_or("Missing project directory")?
                .join(".tools/runtime");
            #[cfg(not(debug_assertions))]
            let runtime = app.path().resource_dir()?.join("tools");
            let defaults = Settings {
                tools: crate::media::MediaTools::development(runtime),
                ..Default::default()
            };
            app.manage(Arc::new(Application::new(store, tasks, defaults)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            sources_list,
            import_start,
            task_get,
            tasks_list,
            task_cancel,
            candidates_list,
            candidate_examples,
            candidate_decide,
            source_example_update,
            entries_list,
            entry_get,
            entry_update,
            collection_prepare,
            collection_from_example,
            collection_commit,
            preview_start,
            media_path,
            explain_start,
            settings_get,
            settings_update
        ])
        .run(tauri::generate_context!())
        .expect("Cannot start SubtitleVocabularyList");
}
