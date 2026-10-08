use crate::{
    application::Application,
    error::AppError,
    tasks::TaskSnapshot,
    vocabulary::{SourceInput, digest},
    windows_native,
};
use serde::Serialize;
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use uuid::Uuid;

#[derive(Default)]
pub struct NativeDesktop {
    binding: Mutex<[Option<Shortcut>; 2]>,
    configuration: Mutex<()>,
    error: Mutex<Option<String>>,
    pub(crate) busy: Arc<AtomicBool>,
    pub(crate) screen: Mutex<Option<crate::desktop_ocr::ScreenSession>>,
}
#[derive(Clone, Copy)]
pub enum CaptureAction {
    Selection,
    Ocr,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeStatus {
    pub selection_registered: bool,
    pub ocr_registered: bool,
    pub error: Option<String>,
}
impl NativeDesktop {
    pub fn status(&self) -> NativeStatus {
        NativeStatus {
            selection_registered: self.binding.lock().is_ok_and(|value| value[0].is_some()),
            ocr_registered: self.binding.lock().is_ok_and(|value| value[1].is_some()),
            error: self.error.lock().ok().and_then(|value| value.clone()),
        }
    }
    pub fn remember_error(&self, error: &AppError) {
        if let Ok(mut value) = self.error.lock() {
            *value = Some(error.message.clone());
        }
    }
    pub fn configure(&self, app: &AppHandle, selection: &str, ocr: &str) -> Result<(), AppError> {
        let parse = |value: &str| -> Result<Option<Shortcut>, AppError> {
            if value.trim().is_empty() {
                Ok(None)
            } else {
                value.parse().map(Some).map_err(|_| {
                    AppError::new("invalid_input", "快捷键格式无效，例如 Ctrl+Alt+Shift+W。")
                })
            }
        };
        let proposed = [parse(selection)?, parse(ocr)?];
        if proposed[0].is_some() && proposed[0] == proposed[1] {
            return Err(AppError::new(
                "invalid_input",
                "划词和截图需要使用不同快捷键。",
            ));
        }
        let _configuration = self
            .configuration
            .lock()
            .map_err(|_| AppError::new("internal_error", "快捷键配置不可用。"))?;
        let previous = *self
            .binding
            .lock()
            .map_err(|_| AppError::new("internal_error", "快捷键状态不可用。"))?;
        let mut added = Vec::new();
        for next in proposed.iter().flatten() {
            if !previous.contains(&Some(*next)) {
                if let Err(error) = app.global_shortcut().register(*next) {
                    for registered in added {
                        let _ = app.global_shortcut().unregister(registered);
                    }
                    return Err(AppError::new(
                        "shortcut_unavailable",
                        format!("快捷键未注册，可能已被其他应用占用：{error}"),
                    ));
                }
                added.push(*next);
            }
        }
        let mut removed = Vec::new();
        for old in previous.iter().flatten() {
            if !proposed.contains(&Some(*old)) {
                if let Err(error) = app.global_shortcut().unregister(*old) {
                    for removed in removed {
                        let _ = app.global_shortcut().register(removed);
                    }
                    for registered in added {
                        let _ = app.global_shortcut().unregister(registered);
                    }
                    return Err(AppError::new(
                        "shortcut_unavailable",
                        format!("旧快捷键无法释放，原配置保留：{error}"),
                    ));
                }
                removed.push(*old);
            }
        }
        *self
            .binding
            .lock()
            .map_err(|_| AppError::new("internal_error", "快捷键状态不可用。"))? = proposed;
        *self
            .error
            .lock()
            .map_err(|_| AppError::new("internal_error", "快捷键状态不可用。"))? = None;
        Ok(())
    }
    pub fn action(&self, shortcut: &Shortcut) -> Option<CaptureAction> {
        let binding = self.binding.lock().ok()?;
        if binding[0].as_ref() == Some(shortcut) {
            Some(CaptureAction::Selection)
        } else if binding[1].as_ref() == Some(shortcut) {
            Some(CaptureAction::Ocr)
        } else {
            None
        }
    }
    pub fn capture(&self, app: &AppHandle) -> Result<TaskSnapshot, AppError> {
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err(AppError::new(
                "capture_busy",
                "本次划词仍在处理，请稍后重试。",
            ));
        }
        let origin = match windows_native::foreground_origin() {
            Ok(value) => value,
            Err(error) => {
                self.busy.store(false, Ordering::Release);
                return Err(error);
            }
        };
        let application = app.state::<Arc<Application>>();
        let store = Arc::clone(&application.store);
        let handle = app.clone();
        let busy = Arc::clone(&self.busy);
        let operation = Uuid::new_v4().to_string();
        let hash = digest(&serde_json::to_vec(&origin)?);
        let started=application.tasks.start("selection",&operation,&hash,move|context|{
            struct BusyGuard(Arc<AtomicBool>);impl Drop for BusyGuard{fn drop(&mut self){self.0.store(false,Ordering::Release);}}
            let _guard=BusyGuard(busy);
            context.progress("selection",0,1,"正在读取本次文字选区");
            let result:Result<serde_json::Value,AppError>=(||{
                context.check_cancelled()?;let selected=windows_native::selected_text(&origin)?;context.check_cancelled()?;
                let fingerprint=digest(&serde_json::to_vec(&(&origin.application,&origin.title,&selected.context))?);
                let title=if origin.title.is_empty(){origin.application.clone()}else{format!("{} · {}",origin.application,origin.title)};
                let source_id=store.add_source(&SourceInput{kind:"selection".into(),title:title.clone(),fingerprint,path_hint:None,duration_ms:None})?;
                let kind=crate::ocr::infer_kind(&selected.text);
                Ok(json!({"text":selected.text,"kind":kind,"context":selected.context,"sourceId":source_id,"sourceTitle":title,"locationKey":digest(selected.text.as_bytes())}))
            })();
            match &result {
                Ok(value)=>{deliver_capture(&handle,value);},
                Err(error)=>{if error.code!="cancelled"{capture_failed(&handle,error);}},
            }
            result
        });
        if started.is_err() {
            self.busy.store(false, Ordering::Release);
        }
        started
    }
}
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
pub fn deliver_capture(app: &AppHandle, value: &serde_json::Value) {
    let known = app
        .state::<Arc<Application>>()
        .store
        .is_known_target(
            value["kind"].as_str().unwrap_or(""),
            value["text"].as_str().unwrap_or(""),
        )
        .unwrap_or(false);
    let mut payload = value.clone();
    payload["alreadyKnown"] = serde_json::Value::Bool(known);
    let _ = app.emit("capture_completed", payload);
    if !known {
        show_main(app);
    }
}
pub fn capture_failed(app: &AppHandle, error: &AppError) {
    let _ = app.emit("capture_failed", error);
    show_main(app);
}
pub fn quit(app: &AppHandle) {
    let session_id = app
        .state::<NativeDesktop>()
        .screen
        .lock()
        .ok()
        .and_then(|screen| screen.as_ref().map(|session| session.id.clone()));
    if let Some(id) = session_id {
        app.state::<NativeDesktop>().ocr_cancel(app, &id);
    }
    let application = app.state::<Arc<Application>>();
    application.stop_preparation_scheduler();
    let tasks = Arc::clone(&application.tasks);
    if let Ok(snapshots) = tasks.active() {
        for task in snapshots {
            if !task.terminal() {
                let _ = tasks.cancel(&task.id);
            }
        }
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(3) {
            if tasks.active().is_ok_and(|tasks| tasks.is_empty()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        handle.exit(0);
    });
}
