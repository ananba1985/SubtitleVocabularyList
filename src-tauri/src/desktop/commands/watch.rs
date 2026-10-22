use super::{AppState, background};
use crate::online_subtitles as online;
use crate::{
    error::AppError,
    playback::PlaylistChange,
    player::{NativePlayer, PlayerSnapshot, WatchControl, WatchViewport},
    watch::WatchProgress,
};
use tauri::{Manager, State};

#[svl_ipc_macros::command]
pub(crate) async fn watch_open(
    app: AppState<'_>,
    handle: tauri::AppHandle,
    path: Option<String>,
    source_id: Option<String>,
) -> Result<PlayerSnapshot, AppError> {
    let application = app.inner().clone();
    background(move || {
        handle.state::<NativePlayer>().control(WatchControl {
            action: "hide".into(),
            value: None,
            text: None,
            expected_revision: None,
        })?;
        let media = application
            .store
            .watch_media(path.as_deref(), source_id.as_deref())?;
        handle.state::<NativePlayer>().open(&handle, media)
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn watch_state_get(
    app: AppState<'_>,
    player: State<'_, NativePlayer>,
) -> Result<PlayerSnapshot, AppError> {
    let mut state = player.state()?;
    let application = app.inner().clone();
    background(move || {
        if state.session_id.is_empty() {
            state.playback = application.store.playback_state()?;
        }
        Ok(state)
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn watch_playlist_update(
    app: AppState<'_>,
    handle: tauri::AppHandle,
    change: PlaylistChange,
) -> Result<PlayerSnapshot, AppError> {
    let application = app.inner().clone();
    background(move || {
        let items = if change.action == "append" {
            application.store.playlist_items(&change.paths)?
        } else {
            vec![]
        };
        handle
            .state::<NativePlayer>()
            .playlist(&handle, change, items)
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn watch_control(
    handle: tauri::AppHandle,
    control: WatchControl,
) -> Result<PlayerSnapshot, AppError> {
    background(move || {
        let player = handle.state::<NativePlayer>();
        if matches!(
            control.action.as_str(),
            "speed" | "volume" | "mute" | "auto_next"
        ) {
            player.control_ready(&handle, control)
        } else {
            player.control(control)
        }
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) fn watch_viewport_set(
    handle: tauri::AppHandle,
    viewport: WatchViewport,
) -> Result<(), AppError> {
    handle.state::<NativePlayer>().viewport(&handle, viewport)
}
#[svl_ipc_macros::command]
pub(crate) async fn watch_progress_get(
    app: AppState<'_>,
) -> Result<Option<WatchProgress>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.watch_progress()).await
}

#[svl_ipc_macros::command]
pub(crate) async fn watch_collection_prepare(
    app: AppState<'_>,
    handle: tauri::AppHandle,
    session_id: String,
    expected_revision: u64,
    clip_range: Option<crate::watch::WatchClipRange>,
) -> Result<crate::watch::WatchContext, AppError> {
    let app = app.inner().clone();
    background(move || {
        let state = handle.state::<NativePlayer>().state()?;
        if state.session_id != session_id || state.revision != expected_revision || state.playing {
            return Err(AppError::new(
                "conflict",
                "播放上下文已变化，请重新暂停后收录。",
            ));
        }
        let mut context = state
            .context
            .ok_or_else(|| AppError::new("not_found", "没有当前原句。"))?;
        if let Some(range) = clip_range {
            context = context.with_manual_clip(&range, (state.duration * 1000.0).round() as i64)?;
        }
        app.store
            .bind_watch_context(&state.path, &state.title, &context)
    })
    .await
}

#[svl_ipc_macros::command]
pub(crate) async fn subtitle_info_get(
    app: AppState<'_>,
    source_id: String,
) -> Result<Option<crate::subtitles::SubtitleInfo>, AppError> {
    let app = app.inner().clone();
    background(move || Ok(app.store.subtitle_bundle(&source_id)?.map(|b| b.info()))).await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_prepare_start(
    app: AppState<'_>,
    source_id: String,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<crate::subtitles::SubtitleInfo>, AppError> {
    let app = app.inner().clone();
    background(move || {
        let worker = app.clone();
        let hash = crate::vocabulary::digest(source_id.as_bytes());
        app.tasks
            .start_typed("subtitle_import", &operation_id, &hash, move |context| {
                context.subject("中英字幕取得");
                worker.store.prepare_source_subtitles(
                    &worker.settings()?.tools,
                    &source_id,
                    &context.cancelled,
                    &|stage, current, total| {
                        context.progress(stage, current, total, "正在取得本地中英字幕")
                    },
                )
            })
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_chinese_select(
    app: AppState<'_>,
    source_id: String,
    asset_id: Option<String>,
) -> Result<crate::subtitles::SubtitleInfo, AppError> {
    let app = app.inner().clone();
    background(move || app.store.select_chinese_subtitle(&source_id, asset_id)).await
}

#[svl_ipc_macros::command]
pub(crate) async fn subtitle_service_get(
    app: AppState<'_>,
) -> Result<online::SubtitleService, AppError> {
    let app = app.inner().clone();
    background(move || online::service(&app.store)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_service_set(
    app: AppState<'_>,
    config: online::SubtitleService,
    api_key: Option<String>,
) -> Result<online::SubtitleService, AppError> {
    let app = app.inner().clone();
    background(move || online::save_service(&app.store, config, api_key)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_search_context(
    app: AppState<'_>,
    source_id: String,
) -> Result<online::SubtitleQuery, AppError> {
    let app = app.inner().clone();
    background(move || online::hint(&app.store, &source_id)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_search_get(
    app: AppState<'_>,
    source_id: String,
) -> Result<Option<online::SubtitleSearch>, AppError> {
    let app = app.inner().clone();
    background(move || online::cached_search(&app.store, &source_id)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_search_start(
    app: AppState<'_>,
    source_id: String,
    query: online::SubtitleQuery,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<online::SubtitleSearch>, AppError> {
    let app = app.inner().clone();
    background(move || {
        if app.settings()?.offline_mode {
            return Err(AppError::new(
                "offline",
                "当前已禁止外网请求，本地字幕和观看仍可用。",
            ));
        }
        let hash = crate::vocabulary::digest(&serde_json::to_vec(&(&source_id, &query))?);
        let worker = app.clone();
        app.tasks
            .start_typed("subtitle_search", &operation_id, &hash, move |context| {
                context.subject(&query.title);
                online::search(
                    &worker.store,
                    &source_id,
                    query,
                    &context.cancelled,
                    &|current, total, message| {
                        context.progress("subtitle_search", current, total, message)
                    },
                )
            })
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_download_start(
    app: AppState<'_>,
    source_id: String,
    candidate_id: String,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<online::SubtitleDownload>, AppError> {
    let app = app.inner().clone();
    background(move || {
        if app.settings()?.offline_mode {
            return Err(AppError::new("offline", "当前已禁止外网请求。"));
        }
        let hash = crate::vocabulary::digest(&serde_json::to_vec(&(&source_id, &candidate_id))?);
        let worker = app.clone();
        app.tasks
            .start_typed("subtitle_download", &operation_id, &hash, move |context| {
                context.subject("字幕候选下载");
                context.progress("download", 0, 1, "正在下载所选字幕");
                let result =
                    online::download(&worker.store, &source_id, &candidate_id, &context.cancelled)?;
                context.progress("download", 1, 1, "字幕已下载，请选择文件并核对时间");
                Ok(result)
            })
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_download_preview(
    app: AppState<'_>,
    download_id: String,
    file_id: String,
    offset_ms: i64,
) -> Result<online::SubtitleDownloadPreview, AppError> {
    let app = app.inner().clone();
    background(move || {
        online::preview(
            &app.store,
            &app.settings()?.tools,
            &download_id,
            &file_id,
            offset_ms,
            &std::sync::atomic::AtomicBool::new(false),
        )
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_download_commit_start(
    app: AppState<'_>,
    download_id: String,
    file_id: String,
    offset_ms: i64,
    confirmed: bool,
    select_chinese: bool,
    operation_id: String,
) -> Result<crate::tasks::TaskHandle<crate::subtitles::SubtitleInfo>, AppError> {
    let app = app.inner().clone();
    background(move || {
        let hash = crate::vocabulary::digest(&serde_json::to_vec(&(
            &download_id,
            &file_id,
            offset_ms,
            confirmed,
            select_chinese,
        ))?);
        let worker = app.clone();
        app.tasks
            .start_typed("subtitle_import", &operation_id, &hash, move |context| {
                context.subject("字幕核对导入");
                context.progress("attach", 0, 1, "正在导入核对后的字幕");
                let result = online::commit(
                    &worker.store,
                    &worker.settings()?.tools,
                    &download_id,
                    &file_id,
                    offset_ms,
                    confirmed,
                    select_chinese,
                    &context.cancelled,
                )?;
                context.progress("attach", 1, 1, "字幕已导入，个人词库与历史保持");
                Ok(result)
            })
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn subtitle_alignment_set(
    app: AppState<'_>,
    source_id: String,
    asset_id: String,
    offset_ms: i64,
) -> Result<crate::subtitles::SubtitleInfo, AppError> {
    let app = app.inner().clone();
    background(move || {
        app.store
            .set_subtitle_offset(&source_id, &asset_id, offset_ms)
    })
    .await
}
