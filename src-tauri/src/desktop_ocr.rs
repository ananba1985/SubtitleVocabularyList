use crate::{
    application::Application,
    desktop_capture::{self, NativeDesktop},
    error::AppError,
    ocr::{PixelRect, ScreenBounds},
    tasks::TaskSnapshot,
    vocabulary::{SourceInput, digest},
    windows_native,
};
use serde::Serialize;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSession {
    pub id: String,
    pub bounds: ScreenBounds,
    pub image_path: PathBuf,
}
impl Drop for ScreenSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.image_path);
    }
}
struct BusyGuard {
    flag: Arc<AtomicBool>,
    release: bool,
}
impl Drop for BusyGuard {
    fn drop(&mut self) {
        if self.release {
            self.flag.store(false, Ordering::Release);
        }
    }
}
struct TemporaryImage(Option<PathBuf>);
impl Drop for TemporaryImage {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl NativeDesktop {
    pub fn ocr_start(&self, app: &AppHandle) -> Result<TaskSnapshot, AppError> {
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err(AppError::new(
                "capture_busy",
                "已有采集正在进行，请先完成或取消。",
            ));
        }
        let application = app.state::<Arc<Application>>();
        let store = Arc::clone(&application.store);
        let handle = app.clone();
        let flag = Arc::clone(&self.busy);
        let id = Uuid::new_v4().to_string();
        let operation = id.clone();
        let started = application.tasks.start(
            "screen_capture",
            &operation,
            &digest(operation.as_bytes()),
            move |context| {
                let mut busy = BusyGuard {
                    flag,
                    release: true,
                };
                let result: Result<serde_json::Value, AppError> = (|| {
                    context.check_cancelled()?;
                    context.progress("snapshot", 0, 1, "正在准备本次屏幕选区");
                    let image_path = store.root().join("jobs").join(format!("screen-{id}.png"));
                    let mut temporary = TemporaryImage(Some(image_path.clone()));
                    let bounds = windows_native::capture_screen(&image_path)?;
                    context.check_cancelled()?;
                    let session = ScreenSession {
                        id: id.clone(),
                        bounds: bounds.clone(),
                        image_path,
                    };
                    *handle
                        .state::<NativeDesktop>()
                        .screen
                        .lock()
                        .map_err(|_| AppError::new("internal_error", "截图状态不可用。"))? =
                        Some(session);
                    let (send, receive) = std::sync::mpsc::channel();
                    let window_app = handle.clone();
                    let window_id = id.clone();
                    handle
                        .run_on_main_thread(move || {
                            let result = (|| {
                                let window = WebviewWindowBuilder::new(
                                    &window_app,
                                    format!("capture-{window_id}"),
                                    WebviewUrl::App(
                                        format!("index.html?capture={window_id}").into(),
                                    ),
                                )
                                .title("选择截图区域")
                                .decorations(false)
                                .shadow(false)
                                .resizable(false)
                                .always_on_top(true)
                                .skip_taskbar(true)
                                .visible(false)
                                .build()?;
                                window.set_position(PhysicalPosition::new(bounds.x, bounds.y))?;
                                window.set_size(PhysicalSize::new(bounds.width, bounds.height))?;
                                window.show()?;
                                windows_native::fit_capture_window(
                                    window.hwnd()?.0 as isize,
                                    &bounds,
                                )?;
                                window.set_focus()?;
                                Ok::<_, Box<dyn std::error::Error>>(())
                            })();
                            let _ = send.send(result.map_err(|error| error.to_string()));
                        })
                        .map_err(|error| {
                            AppError::new(
                                "provider_unavailable",
                                format!("无法打开截图选区：{error}"),
                            )
                        })?;
                    receive
                        .recv()
                        .map_err(|_| AppError::new("interrupted", "截图窗口准备已中断。"))?
                        .map_err(|error| {
                            AppError::new(
                                "provider_unavailable",
                                format!("无法打开截图选区：{error}"),
                            )
                        })?;
                    context.check_cancelled()?;
                    busy.release = false;
                    // The waiting session now owns the snapshot; it is removed on submit or cancel.
                    temporary.0.take();
                    Ok(json!({"sessionId":id,"stage":"awaiting_selection"}))
                })();
                if let Err(error) = &result {
                    handle.state::<NativeDesktop>().ocr_cancel(&handle, &id);
                    if error.code != "cancelled" {
                        desktop_capture::capture_failed(&handle, error);
                    }
                }
                result
            },
        );
        if started.is_err() {
            self.busy.store(false, Ordering::Release);
        }
        started
    }
    pub fn ocr_session(&self, id: &str) -> Result<serde_json::Value, AppError> {
        let screen = self
            .screen
            .lock()
            .map_err(|_| AppError::new("internal_error", "截图状态不可用。"))?;
        let session = screen
            .as_ref()
            .filter(|session| session.id == id)
            .ok_or_else(|| AppError::new("not_found", "本次截图选区已失效。"))?;
        Ok(serde_json::to_value(session)?)
    }
    pub fn ocr_cancel(&self, app: &AppHandle, id: &str) {
        self.ocr_dismiss(id);
        if let Some(window) = app.get_webview_window(&format!("capture-{id}")) {
            let _ = window.close();
        }
    }
    pub fn ocr_dismiss(&self, id: &str) {
        if let Ok(mut screen) = self.screen.lock()
            && screen.as_ref().is_some_and(|session| session.id == id)
        {
            screen.take();
            self.busy.store(false, Ordering::Release);
        }
    }
    pub fn ocr_submit(
        &self,
        app: &AppHandle,
        id: String,
        rect: PixelRect,
    ) -> Result<TaskSnapshot, AppError> {
        let application = app.state::<Arc<Application>>();
        let store = Arc::clone(&application.store);
        let tools = application.settings()?.tools;
        let session = {
            let mut screen = self
                .screen
                .lock()
                .map_err(|_| AppError::new("internal_error", "截图状态不可用。"))?;
            let session = screen
                .as_ref()
                .filter(|session| session.id == id)
                .ok_or_else(|| AppError::new("not_found", "本次截图已结束，请重新截图。"))?;
            rect.validate(&session.bounds)?;
            // Reject a display change rather than crop against a different desktop geometry.
            if windows_native::screen_bounds()? != session.bounds {
                return Err(AppError::new(
                    "capture_changed",
                    "屏幕配置已变化，请取消并重新截图。",
                ));
            }
            screen
                .take()
                .ok_or_else(|| AppError::new("not_found", "本次截图已结束。"))?
        };
        let handle = app.clone();
        let flag = Arc::clone(&self.busy);
        let hash = digest(&serde_json::to_vec(&rect)?);
        let started=application.tasks.start("ocr",&id,&hash,move|context|{
            let _busy=BusyGuard{flag,release:true};
            let result:Result<serde_json::Value,AppError>=(||{
                context.check_cancelled()?;context.progress("crop",0,1,"正在裁剪本次选区");
                let image=image::open(&session.image_path).map_err(|error|AppError::new("resource_missing",format!("本次截图不可用：{error}")))?;
                let crop_path=store.root().join("jobs").join(format!("ocr-{}.png",Uuid::new_v4()));
                let _cropped=TemporaryImage(Some(crop_path.clone()));
                image.crop_imm(rect.x,rect.y,rect.width,rect.height).save(&crop_path).map_err(|error|AppError::new("io_error",format!("无法保存本次选区：{error}")))?;
                context.check_cancelled()?;context.progress("ocr",0,1,"正在本地识别英语文字");
                let text=crate::ocr::recognize(&tools.tesseract,&crop_path,&context.cancelled)?;context.check_cancelled()?;
                let fingerprint=crate::corpus::file_hash(&crop_path)?;
                let source_id=store.add_source(&SourceInput{kind:"ocr".into(),title:"屏幕 OCR".into(),fingerprint:fingerprint.clone(),path_hint:None,duration_ms:None})?;
                Ok(json!({"text":text,"kind":crate::ocr::infer_kind(&text),"context":text,"sourceId":source_id,"sourceTitle":"屏幕 OCR","locationKey":fingerprint}))
            })();
            match &result{Ok(value)=>{desktop_capture::deliver_capture(&handle,value);},Err(error)=>{if error.code!="cancelled"{desktop_capture::capture_failed(&handle,error);}}}result
        });
        if started.is_err() {
            self.busy.store(false, Ordering::Release);
        }
        if let Some(window) = app.get_webview_window(&format!("capture-{id}")) {
            let _ = window.close();
        }
        if let Err(error) = &started {
            desktop_capture::capture_failed(app, error);
        }
        started
    }
}
