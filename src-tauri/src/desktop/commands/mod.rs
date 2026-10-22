pub(super) mod assessment;
pub(super) mod capture;
pub(super) mod corpus;
pub(super) mod learning;
pub(super) mod query;
pub(super) mod synchronization;
pub(super) mod system;
pub(super) mod vocabulary;
pub(super) mod watch;

use crate::{application::Application, error::AppError};
use std::sync::Arc;
use tauri::State;

type AppState<'a> = State<'a, Arc<Application>>;

async fn background<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| AppError::new("internal_error", "本次操作异常中断，已保存内容保留。"))?
}

async fn background_project<T, S>(
    work: impl FnOnce() -> Result<S, AppError> + Send + 'static,
) -> Result<T, AppError>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    S: serde::Serialize + Send + 'static,
{
    background(move || crate::ipc_dto::project(work()?)).await
}

// Keep all desktop IPC registrations at this single entry point.
svl_ipc_macros::catalog! {
        watch::watch_open,
        watch::watch_state_get,
        watch::watch_playlist_update,
        watch::watch_control,
        watch::watch_viewport_set,
        watch::watch_progress_get,
        watch::watch_collection_prepare,
        watch::subtitle_info_get,
        watch::subtitle_prepare_start,
        watch::subtitle_chinese_select,
        watch::subtitle_service_get,
        watch::subtitle_service_set,
        watch::subtitle_search_context,
        watch::subtitle_search_get,
        watch::subtitle_search_start,
        watch::subtitle_download_start,
        watch::subtitle_download_preview,
        watch::subtitle_download_commit_start,
        watch::subtitle_alignment_set,
        assessment::assessment_preview,
        assessment::assessment_start,
        assessment::assessment_session_get,
        assessment::assessment_session_list,
        assessment::assessment_answer_submit,
        assessment::assessment_session_transition,
        assessment::assessment_audio_start,
        assessment::assessment_audio_event,
        assessment::assessment_result_correct,
        assessment::assessment_report_get,
        assessment::assessment_report_items,
        assessment::assessment_history_list,
        assessment::assessment_compare,
        assessment::assessment_collection_prepare,
        assessment::assessment_collection_commit,
        assessment::assessment_catalog_get,
        assessment::assessment_settings_get,
        assessment::assessment_settings_update,
        assessment::assessment_bank_prepare,
        assessment::assessment_bank_review,
        assessment::assessment_bank_items,
        assessment::assessment_bank_audio_prepare,
        assessment::assessment_sync_status,
        assessment::assessment_sync_conflicts,
        assessment::assessment_sync_keep_both,
        assessment::assessment_audio_prepare,
        corpus::sources_list,
        corpus::import_start,
        corpus::media_inspect,
        corpus::candidates_list,
        corpus::known_target_get,
        corpus::known_target_set,
        corpus::known_targets_list,
        corpus::candidate_examples,
        corpus::candidate_decide,
        corpus::source_example_update,
        vocabulary::entries_list,
        vocabulary::entry_get,
        vocabulary::entry_update,
        vocabulary::collection_prepare,
        vocabulary::collection_from_example,
        vocabulary::collection_commit,
        vocabulary::preview_start,
        vocabulary::media_path,
        learning::review_units,
        learning::review_question,
        learning::review_hint,
        learning::review_submit,
        learning::review_correct,
        learning::review_history,
        learning::review_audio_start,
        learning::practice_get,
        learning::practice_import,
        learning::practice_progress_save,
        learning::practice_audio_save,
        learning::practice_example,
        learning::speech_voices,
        learning::speech_start,
        query::translation_session_open,
        query::translation_session_get,
        query::translation_query_start,
        query::translation_query_cancel,
        query::translation_window_hide,
        query::translation_window_pin,
        query::translation_open,
        query::translation_collect,
        query::query_copy,
        query::clipboard_listener_get,
        query::clipboard_listener_set,
        query::providers_list,
        query::providers_update,
        query::plugin_install,
        query::plugin_remove,
        query::explanations_status,
        query::explanations_prepare,
        query::explanation_get,
        query::explanation_import,
        query::explain_start,
        query::online_query_start,
        system::app_info,
        system::settings_get,
        system::settings_update,
        system::autostart_get,
        system::autostart_set,
        system::task_get,
        system::tasks_list,
        system::tasks_history,
        system::task_cancel,
        system::tray_action_get,
        system::app_quit,
        synchronization::connection_status,
        synchronization::connection_start,
        synchronization::connection_check,
        synchronization::connection_open,
        synchronization::sync_status,
        synchronization::sync_conflicts,
        synchronization::sync_start,
        synchronization::sync_resolve,
        capture::native_status,
        capture::replacement_status_get,
        capture::replacement_cancel,
        capture::replacement_result_open,
        capture::replacement_clipboard_restore,
        capture::capture_selection,
        capture::capture_ocr,
        capture::capture_session_get,
        capture::capture_ocr_cancel,
        capture::capture_ocr_submit,
}
