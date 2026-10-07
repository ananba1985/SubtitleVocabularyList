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
    binding: Mutex<Option<Shortcut>>,
    error: Mutex<Option<String>>,
    busy: Arc<AtomicBool>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeStatus {
    pub selection_registered: bool,
    pub error: Option<String>,
}
impl NativeDesktop {
    pub fn status(&self) -> NativeStatus {
        NativeStatus {
            selection_registered: self.binding.lock().is_ok_and(|value| value.is_some()),
            error: self.error.lock().ok().and_then(|value| value.clone()),
        }
    }
    pub fn remember_error(&self, error: &AppError) {
        if let Ok(mut value) = self.error.lock() {
            *value = Some(error.message.clone());
        }
    }
    pub fn configure(&self, app: &AppHandle, shortcut: &str) -> Result<(), AppError> {
        let proposed = if shortcut.trim().is_empty() {
            None
        } else {
            Some(shortcut.parse::<Shortcut>().map_err(|_| {
                AppError::new("invalid_input", "划词快捷键格式无效，例如 Ctrl+Alt+W。")
            })?)
        };
        let mut binding = self
            .binding
            .lock()
            .map_err(|_| AppError::new("internal_error", "快捷键状态不可用。"))?;
        if *binding == proposed {
            if let Ok(mut error) = self.error.lock() {
                *error = None;
            }
            return Ok(());
        }
        if let Some(next) = proposed {
            app.global_shortcut().register(next).map_err(|error| {
                AppError::new(
                    "shortcut_unavailable",
                    format!("快捷键未注册，可能已被其他应用占用：{error}"),
                )
            })?;
        }
        if let Some(previous) = *binding
            && let Err(error) = app.global_shortcut().unregister(previous)
        {
            if let Some(next) = proposed {
                let _ = app.global_shortcut().unregister(next);
            }
            return Err(AppError::new(
                "shortcut_unavailable",
                format!("旧快捷键无法释放，配置保留：{error}"),
            ));
        }
        *binding = proposed;
        *self
            .error
            .lock()
            .map_err(|_| AppError::new("internal_error", "快捷键状态不可用。"))? = None;
        Ok(())
    }
    pub fn matches(&self, shortcut: &Shortcut) -> bool {
        self.binding
            .lock()
            .is_ok_and(|value| value.as_ref() == Some(shortcut))
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
                let kind=if selected.text.ends_with(['.','!','?'])||selected.text.contains('\n'){"sentence"}else if selected.text.split_whitespace().count()>1{"phrase"}else{"word"};
                Ok(json!({"text":selected.text,"kind":kind,"context":selected.context,"sourceId":source_id,"sourceTitle":title,"locationKey":digest(selected.text.as_bytes())}))
            })();
            match &result {
                Ok(value)=>{let _=handle.emit("capture_completed",value);show_main(&handle);},
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
pub fn capture_failed(app: &AppHandle, error: &AppError) {
    let _ = app.emit("capture_failed", error);
    show_main(app);
}
pub fn quit(app: &AppHandle) {
    let application = app.state::<Arc<Application>>();
    let tasks = Arc::clone(&application.tasks);
    if let Ok(snapshots) = tasks.list() {
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
            if tasks
                .list()
                .is_ok_and(|tasks| tasks.iter().all(|task| task.terminal()))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        handle.exit(0);
    });
}
