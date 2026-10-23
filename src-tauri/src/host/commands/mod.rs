#[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
pub(crate) mod assessment;
#[cfg(all(windows, feature = "desktop"))]
pub(crate) mod capture;
pub(crate) mod corpus;
pub(crate) mod subtitle_commands;
#[cfg(any(all(windows,feature="desktop"),all(target_os="android",feature="mobile")))]
pub(crate) mod episode_sharing;
pub(crate) mod learning;
pub(crate) mod mobile_media;
pub(crate) mod mobile_capture;
pub(crate) mod mobile_tools;
pub(crate) mod query;
#[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
pub(crate) mod synchronization;
pub(crate) mod system;
pub(crate) mod vocabulary;
#[cfg(all(windows, feature = "desktop"))]
pub(crate) mod watch;

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

// One command catalog for both hosts; platform dependencies stay explicitly scoped.
svl_ipc_macros::catalog! {
        system::app_ready,
        mobile_capture::mobile_incoming_get,
        mobile_tools::mobile_query_seed,
        mobile_tools::mobile_copy,
        mobile_tools::entry_review_history,
        mobile_tools::mobile_resources_get,
        mobile_tools::mobile_images_release,
        mobile_tools::mobile_images_cleanup,
        mobile_media::video_collection_draft,
        mobile_media::video_translation_set,
        mobile_capture::mobile_incoming_ack,
        mobile_capture::mobile_image_prepare,
        mobile_capture::mobile_image_pick,
        mobile_capture::mobile_video_frame,
        mobile_capture::mobile_ocr_start,
        mobile_capture::mobile_frame_preview,
        mobile_capture::mobile_frame_draft,
        mobile_capture::mobile_image_draft,
        mobile_capture::mobile_text_draft,
        mobile_capture::mobile_shared_video,
        mobile_capture::mobile_drafts_get,
        mobile_capture::mobile_drafts_save,
        mobile_media::video_library,
        mobile_media::video_learning_get,
        mobile_media::video_learning_visible,
        mobile_media::video_learning_skip,
        mobile_media::video_pick,
        mobile_media::video_folder_pick,
        mobile_media::video_folder_index,
        mobile_media::video_open,
        mobile_media::video_state,
        mobile_media::video_context,
        mobile_media::video_control,
        mobile_media::video_viewport,
        mobile_media::video_study,
        mobile_media::video_index_start,
        mobile_media::video_subtitle_add,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::device_sync_get,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::device_sync_update,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::device_sync_bind,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::connection_disconnect,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_open,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_related_videos,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_files_pending,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_files_open_next,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_state_get,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_playlist_update,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_control,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_viewport_set,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_progress_get,
        #[cfg(all(windows, feature = "desktop"))]
        watch::watch_collection_prepare,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_info_get,
        #[cfg(all(windows, feature = "desktop"))]
        subtitle_commands::subtitle_prepare_start,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_chinese_select,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_service_get,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_service_set,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_search_context,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_search_get,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_search_start,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_download_start,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_download_preview,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_download_commit_start,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        subtitle_commands::subtitle_alignment_set,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_preview,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_start,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_session_get,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_session_list,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_answer_submit,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_session_transition,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_audio_start,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_audio_event,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_result_correct,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_report_get,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_report_items,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_history_list,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_compare,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_collection_prepare,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_collection_commit,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_catalog_get,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_settings_get,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_settings_update,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_bank_prepare,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_bank_review,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_bank_items,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_bank_audio_prepare,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::assessment_sync_status,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::assessment_sync_conflicts,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::assessment_sync_keep_both,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        assessment::assessment_audio_prepare,
        corpus::sources_list,
        #[cfg(all(windows, feature = "desktop"))]
        corpus::import_start,
        #[cfg(all(windows, feature = "desktop"))]
        corpus::media_inspect,
        corpus::candidates_list,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        corpus::known_target_get,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        corpus::known_target_set,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
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
        learning::audio_open,
        learning::audio_state,
        learning::audio_control,
        learning::speech_settings_open,
        learning::review_units,
        learning::review_question,
        learning::review_hint,
        learning::review_submit,
        learning::review_correct,
        learning::review_history,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        learning::review_audio_start,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        learning::practice_get,
        learning::practice_file_pick,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        learning::practice_import,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        learning::practice_progress_save,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        learning::practice_audio_save,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        learning::practice_example,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        learning::speech_voices,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        learning::speech_start,
        query::translation_session_open,
        query::translation_session_get,
        query::translation_query_start,
        query::translation_query_cancel,
        #[cfg(all(windows, feature = "desktop"))]
        query::translation_window_hide,
        #[cfg(all(windows, feature = "desktop"))]
        query::translation_window_pin,
        #[cfg(all(windows, feature = "desktop"))]
        query::translation_open,
        #[cfg(all(windows, feature = "desktop"))]
        query::translation_collect,
        #[cfg(all(windows, feature = "desktop"))]
        query::query_copy,
        #[cfg(all(windows, feature = "desktop"))]
        query::clipboard_listener_get,
        #[cfg(all(windows, feature = "desktop"))]
        query::clipboard_listener_set,
        query::providers_list,
        query::providers_update,
        #[cfg(all(windows, feature = "desktop"))]
        query::plugin_install,
        #[cfg(all(windows, feature = "desktop"))]
        query::plugin_remove,
        #[cfg(all(windows, feature = "desktop"))]
        query::explanations_status,
        #[cfg(all(windows, feature = "desktop"))]
        query::explanations_prepare,
        query::explanation_get,
        query::explanation_import,
        query::explain_start,
        query::online_query_start,
        system::app_capabilities,
        system::app_info,
        system::settings_get,
        system::settings_update,
        #[cfg(all(windows, feature = "desktop"))]
        system::autostart_get,
        #[cfg(all(windows, feature = "desktop"))]
        system::autostart_set,
        system::task_get,
        system::tasks_list,
        system::tasks_history,
        system::task_cancel,
        #[cfg(all(windows, feature = "desktop"))]
        system::tray_action_get,
        #[cfg(all(windows, feature = "desktop"))]
        system::app_quit,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::connection_status,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::connection_start,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::connection_check,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::connection_open,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::sync_status,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::sync_conflicts,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::sync_start,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::sync_metered_start,
        #[cfg(any(all(windows,feature="desktop"),all(target_os="android",feature="mobile")))]
        episode_sharing::prepared_episode_local,
        #[cfg(any(all(windows,feature="desktop"),all(target_os="android",feature="mobile")))]
        episode_sharing::prepared_episode_check,
        #[cfg(any(all(windows,feature="desktop"),all(target_os="android",feature="mobile")))]
        episode_sharing::prepared_episode_publish,
        #[cfg(any(all(windows,feature="desktop"),all(target_os="android",feature="mobile")))]
        episode_sharing::prepared_episode_download,
        #[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
        synchronization::sync_resolve,
        #[cfg(all(windows, feature = "desktop"))]
        capture::native_status,
        #[cfg(all(windows, feature = "desktop"))]
        capture::replacement_status_get,
        #[cfg(all(windows, feature = "desktop"))]
        capture::replacement_cancel,
        #[cfg(all(windows, feature = "desktop"))]
        capture::replacement_result_open,
        #[cfg(all(windows, feature = "desktop"))]
        capture::replacement_clipboard_restore,
        #[cfg(all(windows, feature = "desktop"))]
        capture::capture_selection,
        #[cfg(all(windows, feature = "desktop"))]
        capture::capture_ocr,
        #[cfg(all(windows, feature = "desktop"))]
        capture::capture_session_get,
        #[cfg(all(windows, feature = "desktop"))]
        capture::capture_ocr_cancel,
        #[cfg(all(windows, feature = "desktop"))]
        capture::capture_ocr_submit,
}
