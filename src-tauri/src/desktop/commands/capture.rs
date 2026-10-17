use crate::error::AppError;
use crate::{
    desktop_capture::{NativeDesktop, NativeStatus},
    tasks::TaskSnapshot,
};
use tauri::Manager;

#[tauri::command]
pub(crate) fn native_status(handle: tauri::AppHandle) -> NativeStatus {
    handle.state::<NativeDesktop>().status()
}

#[tauri::command]
pub(crate) fn replacement_status_get(
    handle: tauri::AppHandle,
) -> crate::desktop_replacement::ReplacementStatus {
    handle
        .state::<crate::desktop_replacement::NativeReplacement>()
        .status()
}

#[tauri::command]
pub(crate) fn replacement_cancel(handle: tauri::AppHandle, operation_id: String) {
    let native = handle.state::<crate::desktop_replacement::NativeReplacement>();
    if native.status().operation_id == operation_id {
        native.cancel();
    }
}

#[tauri::command]
pub(crate) fn replacement_result_open(handle: tauri::AppHandle) -> Result<(), AppError> {
    handle
        .state::<crate::desktop_replacement::NativeReplacement>()
        .open_result(&handle)
}

#[tauri::command]
pub(crate) fn replacement_clipboard_restore(handle: tauri::AppHandle) -> Result<(), AppError> {
    handle
        .state::<crate::desktop_replacement::NativeReplacement>()
        .restore_clipboard(&handle)
}

#[tauri::command]
pub(crate) fn capture_selection(handle: tauri::AppHandle) -> Result<TaskSnapshot, AppError> {
    handle.state::<NativeDesktop>().capture(&handle)
}

#[tauri::command]
pub(crate) fn capture_ocr(handle: tauri::AppHandle) -> Result<TaskSnapshot, AppError> {
    handle.state::<NativeDesktop>().ocr_start(&handle)
}

#[tauri::command]
pub(crate) fn capture_session_get(
    handle: tauri::AppHandle,
    session_id: String,
) -> Result<serde_json::Value, AppError> {
    handle.state::<NativeDesktop>().ocr_session(&session_id)
}

#[tauri::command]
pub(crate) fn capture_ocr_cancel(handle: tauri::AppHandle, session_id: String) {
    handle
        .state::<NativeDesktop>()
        .ocr_cancel(&handle, &session_id);
}

#[tauri::command]
pub(crate) fn capture_ocr_submit(
    handle: tauri::AppHandle,
    session_id: String,
    rect: crate::ocr::PixelRect,
) -> Result<TaskSnapshot, AppError> {
    handle
        .state::<NativeDesktop>()
        .ocr_submit(&handle, session_id, rect)
}
