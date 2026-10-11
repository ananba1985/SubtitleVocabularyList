use crate::{
    application::Application,
    error::AppError,
    queries::{QueryInput, QueryOrigin},
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use windows::{
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
        System::{DataExchange::*, Memory::*},
        UI::WindowsAndMessaging::*,
    },
    core::w,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClipboardPreferences {
    pub debounce_ms: u64,
    pub persist_results: bool,
}
impl Default for ClipboardPreferences {
    fn default() -> Self {
        Self {
            debounce_ms: 300,
            persist_results: true,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardStatus {
    pub enabled: bool,
    pub debounce_ms: u64,
    pub persist_results: bool,
    pub error: Option<String>,
}
pub struct ClipboardMenu(pub tauri::menu::CheckMenuItem<tauri::Wry>);
#[derive(Default)]
pub struct QueryWindowState {
    creating: Arc<AtomicBool>,
    focus: Arc<AtomicBool>,
}
pub fn refresh_clipboard_menu(app: &AppHandle) {
    let status = app.state::<ClipboardWatcher>().status();
    if let Some(item) = app.try_state::<ClipboardMenu>() {
        let _ = item.0.set_checked(status.enabled);
    }
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(if status.enabled {
            "SubtitleVocabularyList · 正在监听剪贴板"
        } else {
            "SubtitleVocabularyList"
        }));
    }
    let _ = app.emit_to("translator", "clipboard_changed", ());
}

#[derive(Default)]
pub struct ClipboardWatcher {
    enabled: Arc<AtomicBool>,
    ignore_sequence: Arc<AtomicU32>,
    epoch: Arc<AtomicU32>,
    preferences: Mutex<ClipboardPreferences>,
    error: Arc<Mutex<Option<String>>>,
    dispatch: Arc<Mutex<()>>,
}
impl ClipboardWatcher {
    pub fn status(&self) -> ClipboardStatus {
        let prefs = self.preferences.lock().unwrap();
        ClipboardStatus {
            enabled: self.enabled.load(Ordering::Relaxed),
            debounce_ms: prefs.debounce_ms,
            persist_results: prefs.persist_results,
            error: self.error.lock().unwrap().clone(),
        }
    }
    pub fn stop(&self, app: &AppHandle) {
        let _guard = self.dispatch.lock().unwrap();
        self.enabled.store(false, Ordering::Relaxed);
        self.epoch.fetch_add(1, Ordering::Relaxed);
        if let Some(session) = app.state::<Arc<Application>>().queries.owner("clipboard") {
            let _ = app
                .state::<Arc<Application>>()
                .queries
                .cancel(&session.session_id, None);
        }
    }
    pub fn set(
        &self,
        app: &AppHandle,
        enabled: bool,
        prefs: ClipboardPreferences,
    ) -> Result<ClipboardStatus, AppError> {
        if !(100..=2000).contains(&prefs.debounce_ms) {
            return Err(AppError::new(
                "invalid_input",
                "合并间隔应为 100～2000 毫秒。",
            ));
        }
        self.stop(app);
        *self.preferences.lock().unwrap() = prefs.clone();
        *self.error.lock().unwrap() = None;
        let application = app.state::<Arc<Application>>();
        application.store.connection()?.execute("INSERT INTO settings(key,value_json) VALUES ('query_clipboard',?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",[serde_json::to_string(&prefs)?])?;
        if !enabled {
            return Ok(self.status());
        }
        let epoch = self.epoch.load(Ordering::Relaxed);
        self.enabled.store(true, Ordering::Relaxed);
        let live = Arc::clone(&self.enabled);
        let epochs = Arc::clone(&self.epoch);
        let ignore = Arc::clone(&self.ignore_sequence);
        let errors = Arc::clone(&self.error);
        let dispatch = Arc::clone(&self.dispatch);
        let handle = app.clone();
        let (ready_send, ready_receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), AppError> {
                let window = unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE::default(),
                        w!("STATIC"),
                        w!("SubtitleVocabularyList clipboard"),
                        WINDOW_STYLE::default(),
                        0,
                        0,
                        0,
                        0,
                        Some(HWND_MESSAGE),
                        None,
                        None,
                        None,
                    )
                }
                .map_err(|_| AppError::new("capture_unavailable", "无法启动剪贴板监听。"))?;
                struct Guard(HWND);
                impl Drop for Guard {
                    fn drop(&mut self) {
                        unsafe {
                            let _ = RemoveClipboardFormatListener(self.0);
                            let _ = DestroyWindow(self.0);
                        }
                    }
                }
                let _guard = Guard(window);
                unsafe { AddClipboardFormatListener(window) }
                    .map_err(|_| AppError::new("capture_unavailable", "无法接收剪贴板变化。"))?;
                let _ = ready_send.send(Ok::<(), AppError>(()));
                let mut seen = unsafe { GetClipboardSequenceNumber() };
                let mut pending = None;
                let mut previous = String::new();
                while live.load(Ordering::Relaxed) && epochs.load(Ordering::Relaxed) == epoch {
                    let mut message = MSG::default();
                    while unsafe { PeekMessageW(&mut message, Some(window), 0, 0, PM_REMOVE) }
                        .as_bool()
                    {
                        if message.message == WM_CLIPBOARDUPDATE {
                            let sequence = unsafe { GetClipboardSequenceNumber() };
                            if sequence != seen {
                                seen = sequence;
                                pending = Some((sequence, Instant::now()));
                            }
                        }
                        unsafe {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                    if let Some((sequence, changed)) = pending
                        && changed.elapsed() >= Duration::from_millis(prefs.debounce_ms)
                    {
                        pending = None;
                        if sequence != ignore.load(Ordering::Relaxed)
                            && let Ok(text) = read_clipboard()
                        {
                            let text = text.trim();
                            if !text.is_empty() && text.chars().count() <= 4000 && text != previous
                            {
                                previous = text.to_owned();
                                let _dispatch = dispatch.lock().unwrap();
                                if sequence == ignore.load(Ordering::Relaxed)
                                    || sequence != unsafe { GetClipboardSequenceNumber() }
                                {
                                    continue;
                                }
                                let app = handle.state::<Arc<Application>>().inner().clone();
                                if !live.load(Ordering::Relaxed)
                                    || epochs.load(Ordering::Relaxed) != epoch
                                {
                                    break;
                                }
                                let input = QueryInput {
                                    target: text.into(),
                                    context: String::new(),
                                    mode: if crate::ocr::infer_kind(text) == "sentence" {
                                        "translate"
                                    } else {
                                        "explain"
                                    }
                                    .into(),
                                    source_language: "en".into(),
                                    target_language: "zh-CN".into(),
                                    origin: QueryOrigin {
                                        kind: "clipboard".into(),
                                        source_title: "剪贴板复制".into(),
                                        ..Default::default()
                                    },
                                };
                                if app
                                    .store
                                    .is_known_target(&crate::ocr::infer_kind(text), text)
                                    .unwrap_or(false)
                                {
                                    continue;
                                }
                                let session = app.queries.open(input, Some("clipboard"))?;
                                if let Err(error) = app.query_start(
                                    session.session_id.clone(),
                                    session.input_revision,
                                    vec![],
                                    true,
                                    prefs.persist_results,
                                ) {
                                    if error.code != "not_found" {
                                        return Err(error);
                                    }
                                }
                                drop(_dispatch);
                                if live.load(Ordering::Relaxed)
                                    && epochs.load(Ordering::Relaxed) == epoch
                                {
                                    show_query(&handle, false)?;
                                }
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(30));
                }
                Ok(())
            })();
            if let Err(error) = &result {
                let _ = ready_send.send(Err(error.clone()));
            }
            if epochs.load(Ordering::Relaxed) == epoch
                && let Err(error) = result
            {
                *errors.lock().unwrap() = Some(error.message);
                live.store(false, Ordering::Relaxed);
                refresh_clipboard_menu(&handle);
            }
        });
        ready_receive
            .recv_timeout(Duration::from_secs(3))
            .map_err(|_| {
                AppError::new("capture_unavailable", "剪贴板监听尚未准备完成，请重试。")
            })??;
        refresh_clipboard_menu(app);
        Ok(self.status())
    }
    pub fn initialize(&self, app: &Application) -> Result<(), AppError> {
        use rusqlite::OptionalExtension;
        let value: Option<String> = app
            .store
            .connection()?
            .query_row(
                "SELECT value_json FROM settings WHERE key='query_clipboard'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(value) = value {
            *self.preferences.lock().unwrap() = serde_json::from_str(&value)?;
        }
        Ok(())
    }
    pub fn copy(&self, text: &str) -> Result<(), AppError> {
        if text.chars().count() > 16000 {
            return Err(AppError::new("invalid_input", "复制文字过长。"));
        }
        self.mutate(|| write_clipboard(text))
    }
    pub(crate) fn mutate<T>(
        &self,
        operation: impl FnOnce() -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let _guard = self.dispatch.lock().unwrap();
        let result = operation();
        self.ignore_sequence
            .store(unsafe { GetClipboardSequenceNumber() }, Ordering::Relaxed);
        result
    }
}

fn clipboard_error() -> AppError {
    AppError::new("capture_unavailable", "剪贴板暂不可用，请稍后重试。")
}
fn read_clipboard() -> Result<String, AppError> {
    unsafe {
        if IsClipboardFormatAvailable(13).is_err() {
            return Ok(String::new());
        }
        OpenClipboard(None).map_err(|_| clipboard_error())?;
        struct Close;
        impl Drop for Close {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseClipboard();
                }
            }
        }
        let _close = Close;
        let data = GetClipboardData(13).map_err(|_| clipboard_error())?;
        let global = HGLOBAL(data.0);
        let size = GlobalSize(global);
        if size > 40_000 {
            return Err(AppError::new("invalid_input", "复制文字过长。"));
        }
        let pointer = GlobalLock(global);
        if pointer.is_null() {
            return Err(clipboard_error());
        }
        let values = std::slice::from_raw_parts(pointer.cast::<u16>(), size / 2);
        let count = values.iter().position(|v| *v == 0).unwrap_or(values.len());
        let text = String::from_utf16_lossy(&values[..count]);
        let _ = GlobalUnlock(global);
        Ok(text)
    }
}
fn write_clipboard(text: &str) -> Result<(), AppError> {
    unsafe {
        let text = text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let memory = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2).map_err(|_| clipboard_error())?;
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(clipboard_error());
        }
        std::ptr::copy_nonoverlapping(text.as_ptr(), pointer.cast::<u16>(), text.len());
        let _ = GlobalUnlock(memory);
        if OpenClipboard(None).is_err() {
            let _ = GlobalFree(Some(memory));
            return Err(clipboard_error());
        }
        let result = EmptyClipboard().and_then(|_| SetClipboardData(13, Some(HANDLE(memory.0))));
        let _ = CloseClipboard();
        if result.is_err() {
            let _ = GlobalFree(Some(memory));
            return Err(clipboard_error());
        }
        Ok(())
    }
}

pub fn show_query(app: &AppHandle, focus: bool) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("translator") {
        window
            .show()
            .map_err(|_| AppError::new("internal_error", "无法显示查询窗口。"))?;
        let _ = window.unminimize();
        if focus {
            let _ = window.set_focus();
        }
        return Ok(());
    }
    let state = app.state::<QueryWindowState>();
    if focus {
        state.focus.store(true, Ordering::Relaxed);
    }
    if state.creating.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let creating = Arc::clone(&state.creating);
    let wants_focus = Arc::clone(&state.focus);
    let handle = app.clone();
    std::thread::spawn(move || {
        struct Guard(Arc<AtomicBool>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _guard = Guard(creating);
        if let Err(error) = create_query(&handle, focus) {
            let _ = handle.emit_to("main", "capture_failed", error);
            crate::desktop_capture::show_main(&handle);
        } else if wants_focus.swap(false, Ordering::Relaxed)
            && let Some(window) = handle.get_webview_window("translator")
        {
            let _ = window.set_focus();
        }
    });
    Ok(())
}
fn create_query(app: &AppHandle, focus: bool) -> Result<(), AppError> {
    let window = WebviewWindowBuilder::new(
        app,
        "translator",
        WebviewUrl::App("index.html?translate=1".into()),
    )
    .title("查词 · SubtitleVocabularyList")
    .inner_size(420.0, 320.0)
    .min_inner_size(300.0, 220.0)
    .visible(false)
    .focused(focus)
    .build()
    .map_err(|_| AppError::new("internal_error", "无法创建查询窗口。"))?;
    let mut point = windows::Win32::Foundation::POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    if let Some(monitor) = app
        .monitor_from_point(point.x as f64, point.y as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
    {
        let work = monitor.work_area();
        let scale = monitor.scale_factor();
        let width = 420.0f64.min(work.size.width as f64 / scale);
        let height = 320.0f64.min(work.size.height as f64 / scale);
        let _ = window.set_size(tauri::LogicalSize::new(width, height));
        let size = window.outer_size().unwrap_or_default();
        let right = work.position.x + work.size.width as i32 - size.width as i32;
        let bottom = work.position.y + work.size.height as i32 - size.height as i32;
        let _ = window.set_position(tauri::PhysicalPosition::new(
            (point.x + 12).clamp(work.position.x, right.max(work.position.x)),
            (point.y + 12).clamp(work.position.y, bottom.max(work.position.y)),
        ));
    }
    Ok(())
}

pub fn open_input(app: &AppHandle) -> Result<(), AppError> {
    let input = QueryInput {
        target: String::new(),
        context: String::new(),
        mode: "explain".into(),
        source_language: "en".into(),
        target_language: "zh-CN".into(),
        origin: QueryOrigin::default(),
    };
    app.state::<Arc<Application>>().queries.open(input, None)?;
    show_query(app, true)
}
pub fn open_capture(app: &AppHandle, payload: &serde_json::Value) -> Result<(), AppError> {
    let text = payload["text"].as_str().unwrap_or_default();
    let kind = payload["kind"].as_str().unwrap_or("word");
    let app_state = app.state::<Arc<Application>>().inner().clone();
    let origin = QueryOrigin {
        kind: if payload["captureKind"].as_str() == Some("ocr") {
            "ocr"
        } else {
            "selection"
        }
        .into(),
        source_id: payload["sourceId"].as_str().map(str::to_owned),
        source_title: payload["sourceTitle"].as_str().unwrap_or_default().into(),
        location_key: payload["locationKey"].as_str().unwrap_or_default().into(),
    };
    let session = app_state.queries.open(
        QueryInput {
            target: text.into(),
            context: payload["context"].as_str().unwrap_or_default().into(),
            mode: if kind == "sentence" {
                "translate"
            } else {
                "explain"
            }
            .into(),
            source_language: "en".into(),
            target_language: "zh-CN".into(),
            origin,
        },
        None,
    )?;
    if let Err(error) = app_state.query_start(
        session.session_id,
        session.input_revision,
        vec![],
        true,
        true,
    ) {
        if error.code != "not_found" {
            return Err(error);
        }
    }
    show_query(app, true)
}
pub fn hide_query(app: &AppHandle) {
    app.state::<Arc<Application>>().queries.cancel_all();
    let _ = app.emit_to("translator", "translation_hidden", ());
    if let Some(window) = app.get_webview_window("translator") {
        let _ = window.hide();
    }
}
