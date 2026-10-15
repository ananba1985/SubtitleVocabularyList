use crate::{
    application::{Application, PreparedCollection, Settings, example_input_from_corpus},
    corpus::{Candidate, CandidateExample, SourceSummary},
    error::AppError,
    reviews::{AnswerInput, Attempt, CorrectionInput, Question, ReviewUnit},
    site_connection::ConnectionStatus,
    store::Store,
    tasks::{TaskManager, TaskSnapshot},
    vocabulary::{CollectionInput, Entry, EntryUpdate},
};
use crate::{
    desktop_capture::{self, NativeDesktop, NativeStatus},
    windows_native::SystemVoice,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_global_shortcut::ShortcutState;

type AppState<'a> = State<'a, Arc<Application>>;

#[tauri::command]
async fn assessment_preview(app: AppState<'_>, config: crate::assessment_bank::AssessmentConfig) -> Result<crate::assessment_sampling::AssessmentPreview, AppError> {
    let app=Arc::clone(app.inner());
    tauri::async_runtime::spawn_blocking(move||app.store.assessment_preview(&config)).await.map_err(|_|AppError::new("internal_error","测试预览中断，请重试。"))?
}
#[tauri::command]
async fn assessment_start(app: AppState<'_>, input: crate::assessment::StartAssessment) -> Result<crate::assessment::AssessmentSession, AppError> {
    let app=Arc::clone(app.inner());
    tauri::async_runtime::spawn_blocking(move||app.store.assessment_start(&input)).await.map_err(|_|AppError::new("internal_error","测试启动中断，请重新读取未完成测试。"))?
}
#[tauri::command]
fn assessment_session_get(app: AppState<'_>, session_id: String) -> Result<crate::assessment::AssessmentSession, AppError> {app.store.assessment_session(&session_id)}
#[tauri::command]
fn assessment_session_list(app: AppState<'_>, unfinished_only: bool, offset: usize, limit: usize) -> Result<serde_json::Value, AppError> {app.store.assessment_session_list(unfinished_only,offset,limit)}
#[tauri::command]
fn assessment_answer_submit(app: AppState<'_>, input: crate::assessment::AssessmentAnswerInput) -> Result<crate::assessment::AssessmentSession, AppError> {app.store.assessment_answer_submit(&input)}
#[tauri::command]
fn assessment_session_transition(app: AppState<'_>, input: crate::assessment::TransitionAssessment) -> Result<crate::assessment::AssessmentSession, AppError> {app.store.assessment_transition(&input)}
#[tauri::command]
fn assessment_audio_start(app: AppState<'_>, input: crate::assessment::AssessmentAudioStart) -> Result<crate::assessment::AssessmentPlayback, AppError> {app.store.assessment_audio_start(&input)}
#[tauri::command]
fn assessment_audio_event(app: AppState<'_>, input: crate::assessment::AssessmentAudioEvent) -> Result<serde_json::Value, AppError> {app.store.assessment_audio_event(&input)}
#[tauri::command]
fn assessment_result_correct(app: AppState<'_>, input: crate::assessment::CorrectAssessment) -> Result<serde_json::Value, AppError> {app.store.assessment_correct(&input)}

#[tauri::command]
fn assessment_catalog_get(app: AppState<'_>) -> Result<crate::assessment_bank::CatalogInventory, AppError> {
    app.store.assessment_inventory()
}
#[tauri::command]
fn assessment_settings_get(app: AppState<'_>) -> Result<crate::assessment_bank::AssessmentSettings, AppError> {
    app.store.assessment_settings()
}
#[tauri::command]
fn assessment_settings_update(app: AppState<'_>, settings: crate::assessment_bank::AssessmentSettings, operation_id: String) -> Result<crate::assessment_bank::AssessmentSettings, AppError> {
    app.store.assessment_save_settings(settings, &operation_id)
}
#[tauri::command]
fn assessment_bank_prepare(app: AppState<'_>, input: crate::assessment_bank::PrepareBank) -> Result<TaskSnapshot, AppError> {
    app.inner().assessment_bank_prepare(input)
}
#[tauri::command]
fn assessment_bank_review(app: AppState<'_>, input: crate::assessment_bank::ReviewBank) -> Result<serde_json::Value, AppError> {
    app.store.assessment_bank_review(&input)
}
#[tauri::command]
fn assessment_bank_items(app: AppState<'_>, bank_id: String, state: Option<String>, offset: usize, limit: usize) -> Result<crate::assessment_bank::BankPage, AppError> {
    app.store.assessment_bank_items(&bank_id, state.as_deref(), offset, limit)
}

#[derive(Default)]
struct TrayAction(Mutex<Option<String>>);

#[tauri::command]
fn tray_action_get(action: State<'_, TrayAction>) -> Result<Option<String>, AppError> {
    Ok(action
        .0
        .lock()
        .map_err(|_| AppError::new("internal_error", "托盘操作暂时不可用。"))?
        .take())
}

fn tray_navigate(app: &tauri::AppHandle, action: &str) {
    if let Ok(mut pending) = app.state::<TrayAction>().0.lock() {
        *pending = Some(action.to_owned());
    }
    desktop_capture::show_main(app);
    let _ = app.emit_to("main", "tray_action", ());
}

#[tauri::command]
fn translation_session_open(
    app: AppState<'_>,
    input: crate::queries::QueryInput,
    session_id: Option<String>,
) -> Result<crate::queries::QuerySession, AppError> {
    app.queries.open(input, session_id.as_deref())
}
#[tauri::command]
fn translation_session_get(
    app: AppState<'_>,
    session_id: Option<String>,
) -> Result<Option<crate::queries::QuerySession>, AppError> {
    app.queries.get(session_id.as_deref())
}
#[tauri::command]
fn translation_query_start(
    app: AppState<'_>,
    session_id: String,
    expected_input_revision: u64,
    provider_ids: Vec<String>,
    automatic: bool,
    persist: bool,
) -> Result<crate::queries::QuerySession, AppError> {
    app.inner().query_start(
        session_id,
        expected_input_revision,
        provider_ids,
        automatic,
        persist,
    )
}
#[tauri::command]
fn translation_query_cancel(
    app: AppState<'_>,
    session_id: String,
    query_id: Option<String>,
) -> Result<(), AppError> {
    app.queries.cancel(&session_id, query_id.as_deref())
}
#[tauri::command]
fn translation_window_hide(handle: tauri::AppHandle) {
    crate::desktop_query::hide_query(&handle);
}
#[tauri::command]
fn translation_window_pin(handle: tauri::AppHandle, pinned: bool) -> Result<(), AppError> {
    handle
        .get_webview_window("translator")
        .ok_or_else(|| AppError::new("not_found", "查询窗口未打开。"))?
        .set_always_on_top(pinned)
        .map_err(|_| AppError::new("internal_error", "无法固定查询窗口。"))
}
#[tauri::command]
fn translation_open(handle: tauri::AppHandle) -> Result<(), AppError> {
    crate::desktop_query::open_input(&handle)
}
#[tauri::command]
fn translation_collect(
    app: AppState<'_>,
    handle: tauri::AppHandle,
    session_id: String,
    input_revision: u64,
    provider_id: String,
) -> Result<(), AppError> {
    let seed = app.query_collection(&session_id, input_revision, &provider_id)?;
    handle
        .emit_to("main", "capture_completed", seed)
        .map_err(|_| AppError::new("internal_error", "无法打开收录确认。"))?;
    crate::desktop_query::hide_query(&handle);
    desktop_capture::show_main(&handle);
    Ok(())
}
#[tauri::command]
fn query_copy(handle: tauri::AppHandle, text: String) -> Result<(), AppError> {
    handle
        .state::<crate::desktop_query::ClipboardWatcher>()
        .copy(&text)
}
#[tauri::command]
fn clipboard_listener_get(handle: tauri::AppHandle) -> crate::desktop_query::ClipboardStatus {
    handle
        .state::<crate::desktop_query::ClipboardWatcher>()
        .status()
}
#[tauri::command]
fn clipboard_listener_set(
    handle: tauri::AppHandle,
    enabled: bool,
    preferences: crate::desktop_query::ClipboardPreferences,
) -> Result<crate::desktop_query::ClipboardStatus, AppError> {
    let status = handle
        .state::<crate::desktop_query::ClipboardWatcher>()
        .set(&handle, enabled, preferences)?;
    crate::desktop_query::refresh_clipboard_menu(&handle);
    Ok(status)
}
#[tauri::command]
fn providers_list(app: AppState<'_>) -> Result<Vec<crate::providers::Provider>, AppError> {
    app.store.providers()
}
#[tauri::command]
async fn providers_update(
    app: AppState<'_>,
    update: crate::providers::ProviderUpdate,
) -> Result<Vec<crate::providers::Provider>, AppError> {
    let app = app.inner().clone();
    background(move || {
        let id = update.id.clone();
        let providers = app.store.provider_update(update)?;
        app.queries.cancel_provider(&id);
        Ok(providers)
    })
    .await
}
#[tauri::command]
async fn plugin_install(
    app: AppState<'_>,
    path: String,
) -> Result<crate::providers::Provider, AppError> {
    let app = app.inner().clone();
    background(move || {
        let metadata = std::fs::metadata(&path)?;
        if !metadata.is_file() || metadata.len() > 512_000 {
            return Err(AppError::new(
                "invalid_input",
                "请选择小于 512 KB 的插件 JSON 配置。",
            ));
        }
        let manifest = serde_json::from_str(&std::fs::read_to_string(path)?)
            .map_err(|_| AppError::new("invalid_data", "插件配置不是支持的 JSON 格式。"))?;
        let provider = app.store.plugin_install_manifest(manifest)?;
        app.queries.cancel_provider(&provider.id);
        Ok(provider)
    })
    .await
}
#[tauri::command]
async fn plugin_remove(
    app: AppState<'_>,
    plugin_id: String,
) -> Result<Vec<crate::providers::Provider>, AppError> {
    let app = app.inner().clone();
    background(move || {
        let providers = app.store.plugin_remove(&plugin_id)?;
        app.queries.cancel_provider(&plugin_id);
        Ok(providers)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    data_directory: String,
    schema_version: i64,
    entry_count: i64,
    source_count: i64,
    network_unavailable: bool,
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
            version: env!("CARGO_PKG_VERSION").into(),
            network_unavailable: app.network_unavailable(),
            data_directory: app.store.root().display().to_string(),
            schema_version: connection
                .pragma_query_value(None, "user_version", |row| row.get(0))?,
            entry_count: connection.query_row(
                "SELECT COUNT(*) FROM entries WHERE archived=0",
                [],
                |row| row.get(0),
            )?,
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
    options: Option<crate::media::ImportOptions>,
) -> Result<TaskSnapshot, AppError> {
    app.import_with_options(paths, operation_id, options.unwrap_or_default())
}
#[tauri::command]
async fn media_inspect(
    app: AppState<'_>,
    path: String,
) -> Result<crate::media::MediaInspection, AppError> {
    let tools = app.settings()?.tools;
    background(move || crate::media::inspect(&tools, std::path::Path::new(&path))).await
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
fn tasks_history(
    app: AppState<'_>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<crate::tasks::TaskHistoryPage, AppError> {
    app.tasks.history(offset.unwrap_or(0), limit.unwrap_or(10))
}
#[tauri::command]
async fn task_cancel(app: AppState<'_>, task_id: String) -> Result<TaskSnapshot, AppError> {
    let app = app.inner().clone();
    background(move || {
        let task = app.tasks.get(&task_id)?;
        if task.kind == "explanation_batch" && !task.terminal() {
            app.pause_preparation()?;
        }
        app.tasks.cancel(&task_id)
    })
    .await
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn candidates_list(
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
fn known_target_get(app: AppState<'_>, kind: String, text: String) -> Result<bool, AppError> {
    app.store.is_known_target(&kind, &text)
}
#[tauri::command]
fn known_target_set(
    app: AppState<'_>,
    kind: String,
    text: String,
    known: bool,
) -> Result<(), AppError> {
    app.store.set_known_target(&kind, &text, known)?;
    if !known {
        app.preparation_requested
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}
#[tauri::command]
async fn known_targets_list(
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
async fn review_units(
    app: AppState<'_>,
    mode: String,
    dimension: String,
    offset: u32,
    limit: u32,
) -> Result<Vec<ReviewUnit>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_units(&mode, &dimension, offset, limit)).await
}
#[tauri::command]
async fn review_question(
    app: AppState<'_>,
    unit_id: String,
    expected_revision: i64,
) -> Result<Question, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_question(&unit_id, expected_revision)).await
}
#[tauri::command]
fn review_hint(app: AppState<'_>, question_id: String) -> Result<String, AppError> {
    app.store.review_hint(&question_id)
}
#[tauri::command]
async fn review_submit(app: AppState<'_>, input: AnswerInput) -> Result<Attempt, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_submit(&input)).await
}
#[tauri::command]
async fn review_correct(app: AppState<'_>, input: CorrectionInput) -> Result<Attempt, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_correct(&input)).await
}
#[tauri::command]
async fn review_history(
    app: AppState<'_>,
    unit_id: Option<String>,
    offset: u32,
    limit: u32,
) -> Result<Vec<Attempt>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.review_history(unit_id.as_deref(), offset, limit)).await
}
#[tauri::command]
fn review_audio_start(
    app: AppState<'_>,
    question_id: String,
    operation_id: String,
) -> Result<TaskSnapshot, AppError> {
    app.review_audio_start(question_id, operation_id)
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
async fn explanations_status(
    app: AppState<'_>,
) -> Result<crate::preparation::PreparationStatus, AppError> {
    let app = app.inner().clone();
    background(move || app.preparation_status()).await
}
#[tauri::command]
async fn explanations_prepare(
    app: AppState<'_>,
    source_id: Option<String>,
) -> Result<crate::preparation::PreparationReply, AppError> {
    let app = app.inner().clone();
    background(move || app.prepare_explanations(source_id, true)).await
}
#[tauri::command]
async fn explanation_get(
    app: AppState<'_>,
    text: String,
    context: String,
) -> Result<Option<crate::explanations::PreparedExplanation>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.prepared_explanation(&text, &context)).await
}
#[tauri::command]
async fn explanation_import(
    app: AppState<'_>,
    text: String,
    context: String,
    value: crate::explanations::Explanation,
    model_url: String,
    model_name: String,
) -> Result<crate::explanations::Explanation, AppError> {
    let app = app.inner().clone();
    background(move || {
        app.store
            .save_explanation(&text, &context, &value, &model_url, &model_name)
    })
    .await
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
fn online_query_start(
    app: AppState<'_>,
    text: String,
    provider: Option<String>,
    operation_id: String,
) -> Result<TaskSnapshot, AppError> {
    app.online_query_start(
        text,
        provider.unwrap_or_else(|| "dictionary".into()),
        operation_id,
    )
}
#[tauri::command]
fn settings_get(app: AppState<'_>) -> Result<Settings, AppError> {
    app.settings()
}
#[tauri::command]
async fn practice_get(
    app: AppState<'_>,
) -> Result<Option<crate::practice::PracticeState>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.practice_get()).await
}
#[tauri::command]
async fn practice_import(
    app: AppState<'_>,
    document: crate::practice::PracticeDocument,
) -> Result<crate::practice::PracticeState, AppError> {
    let app = app.inner().clone();
    background(move || app.store.practice_import(document)).await
}
#[tauri::command]
async fn practice_progress_save(
    app: AppState<'_>,
    document_id: String,
    progress: crate::practice::PracticeProgress,
) -> Result<(), AppError> {
    let app = app.inner().clone();
    background(move || app.store.practice_progress_save(&document_id, progress)).await
}
#[tauri::command]
async fn practice_audio_save(
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
#[tauri::command]
async fn practice_example(
    app: AppState<'_>,
    document_id: String,
    chapter: usize,
    sentence: usize,
) -> Result<CandidateExample, AppError> {
    let app = app.inner().clone();
    background(move || app.store.practice_example(&document_id, chapter, sentence)).await
}
#[tauri::command]
fn connection_status(app: AppState<'_>) -> Result<ConnectionStatus, AppError> {
    app.connection_status()
}
#[tauri::command]
fn connection_start(
    app: AppState<'_>,
    operation_id: String,
    replace: bool,
) -> Result<TaskSnapshot, AppError> {
    app.connection_start(operation_id, replace)
}
#[tauri::command]
fn connection_check(app: AppState<'_>, operation_id: String) -> Result<TaskSnapshot, AppError> {
    app.connection_check(operation_id)
}
#[tauri::command]
fn connection_open(app: AppState<'_>) -> Result<(), AppError> {
    app.connection_open()
}
#[tauri::command]
fn sync_status(app: AppState<'_>) -> Result<crate::synchronization::SyncStatus, AppError> {
    app.sync_status()
}
#[tauri::command]
fn sync_conflicts(
    app: AppState<'_>,
) -> Result<Vec<crate::synchronization::SyncConflict>, AppError> {
    app.sync_conflicts()
}
#[tauri::command]
fn sync_start(app: AppState<'_>, operation_id: String) -> Result<TaskSnapshot, AppError> {
    app.inner().sync_start(operation_id)
}
#[tauri::command]
fn sync_resolve(
    app: AppState<'_>,
    input: crate::synchronization::ResolutionInput,
    operation_id: String,
) -> Result<TaskSnapshot, AppError> {
    app.inner().sync_resolve(input, operation_id)
}
#[tauri::command]
async fn settings_update(
    app: AppState<'_>,
    handle: tauri::AppHandle,
    settings: Settings,
) -> Result<Settings, AppError> {
    let app = app.inner().clone();
    background(move || {
        let previous = app.settings()?;
        let native = handle.state::<NativeDesktop>();
        native.configure(
            &handle,
            &settings.selection_shortcut,
            &settings.ocr_shortcut,
            &settings.input_shortcut,
            &settings.replacement_shortcut,
        )?;
        match app.save_settings(settings) {
            Ok(settings) => {
                if settings.replacement_shortcut != previous.replacement_shortcut
                    || settings.replacement_provider_id != previous.replacement_provider_id
                {
                    handle
                        .state::<crate::desktop_replacement::NativeReplacement>()
                        .cancel();
                }
                Ok(settings)
            }
            Err(error) => {
                let _ = native.configure(
                    &handle,
                    &previous.selection_shortcut,
                    &previous.ocr_shortcut,
                    &previous.input_shortcut,
                    &previous.replacement_shortcut,
                );
                Err(error)
            }
        }
    })
    .await
}

#[tauri::command]
async fn speech_voices() -> Result<Vec<SystemVoice>, AppError> {
    background(crate::windows_native::system_voices).await
}
#[tauri::command]
fn speech_start(
    app: AppState<'_>,
    text: String,
    operation_id: String,
    voice_id: Option<String>,
) -> Result<TaskSnapshot, AppError> {
    app.speech_start(text, operation_id, voice_id)
}
#[tauri::command]
fn native_status(handle: tauri::AppHandle) -> NativeStatus {
    handle.state::<NativeDesktop>().status()
}
#[tauri::command]
fn replacement_status_get(
    handle: tauri::AppHandle,
) -> crate::desktop_replacement::ReplacementStatus {
    handle
        .state::<crate::desktop_replacement::NativeReplacement>()
        .status()
}
#[tauri::command]
fn replacement_cancel(handle: tauri::AppHandle, operation_id: String) {
    let native = handle.state::<crate::desktop_replacement::NativeReplacement>();
    if native.status().operation_id == operation_id {
        native.cancel();
    }
}
#[tauri::command]
fn replacement_result_open(handle: tauri::AppHandle) -> Result<(), AppError> {
    handle
        .state::<crate::desktop_replacement::NativeReplacement>()
        .open_result(&handle)
}
#[tauri::command]
fn replacement_clipboard_restore(handle: tauri::AppHandle) -> Result<(), AppError> {
    handle
        .state::<crate::desktop_replacement::NativeReplacement>()
        .restore_clipboard(&handle)
}
#[tauri::command]
fn capture_selection(handle: tauri::AppHandle) -> Result<TaskSnapshot, AppError> {
    handle.state::<NativeDesktop>().capture(&handle)
}
#[tauri::command]
fn app_quit(handle: tauri::AppHandle) {
    desktop_capture::quit(&handle);
}
#[tauri::command]
fn capture_ocr(handle: tauri::AppHandle) -> Result<TaskSnapshot, AppError> {
    handle.state::<NativeDesktop>().ocr_start(&handle)
}
#[tauri::command]
fn capture_session_get(
    handle: tauri::AppHandle,
    session_id: String,
) -> Result<serde_json::Value, AppError> {
    handle.state::<NativeDesktop>().ocr_session(&session_id)
}
#[tauri::command]
fn capture_ocr_cancel(handle: tauri::AppHandle, session_id: String) {
    handle
        .state::<NativeDesktop>()
        .ocr_cancel(&handle, &session_id);
}
#[tauri::command]
fn capture_ocr_submit(
    handle: tauri::AppHandle,
    session_id: String,
    rect: crate::ocr::PixelRect,
) -> Result<TaskSnapshot, AppError> {
    handle
        .state::<NativeDesktop>()
        .ocr_submit(&handle, session_id, rect)
}

pub fn run() {
    let _startup = std::time::Instant::now();
    let root = crate::app_paths::data_directory().expect("Cannot resolve the data directory");
    crate::app_paths::prepare_process(&root).expect("Cannot prepare the data and cache directories");
    let mut context = tauri::generate_context!();
    let main_config = context.config_mut().app.windows.remove(0);
    let first_page = Arc::new(std::sync::atomic::AtomicBool::new(true));
    tauri::Builder::default()
        .on_page_load(move |webview, event| {
            if webview.label()=="translator" && event.event()==tauri::webview::PageLoadEvent::Finished {let _=webview.window().show();}
            if webview.label() == "main" {
                #[cfg(debug_assertions)]
                eprintln!(
                    "[startup] webview {:?}: {} ms",
                    event.event(),
                    _startup.elapsed().as_millis()
                );
                if event.event() == tauri::webview::PageLoadEvent::Finished
                    && first_page.swap(false, std::sync::atomic::Ordering::AcqRel)
                {
                    let window = webview.window();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if let Some(native) = app.try_state::<NativeDesktop>() && matches!(native.action(shortcut), Some(crate::desktop_capture::CaptureAction::Replacement)) {
                        if event.state() == ShortcutState::Released { native.replacement_key_down.store(false, std::sync::atomic::Ordering::Release); }
                        else if !native.replacement_key_down.swap(true, std::sync::atomic::Ordering::AcqRel) { app.state::<crate::desktop_replacement::NativeReplacement>().toggle(app); }
                        return;
                    }
                    if event.state() == ShortcutState::Pressed
                        && let Some(native) = app.try_state::<NativeDesktop>()
                    {
                        let result = match native.action(shortcut) {
                            Some(crate::desktop_capture::CaptureAction::Selection) => {
                                Some(native.capture(app))
                            }
                            Some(crate::desktop_capture::CaptureAction::Ocr) => {
                                Some(native.ocr_start(app))
                            }
                            Some(crate::desktop_capture::CaptureAction::Input) => {
                                if let Err(error)=crate::desktop_query::open_input(app){desktop_capture::capture_failed(app,&error);}
                                None
                            }
                            Some(crate::desktop_capture::CaptureAction::Replacement) => None,
                            None => None,
                        };
                        if let Some(Err(error)) = result
                            && error.code != "capture_busy"
                        {
                            desktop_capture::capture_failed(app, &error);
                        }
                    }
                })
                .build(),
        )
        .setup(move |app| {
            // Use the builder's absolute path API; window-config dataDirectory
            // is resolved relative to the system local-data directory by Tauri.
            tauri::WebviewWindowBuilder::from_config(app.handle(), &main_config)?
                .data_directory(crate::app_paths::webview_directory(&root))
                .build()?;
            #[cfg(debug_assertions)]
            eprintln!(
                "[startup] native setup begins: {} ms",
                _startup.elapsed().as_millis()
            );
            if let Some(window) = app.get_webview_window("main")
                && let Some(monitor) = window.current_monitor()?.or(app.primary_monitor()?)
            {
                let work = monitor.work_area();
                let scale = monitor.scale_factor();
                let size = crate::desktop_layout::initial_window_size(
                    work.size.width as f64 / scale,
                    work.size.height as f64 / scale,
                );
                window.set_min_size(Some(tauri::LogicalSize::new(
                    size.min_width,
                    size.min_height,
                )))?;
                window.set_size(tauri::LogicalSize::new(size.width, size.height))?;
                let outer = window.outer_size()?;
                window.set_position(tauri::PhysicalPosition::new(
                    work.position.x + (work.size.width.saturating_sub(outer.width) / 2) as i32,
                    work.position.y + (work.size.height.saturating_sub(outer.height) / 2) as i32,
                ))?;
            }
            #[cfg(debug_assertions)]
            eprintln!(
                "[startup] window geometry ready: {} ms",
                _startup.elapsed().as_millis()
            );
            let store = Arc::new(Store::open(root)?);
            #[cfg(debug_assertions)]
            eprintln!(
                "[startup] database ready: {} ms",
                _startup.elapsed().as_millis()
            );
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
            #[cfg(debug_assertions)]
            let tools = crate::media::MediaTools::development(runtime);
            #[cfg(not(debug_assertions))]
            let tools = crate::media::MediaTools::bundled(runtime);
            let defaults = Settings {
                tools,
                ..Default::default()
            };
            let application = Arc::new(Application::new(store, tasks, defaults));
            let settings = application.settings()?;
            application.start_preparation_scheduler();
            let query_handle=app.handle().clone();
            application.queries.observe(Arc::new(move|session|{let _=query_handle.emit_to("translator","query_updated",serde_json::json!({"sessionId":session.session_id,"inputRevision":session.input_revision,"queryId":session.query_id,"origin":session.input.origin.kind}));}));
            let clipboard=crate::desktop_query::ClipboardWatcher::default();clipboard.initialize(&application)?;app.manage(clipboard);
            app.manage(application);
            app.manage(NativeDesktop::default());
            app.manage(TrayAction::default());
            app.manage(crate::desktop_query::QueryWindowState::default());
            app.manage(crate::desktop_replacement::NativeReplacement::default());
            let native = app.state::<NativeDesktop>();
            let mut available = ["", "", "", ""];
            let mut failures = Vec::new();
            for (index, shortcut) in [
                settings.selection_shortcut.as_str(),
                settings.ocr_shortcut.as_str(),
                settings.input_shortcut.as_str(),
                settings.replacement_shortcut.as_str(),
            ]
            .into_iter()
            .enumerate()
            {
                available[index] = shortcut;
                if let Err(error) =
                    native.configure(app.handle(), available[0], available[1], available[2], available[3])
                {
                    available[index] = "";
                    failures.push(error.message);
                }
            }
            if !failures.is_empty() {
                native.remember_error(&AppError::new("shortcut_unavailable", failures.join("\n")));
            }
            use tauri::{
                menu::{CheckMenuItem,Menu, MenuItem, PredefinedMenuItem},
                tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
            };
            let show = MenuItem::with_id(app, "show", "打开单词本", true, None::<&str>)?;
            let collect = MenuItem::with_id(app, "collect", "收录词条", true, None::<&str>)?;
            let query = MenuItem::with_id(app,"query","输入翻译",true,None::<&str>)?;
            let replacement = MenuItem::with_id(app,"replacement","翻译替换：尚无操作（查看）",true,None::<&str>)?;
            let replacement_cancel_item = MenuItem::with_id(app,"replacement_cancel","取消翻译替换",true,None::<&str>)?;
            app.manage(crate::desktop_replacement::ReplacementMenu(replacement.clone()));
            let clipboard = CheckMenuItem::with_id(app,"clipboard","监听剪贴板",true,false,None::<&str>)?;
            app.manage(crate::desktop_query::ClipboardMenu(clipboard.clone()));
            let ocr = MenuItem::with_id(app, "ocr", "截图收录", true, None::<&str>)?;
            let import = MenuItem::with_id(app, "import", "导入剧集", true, None::<&str>)?;
            let episodes = MenuItem::with_id(app, "episodes", "观看前预习", true, None::<&str>)?;
            let practice = MenuItem::with_id(app, "practice", "听读练习", true, None::<&str>)?;
            let reviews = MenuItem::with_id(app, "reviews", "主动回忆测验", true, None::<&str>)?;
            let leech = MenuItem::with_id(app, "leech", "易忘词专项", true, None::<&str>)?;
            let sync = MenuItem::with_id(app, "sync", "站点同步", true, None::<&str>)?;
            let tasks = MenuItem::with_id(app, "tasks", "后台任务", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "本地设置", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "完全退出", true, None::<&str>)?;
            let capture_separator = PredefinedMenuItem::separator(app)?;
            let learning_separator = PredefinedMenuItem::separator(app)?;
            let settings_separator = PredefinedMenuItem::separator(app)?;
            let exit_separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(
                app,
                &[
                    &show,
                    &capture_separator,
                    &query,
                    &replacement,
                    &replacement_cancel_item,
                    &collect,
                    &ocr,
                    &import,
                    &learning_separator,
                    &episodes,
                    &practice,
                    &reviews,
                    &leech,
                    &settings_separator,
                    &sync,
                    &tasks,
                    &settings,
                    &clipboard,
                    &exit_separator,
                    &exit,
                ],
            )?;
            let icon = image::load_from_memory(include_bytes!("../icons/32x32.png"))?.into_rgba8();
            TrayIconBuilder::with_id("main-tray")
                .icon(tauri::image::Image::new_owned(icon.into_raw(), 32, 32))
                .tooltip("SubtitleVocabularyList")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } | TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        }
                    ) {
                        desktop_capture::show_main(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "replacement"=>{if let Err(error)=app.state::<crate::desktop_replacement::NativeReplacement>().open_result(app){desktop_capture::capture_failed(app,&error);}},
                    "replacement_cancel"=>app.state::<crate::desktop_replacement::NativeReplacement>().cancel(),
                    "query"=>{if let Err(error)=crate::desktop_query::open_input(app){desktop_capture::capture_failed(app,&error);}},
                    "clipboard"=>{let watcher=app.state::<crate::desktop_query::ClipboardWatcher>();let status=watcher.status();let _=watcher.set(app,!status.enabled,crate::desktop_query::ClipboardPreferences{debounce_ms:status.debounce_ms,persist_results:status.persist_results});crate::desktop_query::refresh_clipboard_menu(app);},
                    "show" => tray_navigate(app, "library"),
                    "collect" | "import" | "episodes" | "practice" | "reviews" | "leech"
                    | "sync" | "tasks" | "settings" => tray_navigate(app, event.id.as_ref()),
                    "ocr" => {
                        if let Err(error) = app.state::<NativeDesktop>().ocr_start(app) {
                            desktop_capture::capture_failed(app, &error);
                        }
                    }
                    "exit" => desktop_capture::quit(app),
                    _ => {}
                })
                .build(app)?;
            #[cfg(debug_assertions)]
            eprintln!(
                "[startup] services ready: {} ms",
                _startup.elapsed().as_millis()
            );
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label()=="translator" && let tauri::WindowEvent::CloseRequested{api,..}=event{api.prevent_close();crate::desktop_query::hide_query(window.app_handle());}
            if let Some(id) = window.label().strip_prefix("capture-")
                && matches!(event, tauri::WindowEvent::CloseRequested { .. })
            {
                window.app_handle().state::<NativeDesktop>().ocr_dismiss(id);
            }
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let app = window.app_handle();
                if app
                    .state::<Arc<Application>>()
                    .settings()
                    .is_ok_and(|settings| settings.close_to_tray)
                {
                    let _ = window.hide();
                } else {
                    desktop_capture::quit(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            assessment_preview,assessment_start,assessment_session_get,assessment_session_list,
            assessment_answer_submit,assessment_session_transition,assessment_audio_start,
            assessment_audio_event,assessment_result_correct,
            assessment_catalog_get,assessment_settings_get,assessment_settings_update,
            assessment_bank_prepare,assessment_bank_review,assessment_bank_items,
            app_info,
            translation_open,translation_session_open,translation_session_get,translation_query_start,translation_query_cancel,
            translation_window_hide,translation_window_pin,translation_collect,query_copy,
            clipboard_listener_get,clipboard_listener_set,providers_list,providers_update,plugin_install,plugin_remove,
            sources_list,
            import_start,
            media_inspect,
            task_get,
            tasks_list,
            tasks_history,
            task_cancel,
            candidates_list,
            known_target_get,
            known_target_set,
            known_targets_list,
            candidate_examples,
            candidate_decide,
            source_example_update,
            entries_list,
            entry_get,
            entry_update,
            review_units,
            review_question,
            review_hint,
            review_submit,
            review_correct,
            review_history,
            review_audio_start,
            collection_prepare,
            collection_from_example,
            collection_commit,
            preview_start,
            media_path,
            explain_start,
            explanation_get,
            explanation_import,
            explanations_prepare,
            explanations_status,
            online_query_start,
            settings_get,
            practice_get,
            practice_import,
            practice_progress_save,
            practice_audio_save,
            practice_example,
            connection_status,
            connection_start,
            connection_check,
            connection_open,
            sync_status,
            sync_conflicts,
            sync_start,
            sync_resolve,
            settings_update,
            speech_voices,
            speech_start,
            native_status,
            replacement_status_get,replacement_cancel,replacement_result_open,replacement_clipboard_restore,
            tray_action_get,
            capture_selection,
            app_quit,
            capture_ocr,
            capture_session_get,
            capture_ocr_cancel,
            capture_ocr_submit
        ])
        .run(context)
        .expect("Cannot start SubtitleVocabularyList");
}
