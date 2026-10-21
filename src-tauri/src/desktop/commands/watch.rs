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
