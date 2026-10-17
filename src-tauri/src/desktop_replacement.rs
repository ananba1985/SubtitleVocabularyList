use crate::{
    application::Application,
    desktop_query::{ClipboardWatcher, open_input, show_query},
    error::AppError,
    queries::{QueryInput, QueryOrigin},
    replacement_target::{self, Apartment, ClipboardLease, EditingTarget},
    windows_native,
};
use serde::Serialize;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicIsize, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Default, Serialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ReplacementStatus {
    pub operation_id: String,
    pub state: String,
    pub original_text: String,
    pub translation: String,
    pub provider_id: String,
    pub error: Option<String>,
    pub clipboard_pending: bool,
}
#[derive(Default)]
pub struct NativeReplacement {
    status: Mutex<ReplacementStatus>,
    cancel: Mutex<Arc<AtomicBool>>,
    pub busy: AtomicBool,
    pending_clipboard: Mutex<Option<ClipboardLease>>,
    notice: AtomicIsize,
}
pub struct ReplacementMenu(pub tauri::menu::MenuItem<tauri::Wry>);
impl NativeReplacement {
    pub fn status(&self) -> ReplacementStatus {
        self.status.lock().unwrap().clone()
    }
    fn update(&self, app: &AppHandle, state: &str, error: Option<String>) {
        let status = {
            let mut status = self.status.lock().unwrap();
            status.state = state.into();
            status.error = error;
            status.clone()
        };
        let label = match state {
            "capturing" => "读取选句",
            "translating" => "正在翻译",
            "validating" => "核对选区",
            "applying" => "正在替换",
            "replaced" => "已替换",
            "result_only" => "译文可查看",
            "unverified" => "请核对原输入",
            "cancelled" => "已取消",
            "failed" => "失败，可查看原因",
            _ => "尚无操作",
        };
        if let Some(tray) = app.tray_by_id("main-tray") {
            let _ = tray.set_tooltip(Some(format!("SubtitleVocabularyList · 翻译替换：{label}")));
        }
        if let Some(menu) = app.try_state::<ReplacementMenu>() {
            let _ = menu.0.set_text(format!("翻译替换：{label}（查看）"));
        }
        self.show_notice(app, &status, label);
        let _ = app.emit_to(
            "main",
            "replacement_updated",
            serde_json::json!({"operationId":status.operation_id,"state":state}),
        );
    }
    fn show_notice(&self, app: &AppHandle, status: &ReplacementStatus, label: &str) {
        use windows::{
            Win32::{
                Foundation::{HWND, RECT},
                Graphics::Gdi::{DEFAULT_GUI_FONT, GetStockObject},
                UI::WindowsAndMessaging::*,
            },
            core::{HSTRING, w},
        };
        let text = format!(
            "翻译替换：{label}\n{}",
            if status.error.is_some() {
                "右键托盘查看原因和译文"
            } else if status.state == "replaced" {
                "可在原输入框按 Ctrl+Z 撤销"
            } else {
                "保持选区；再按快捷键可取消"
            }
        );
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || unsafe {
            let native = handle.state::<NativeReplacement>();
            let mut window = HWND(native.notice.load(Ordering::Acquire) as *mut _);
            if window.0.is_null() {
                if let Ok(created) = CreateWindowExW(
                    WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                    w!("STATIC"),
                    w!(""),
                    WS_POPUP | WS_BORDER,
                    0,
                    0,
                    380,
                    64,
                    None,
                    None,
                    None,
                    None,
                ) {
                    window = created;
                    native.notice.store(window.0 as isize, Ordering::Release);
                    let font = GetStockObject(DEFAULT_GUI_FONT);
                    SendMessageW(
                        window,
                        WM_SETFONT,
                        Some(windows::Win32::Foundation::WPARAM(font.0 as usize)),
                        Some(windows::Win32::Foundation::LPARAM(1)),
                    );
                } else {
                    return;
                }
            }
            let mut work = RECT::default();
            if SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some((&mut work as *mut RECT).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .is_ok()
            {
                let _ = SetWindowTextW(window, &HSTRING::from(text));
                let _ = SetWindowPos(
                    window,
                    Some(HWND_TOPMOST),
                    work.right - 400,
                    work.bottom - 84,
                    380,
                    64,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        });
        if matches!(
            status.state.as_str(),
            "replaced" | "result_only" | "unverified" | "cancelled" | "failed"
        ) {
            let operation = status.operation_id.clone();
            let state = status.state.clone();
            let handle = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(4));
                let scheduled = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    let native = scheduled.state::<NativeReplacement>();
                    let current = native.status();
                    if current.operation_id == operation && current.state == state {
                        let window = native.notice.swap(0, Ordering::AcqRel);
                        if window != 0 {
                            unsafe {
                                let _ = DestroyWindow(HWND(window as *mut _));
                            }
                        }
                    }
                });
            });
        }
    }
    pub fn cancel(&self) {
        self.cancel.lock().unwrap().store(true, Ordering::Release);
    }
    pub fn toggle(&self, app: &AppHandle) {
        if self.busy.swap(true, Ordering::AcqRel) {
            self.cancel();
            return;
        }
        if self
            .pending_clipboard
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(ClipboardLease::still_owned)
        {
            self.update(
                app,
                "unverified",
                Some("请先核对上次原输入并在本地设置恢复剪贴板，再开始新的替换。".into()),
            );
            self.busy.store(false, Ordering::Release);
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *self.cancel.lock().unwrap() = cancel.clone();
        *self.status.lock().unwrap() = ReplacementStatus {
            operation_id: uuid::Uuid::new_v4().to_string(),
            state: "capturing".into(),
            ..Default::default()
        };
        self.update(app, "capturing", None);
        // The foreground identity is captured synchronously with the hotkey.
        let origin = windows_native::foreground_origin();
        let handle = app.clone();
        std::thread::spawn(move || {
            let native = handle.state::<NativeReplacement>();
            let result = origin.and_then(|origin| native.run(&handle, origin, &cancel));
            if let Err(error) = result {
                native.update(
                    &handle,
                    if error.code == "cancelled" {
                        "cancelled"
                    } else {
                        "failed"
                    },
                    Some(error.message),
                );
            }
            native.busy.store(false, Ordering::Release);
        });
    }
    fn run(
        &self,
        handle: &AppHandle,
        origin: windows_native::CaptureOrigin,
        cancel: &AtomicBool,
    ) -> Result<(), AppError> {
        let started = Instant::now();
        if self
            .pending_clipboard
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(ClipboardLease::still_owned)
        {
            return Err(AppError::new(
                "clipboard_pending",
                "上次粘贴未能核实，请先核对原输入并在本地设置恢复剪贴板。",
            ));
        }
        self.pending_clipboard.lock().unwrap().take();
        let _apartment = Apartment::new()?;
        let target = EditingTarget::capture(origin)?;
        let app = handle.state::<Arc<Application>>();
        let settings = app.settings()?;
        let provider = app
            .store
            .providers()?
            .into_iter()
            .find(|p| {
                p.id == settings.replacement_provider_id && p.enabled && p.supports_translation()
            })
            .ok_or_else(|| {
                AppError::new(
                    "provider_unavailable",
                    "所选翻译替换服务未启用或不支持翻译，请在本地设置修改。",
                )
            })?;
        {
            let mut status = self.status.lock().unwrap();
            status.original_text = target.original.clone();
            status.provider_id = provider.id.clone();
        }
        self.update(handle, "translating", None);
        let input = QueryInput {
            target: target.original.trim().into(),
            context: String::new(),
            mode: "translate".into(),
            source_language: "en".into(),
            target_language: "zh-CN".into(),
            origin: QueryOrigin::default(),
        };
        let session = app.queries.open_replacement(input)?;
        app.query_start(
            session.session_id.clone(),
            session.input_revision,
            vec![provider.id],
            false,
            true,
        )?;
        struct QueryGuard<'a>(&'a Application, String);
        impl Drop for QueryGuard<'_> {
            fn drop(&mut self) {
                let _ = self.0.queries.cancel(&self.1, None);
            }
        }
        let _query_guard = QueryGuard(&app, session.session_id.clone());
        let mut eligible = true;
        let mut lost_reason = None;
        let translation = loop {
            if cancel.load(Ordering::Acquire) {
                return Err(AppError::new("cancelled", "本次翻译替换已取消。"));
            }
            if started.elapsed() >= Duration::from_secs(60) {
                return Err(AppError::new(
                    "timeout",
                    "翻译替换等待已超过 60 秒，本次未回填，请稍后重新选择。",
                ));
            }
            if eligible && let Err(error) = target.validate() {
                eligible = false;
                lost_reason = Some(error.message);
            }
            let current = app
                .queries
                .get(Some(&session.session_id))?
                .ok_or_else(|| AppError::new("cancelled", "本次查询已结束。"))?;
            if let Some(result) = current.results.first() {
                match result.state.as_str() {
                    "ready" => {
                        break result
                            .value
                            .as_ref()
                            .map(|v| v.translation.trim().to_owned())
                            .unwrap_or_default();
                    }
                    "failed" | "cancelled" => {
                        return Err(AppError::new(
                            "provider_unavailable",
                            result
                                .error
                                .clone()
                                .unwrap_or_else(|| "翻译服务已取消本次请求。".into()),
                        ));
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        if translation.is_empty()
            || !translation
                .chars()
                .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
            || translation.contains('\0')
            || translation.chars().count() > 16000
        {
            return Err(AppError::new(
                "invalid_data",
                "未获得完整有效的中文译文，本次没有替换原文。",
            ));
        }
        let leading =
            &target.original[..target.original.len() - target.original.trim_start().len()];
        let trailing = &target.original[target.original.trim_end().len()..];
        let translation = format!("{leading}{translation}{trailing}");
        self.status.lock().unwrap().translation = translation.clone();
        self.update(handle, "validating", None);
        // Held modifiers must be released by the user; we never release their keys.
        while !replacement_target::modifiers_released() {
            if cancel.load(Ordering::Acquire) {
                return Err(AppError::new("cancelled", "本次翻译替换已取消。"));
            }
            if started.elapsed() >= Duration::from_secs(60) {
                eligible = false;
                lost_reason = Some("快捷键尚未释放，本次只保留译文。".into());
                break;
            }
            if let Err(error) = target.validate() {
                eligible = false;
                lost_reason = Some(error.message);
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        if let Err(error) = target.validate() {
            eligible = false;
            lost_reason = Some(error.message);
        }
        if !eligible {
            self.update(handle, "result_only", lost_reason);
            return Ok(());
        }
        let watcher = handle.state::<ClipboardWatcher>();
        let owner = handle
            .get_webview_window("main")
            .ok_or_else(|| AppError::new("not_found", "主窗口不可用。"))?
            .hwnd()
            .map_err(|e| AppError::new("internal_error", e.to_string()))?
            .0 as isize;
        let mut lease = match watcher.mutate(|| ClipboardLease::install(&translation, owner)) {
            Ok(lease) => lease,
            Err(error) => {
                self.update(handle, "result_only", Some(error.message));
                return Ok(());
            }
        };
        let target_error = target.validate().err();
        let abort_reason = if cancel.load(Ordering::Acquire) {
            Some("本次翻译替换已取消。".into())
        } else if let Some(error) = target_error {
            Some(error.message)
        } else if !replacement_target::modifiers_released() {
            Some("粘贴前检测到按键仍被按住，本次只保留译文。".into())
        } else if !lease.still_owned() {
            Some("粘贴前剪贴板已变化，本次只保留译文。".into())
        } else {
            None
        };
        if let Some(reason) = abort_reason {
            watcher.mutate(|| lease.restore())?;
            self.update(
                handle,
                if cancel.load(Ordering::Acquire) {
                    "cancelled"
                } else {
                    "result_only"
                },
                Some(reason),
            );
            return Ok(());
        }
        self.update(handle, "applying", None);
        target.disarm();
        let sent = replacement_target::paste_once();
        let confirmed = sent.is_ok()
            && replacement_target::confirm_paste(&target, &target.expected(&translation));
        if confirmed {
            match watcher.mutate(|| lease.restore()) {
                Ok(()) => self.update(handle, "replaced", None),
                Err(error) => {
                    self.status.lock().unwrap().clipboard_pending = true;
                    *self.pending_clipboard.lock().unwrap() = Some(lease);
                    self.update(
                        handle,
                        "replaced",
                        Some(format!("原文已替换；剪贴板尚未恢复：{}", error.message)),
                    );
                }
            }
        } else {
            self.status.lock().unwrap().clipboard_pending = true;
            *self.pending_clipboard.lock().unwrap() = Some(lease);
            self.update(handle, "unverified", Some(sent.err().map(|e| e.message).unwrap_or_else(|| "粘贴已发送，但无法核实结果。请核对原输入；译文留在剪贴板，核对后可在本地设置恢复原剪贴板。".into())));
        }
        Ok(())
    }
    pub fn restore_clipboard(&self, app: &AppHandle) -> Result<(), AppError> {
        let mut pending = self.pending_clipboard.lock().unwrap();
        if let Some(lease) = pending.as_mut() {
            app.state::<ClipboardWatcher>().mutate(|| lease.restore())?;
        }
        pending.take();
        self.status.lock().unwrap().clipboard_pending = false;
        Ok(())
    }
    pub fn open_result(&self, app: &AppHandle) -> Result<(), AppError> {
        let status = self.status();
        if status.operation_id.is_empty() {
            return open_input(app);
        }
        let application = app.state::<Arc<Application>>();
        let session = application.queries.open(
            QueryInput {
                target: status.original_text.trim().into(),
                context: String::new(),
                mode: "translate".into(),
                source_language: "en".into(),
                target_language: "zh-CN".into(),
                origin: QueryOrigin {
                    kind: "input".into(),
                    source_title: "翻译替换结果（手动查看）".into(),
                    ..Default::default()
                },
            },
            None,
        )?;
        if !status.translation.is_empty() {
            application.queries.present_translation(
                &session.session_id,
                &status.provider_id,
                &status.translation,
            )?;
        } else {
            application.queries.present_error(
                &session.session_id,
                status
                    .error
                    .as_deref()
                    .unwrap_or("尚未获得译文，可取消后重新选择。"),
            )?;
        }
        show_query(app, true)
    }
}
