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
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};
use tauri_plugin_global_shortcut::ShortcutState;

type AppState<'a> = State<'a, Arc<Application>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
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
            version: env!("CARGO_PKG_VERSION").into(),
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
fn task_cancel(app: AppState<'_>, task_id: String) -> Result<TaskSnapshot, AppError> {
    app.tasks.cancel(&task_id)
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
    app.store.set_known_target(&kind, &text, known)
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
        )?;
        match app.save_settings(settings) {
            Ok(settings) => Ok(settings),
            Err(error) => {
                let _ = native.configure(
                    &handle,
                    &previous.selection_shortcut,
                    &previous.ocr_shortcut,
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
) -> Result<TaskSnapshot, AppError> {
    app.speech_start(text, operation_id)
}
#[tauri::command]
fn native_status(handle: tauri::AppHandle) -> NativeStatus {
    handle.state::<NativeDesktop>().status()
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
    let first_page = Arc::new(std::sync::atomic::AtomicBool::new(true));
    tauri::Builder::default()
        .on_page_load(move |webview, event| {
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
            #[cfg(debug_assertions)]
            let default_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .ok_or("Missing project directory")?
                .join(".local/dev-data");
            #[cfg(not(debug_assertions))]
            let default_root = app.path().app_data_dir()?;
            let root = std::env::var_os("SVL_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(default_root);
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
            app.manage(application);
            app.manage(NativeDesktop::default());
            if let Err(error) = app.state::<NativeDesktop>().configure(
                app.handle(),
                &settings.selection_shortcut,
                &settings.ocr_shortcut,
            ) {
                if app
                    .state::<NativeDesktop>()
                    .configure(app.handle(), &settings.selection_shortcut, "")
                    .is_err()
                {
                    let _ = app.state::<NativeDesktop>().configure(
                        app.handle(),
                        "",
                        &settings.ocr_shortcut,
                    );
                }
                app.state::<NativeDesktop>().remember_error(&error);
            }
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };
            let show = MenuItem::with_id(app, "show", "打开单词本", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &exit])?;
            let icon = image::load_from_memory(include_bytes!("../icons/32x32.png"))?.into_rgba8();
            TrayIconBuilder::with_id("main-tray")
                .icon(tauri::image::Image::new_owned(icon.into_raw(), 32, 32))
                .tooltip("SubtitleVocabularyList")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => desktop_capture::show_main(app),
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
            app_info,
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
            online_query_start,
            settings_get,
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
            capture_selection,
            app_quit,
            capture_ocr,
            capture_session_get,
            capture_ocr_cancel,
            capture_ocr_submit
        ])
        .run(tauri::generate_context!())
        .expect("Cannot start SubtitleVocabularyList");
}
