use super::{AppState, background};
use crate::{
    error::AppError,
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
        let media = application
            .store
            .watch_media(path.as_deref(), source_id.as_deref())?;
        handle.state::<NativePlayer>().open(&handle, media)
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) fn watch_state_get(player: State<'_, NativePlayer>) -> Result<PlayerSnapshot, AppError> {
    player.state()
}
#[svl_ipc_macros::command]
pub(crate) async fn watch_control(
    handle: tauri::AppHandle,
    control: WatchControl,
) -> Result<PlayerSnapshot, AppError> {
    background(move || handle.state::<NativePlayer>().control(control)).await
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
