mod mpv;

use crate::{
    application::Application,
    error::AppError,
    watch::{DisplayClock, WatchContext, WatchMedia, WatchProgress},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicIsize, Ordering},
        mpsc::{self, Sender},
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, RGN_DIFF, SetWindowRgn},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                GetCapture, GetKeyboardState, ReleaseCapture, SetCapture, SetFocus, ToUnicode,
            },
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WatchTrack {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub session_id: String,
    pub revision: u64,
    pub title: String,
    pub path: String,
    pub source_id: Option<String>,
    pub playing: bool,
    pub loaded: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
    pub subtitle_delay: f64,
    pub tracks: Vec<WatchTrack>,
    pub context: Option<WatchContext>,
    pub status: String,
    pub remaining_seconds: Option<f64>,
    pub collected_entry_id: Option<String>,
    pub error: Option<String>,
    pub renderer: String,
    pub translation_visible_at: Option<i64>,
    pub collection_started_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WatchControl {
    pub action: String,
    pub value: Option<f64>,
    pub text: Option<String>,
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WatchViewport {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub visible: bool,
}

type Reply = Sender<Result<PlayerSnapshot, AppError>>;
enum Message {
    Key(Option<String>),
    Mouse(f64, f64, Option<bool>),
    Open(WatchMedia, Reply),
    Control(WatchControl, Option<Reply>),
    Viewport(i32, i32, bool),
    Saved(
        String,
        u64,
        Result<crate::vocabulary::CollectionResult, AppError>,
    ),
    Shutdown(Sender<()>),
}
static KEYBOARD: OnceLock<Mutex<Option<Sender<Message>>>> = OnceLock::new();

#[derive(Default)]
pub struct NativePlayer {
    host: AtomicIsize,
    sender: Mutex<Option<Sender<Message>>>,
    snapshot: Arc<Mutex<PlayerSnapshot>>,
}

unsafe extern "system" fn host_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if matches!(message, WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP) {
            if message == WM_LBUTTONDOWN {
                let _ = SetFocus(Some(hwnd));
                SetCapture(hwnd);
            } else if message == WM_LBUTTONUP && GetCapture() == hwnd {
                let _ = ReleaseCapture();
            }
            if let Some(lock) = KEYBOARD.get()
                && let Ok(guard) = lock.lock()
                && let Some(sender) = guard.as_ref()
            {
                // Normalize in the host's own coordinate space. The actor renders
                // in physical pixels, which can differ from Windows input at high DPI.
                let mut client = Default::default();
                if GetClientRect(hwnd, &mut client).is_err() {
                    return LRESULT(0);
                }
                let x = lparam.0 as i16 as f64 / (client.right - client.left).max(1) as f64;
                let y = (lparam.0 >> 16) as i16 as f64 / (client.bottom - client.top).max(1) as f64;
                let _ = sender.send(Message::Mouse(
                    x,
                    y,
                    match message {
                        WM_LBUTTONDOWN => Some(true),
                        WM_LBUTTONUP => Some(false),
                        _ => None,
                    },
                ));
            }
            return LRESULT(0);
        }
        if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP) {
            let up = matches!(message, WM_KEYUP | WM_SYSKEYUP);
            if (up || lparam.0 & (1 << 30) == 0)
                && let Some(lock) = KEYBOARD.get()
                && let Ok(guard) = lock.lock()
                && let Some(sender) = guard.as_ref()
            {
                if up {
                    let _ = sender.send(Message::Key(None));
                } else if let Some(key) = native_key(wparam.0, lparam) {
                    let _ = sender.send(Message::Key(Some(key)));
                }
            }
            return LRESULT(0);
        }
        if message == WM_ERASEBKGND {
            return LRESULT(1);
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

// A thin Windows-to-libmpv input adapter; mpv owns key bindings and repeat behavior.
unsafe fn native_key(vk: usize, scan: LPARAM) -> Option<String> {
    unsafe {
        let mut keys = [0u8; 256];
        GetKeyboardState(&mut keys).ok()?;
        let (ctrl, alt, shift) = (
            keys[0x11] & 0x80 != 0,
            keys[0x12] & 0x80 != 0,
            keys[0x10] & 0x80 != 0,
        );
        let special = match vk {
            0x20 => Some("SPACE"),
            0x1b => Some("ESC"),
            0x25 => Some("LEFT"),
            0x27 => Some("RIGHT"),
            0x26 => Some("UP"),
            0x28 => Some("DOWN"),
            0x21 => Some("PGUP"),
            0x22 => Some("PGDWN"),
            0x24 => Some("HOME"),
            0x23 => Some("END"),
            0x08 => Some("BS"),
            0x2e => Some("DEL"),
            0x0d => Some("ENTER"),
            0x09 => Some("TAB"),
            _ => None,
        };
        let key = if let Some(key) = special {
            key.to_owned()
        } else if (0x70..=0x87).contains(&vk) {
            format!("F{}", vk - 0x6f)
        } else {
            keys[0x11] = 0;
            keys[0x12] = 0;
            let mut text = [0u16; 8];
            let length = ToUnicode(
                vk as u32,
                ((scan.0 >> 16) & 0xff) as u32,
                Some(&keys),
                &mut text,
                4,
            );
            if length <= 0 {
                return None;
            }
            String::from_utf16_lossy(&text[..length as usize])
        };
        Some(format!(
            "{}{}{}{}",
            if ctrl { "Ctrl+" } else { "" },
            if alt { "Alt+" } else { "" },
            if shift && (special.is_some() || (0x70..=0x87).contains(&vk)) {
                "Shift+"
            } else {
                ""
            },
            key
        ))
    }
}

impl NativePlayer {
    pub fn raise(&self, handle: &tauri::AppHandle) {
        let host = self.host.load(Ordering::Acquire);
        if host != 0 {
            let _ = handle.run_on_main_thread(move || unsafe {
                let _ = SetWindowPos(
                    HWND(host as *mut _),
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                );
            });
        }
    }
    fn ensure_worker(&self, handle: &tauri::AppHandle) -> Result<Sender<Message>, AppError> {
        let mut sender = self.sender.lock().map_err(|_| failure("播放会话不可用"))?;
        if let Some(sender) = sender.as_ref() {
            return Ok(sender.clone());
        }
        let window = if self.host.load(Ordering::Acquire) == 0 {
            let (send, receive) = mpsc::channel();
            let app = handle.clone();
            handle
                .run_on_main_thread(move || {
                    let result: Result<isize, AppError> = (|| unsafe {
                        let main = app
                            .get_webview_window("main")
                            .ok_or_else(|| failure("主窗口不可用"))?;
                        let parent = main.hwnd().map_err(|e| failure(e.to_string()))?;
                        let instance =
                            GetModuleHandleW(None).map_err(|e| failure(e.to_string()))?;
                        let class = WNDCLASSW {
                            style: CS_OWNDC,
                            lpfnWndProc: Some(host_proc),
                            hInstance: instance.into(),
                            lpszClassName: w!("SVL_EmbeddedVideo"),
                            ..Default::default()
                        };
                        let _ = RegisterClassW(&class);
                        let host = CreateWindowExW(
                            WINDOW_EX_STYLE::default(),
                            w!("SVL_EmbeddedVideo"),
                            w!("边看边学视频"),
                            WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN | WS_TABSTOP,
                            0,
                            0,
                            1,
                            1,
                            Some(parent),
                            None,
                            Some(instance.into()),
                            None,
                        )
                        .map_err(|e| failure(e.to_string()))?;
                        Ok(host.0 as isize)
                    })();
                    let _ = send.send(result);
                })
                .map_err(|e| failure(e.to_string()))?;
            let host = receive
                .recv_timeout(Duration::from_secs(10))
                .map_err(|_| failure("等待视频区域超时"))??;
            self.host.store(host, Ordering::Release);
            host
        } else {
            self.host.load(Ordering::Acquire)
        };
        let (tx, rx) = mpsc::channel();
        let tx_worker = tx.clone();
        let (ready, waiting) = mpsc::channel();
        let app = handle.clone();
        let application = handle.state::<Arc<Application>>().inner().clone();
        let state = self.snapshot.clone();
        std::thread::Builder::new()
            .name("svl-libmpv".into())
            .spawn(move || {
                #[cfg(debug_assertions)]
                let dll = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../.tools/runtime/mpv/libmpv-2.dll");
                #[cfg(not(debug_assertions))]
                let dll = app
                    .path()
                    .resource_dir()
                    .unwrap_or_default()
                    .join("tools/mpv/libmpv-2.dll");
                let engine = mpv::Engine::new(
                    HWND(window as *mut _),
                    &dll,
                    &application.store.root().join("cache/mpv"),
                );
                match engine {
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                    Ok(engine) => {
                        let _ = ready.send(Ok(()));
                        let mut actor =
                            Actor::new(engine, application, app, state, tx_worker, window);
                        while !actor.stopped {
                            match rx.recv_timeout(Duration::from_millis(8)) {
                                Ok(message) => actor.receive(message),
                                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                Err(_) => {}
                            }
                            actor.tick();
                        }
                        actor.save_progress();
                        let stopped = actor.shutdown_reply.take();
                        drop(actor);
                        if let Some(reply) = stopped {
                            let _ = reply.send(());
                        }
                    }
                }
            })
            .map_err(|e| failure(e.to_string()))?;
        waiting
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| failure("等待播放器初始化超时"))??;
        *KEYBOARD
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| failure("键盘入口不可用"))? = Some(tx.clone());
        *sender = Some(tx.clone());
        Ok(tx)
    }
    pub fn open(
        &self,
        handle: &tauri::AppHandle,
        media: WatchMedia,
    ) -> Result<PlayerSnapshot, AppError> {
        let sender = self.ensure_worker(handle)?;
        let (send, receive) = mpsc::channel();
        sender
            .send(Message::Open(media, send))
            .map_err(|_| failure("播放会话已结束"))?;
        receive
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| failure("等待打开视频超时"))?
    }
    pub fn state(&self) -> Result<PlayerSnapshot, AppError> {
        self.snapshot
            .lock()
            .map(|s| s.clone())
            .map_err(|_| failure("播放状态不可用"))
    }
    pub fn control(&self, control: WatchControl) -> Result<PlayerSnapshot, AppError> {
        let sender = self
            .sender
            .lock()
            .map_err(|_| failure("播放会话不可用"))?
            .clone();
        let Some(sender) = sender else {
            return self.state();
        };
        let (send, receive) = mpsc::channel();
        sender
            .send(Message::Control(control, Some(send)))
            .map_err(|_| failure("播放会话已结束"))?;
        receive
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| failure("等待播放操作超时"))?
    }
    pub fn viewport(
        &self,
        handle: &tauri::AppHandle,
        viewport: WatchViewport,
    ) -> Result<(), AppError> {
        let host = self.host.load(Ordering::Acquire);
        if host == 0 {
            return Ok(());
        }
        let scale = handle
            .get_webview_window("main")
            .and_then(|w| w.scale_factor().ok())
            .unwrap_or(1.0);
        let x = (viewport.x * scale).round() as i32;
        let y = (viewport.y * scale).round() as i32;
        let width = (viewport.width * scale).round().max(1.0) as i32;
        let height = (viewport.height * scale).round().max(1.0) as i32;
        if let Some(sender) = self
            .sender
            .lock()
            .map_err(|_| failure("播放会话不可用"))?
            .as_ref()
        {
            let _ = sender.send(Message::Viewport(width, height, viewport.visible));
        }
        handle
            .run_on_main_thread(move || unsafe {
                let hwnd = HWND(host as *mut _);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE
                        | if viewport.visible {
                            SWP_SHOWWINDOW
                        } else {
                            SWP_HIDEWINDOW
                        },
                );
                // WebView2's compositor can cover a native OpenGL sibling despite
                // HWND z-order. Cut its video rectangle out of the WebView surface;
                // restore the complete WebView for dialogs, hidden video and other pages.
                if let Ok(view) = FindWindowExW(GetParent(hwnd).ok(), None, w!("WRY_WEBVIEW"), None)
                {
                    if viewport.visible {
                        let mut client = Default::default();
                        if GetClientRect(view, &mut client).is_ok() {
                            let region = CreateRectRgn(0, 0, client.right, client.bottom);
                            let video = CreateRectRgn(x, y, x + width, y + height);
                            CombineRgn(Some(region), Some(region), Some(video), RGN_DIFF);
                            let _ = DeleteObject(video.into());
                            if SetWindowRgn(view, Some(region), true) == 0 {
                                let _ = DeleteObject(region.into());
                            }
                        }
                    } else {
                        SetWindowRgn(view, None, true);
                    }
                }
            })
            .map_err(|e| failure(e.to_string()))
    }
    pub fn hide(&self) {
        if let Ok(sender) = self.sender.lock()
            && let Some(sender) = sender.as_ref()
        {
            let _ = sender.send(Message::Viewport(0, 0, false));
        }
    }
    pub fn shutdown(&self) {
        if let Ok(mut sender) = self.sender.lock()
            && let Some(sender) = sender.take()
        {
            let (done, waiting) = mpsc::channel();
            let _ = sender.send(Message::Shutdown(done));
            let _ = waiting.recv_timeout(Duration::from_secs(3));
        }
        if let Some(keyboard) = KEYBOARD.get()
            && let Ok(mut keyboard) = keyboard.lock()
        {
            *keyboard = None;
        }
    }
}

struct Actor {
    engine: mpv::Engine,
    application: Arc<Application>,
    handle: tauri::AppHandle,
    published: Arc<Mutex<PlayerSnapshot>>,
    sender: Sender<Message>,
    state: PlayerSnapshot,
    media: Option<WatchMedia>,
    clock: DisplayClock,
    viewport: (i32, i32, bool),
    window: isize,
    pending_context: Option<Instant>,
    pending_display: bool,
    force_draw: bool,
    overlay: String,
    user_pause: bool,
    stopped: bool,
    poll_at: Instant,
    last_publish: String,
    restore: Option<WatchProgress>,
    bound_sid: Option<i64>,
    enabled: bool,
    delay: u32,
    translation_retry_at: Instant,
    shutdown_reply: Option<Sender<()>>,
    fullscreen: bool,
    last_notice: String,
}
fn failure(detail: impl Into<String>) -> AppError {
    AppError::new("player_unavailable", detail)
}
impl Actor {
    fn new(
        engine: mpv::Engine,
        application: Arc<Application>,
        handle: tauri::AppHandle,
        published: Arc<Mutex<PlayerSnapshot>>,
        sender: Sender<Message>,
        window: isize,
    ) -> Self {
        let settings = application.settings().unwrap_or_default();
        Self {
            engine,
            application,
            handle,
            published,
            sender,
            state: PlayerSnapshot {
                renderer: "libmpv/OpenGL".into(),
                status: "idle".into(),
                volume: 100.0,
                ..Default::default()
            },
            media: None,
            clock: DisplayClock::default(),
            viewport: (1, 1, false),
            window,
            pending_context: None,
            pending_display: false,
            force_draw: false,
            overlay: String::new(),
            user_pause: false,
            stopped: false,
            poll_at: Instant::now(),
            last_publish: String::new(),
            restore: None,
            bound_sid: None,
            enabled: settings.watch_auto_collect,
            delay: settings.watch_collect_delay_seconds,
            translation_retry_at: Instant::now(),
            shutdown_reply: None,
            fullscreen: false,
            last_notice: String::new(),
        }
    }
    fn receive(&mut self, message: Message) {
        match message {
            Message::Key(key) => {
                if let Some(key) = key {
                    if matches!(key.as_str(), "q" | "Q") {
                        let _ = self.control(WatchControl {
                            action: "close".into(),
                            value: None,
                            text: None,
                            expected_revision: None,
                        });
                    } else if key == "F11" {
                        let _ = self.control(WatchControl {
                            action: "fullscreen".into(),
                            value: None,
                            text: None,
                            expected_revision: None,
                        });
                    } else {
                        if let Err(error) = self.engine.command(json!(["keydown", key])) {
                            self.state.error = Some(error.message);
                        }
                    }
                } else {
                    // This pinned mpv build dereferences the optional name in cmd_key;
                    // send an explicit empty string for release-all instead of NULL.
                    if let Err(error) = self.engine.command(json!(["keyup", ""])) {
                        self.state.error = Some(error.message);
                    }
                }
            }
            Message::Mouse(x, y, button) => {
                let x = (x * self.viewport.0 as f64).round() as i32;
                let y = (y * self.viewport.1 as f64).round() as i32;
                if let Err(error) = self.engine.command(json!(["mouse", x, y])) {
                    self.state.error = Some(error.message);
                }
                if let Some(down) = button {
                    if let Err(error) = self
                        .engine
                        .command(json!([if down { "keydown" } else { "keyup" }, "MBTN_LEFT"]))
                    {
                        self.state.error = Some(error.message);
                    }
                }
            }
            Message::Open(media, reply) => {
                let result = self.open(media).map(|_| self.state.clone());
                let _ = reply.send(result);
            }
            Message::Control(control, reply) => {
                let result = self.control(control).map(|_| self.state.clone());
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                } else if let Err(error) = result {
                    self.state.error = Some(error.message);
                }
            }
            Message::Viewport(w, h, visible) => {
                self.viewport = (w, h, visible);
                if !visible {
                    let _ = self.engine.command(json!(["set_property", "pause", true]));
                    self.state.playing = false;
                    self.invalidate();
                    self.save_progress();
                } else {
                    self.force_draw = true;
                    if !self.overlay.is_empty() {
                        let _ = self.engine.translation(&self.overlay, w, h);
                    }
                }
            }
            Message::Saved(session, revision, result) => {
                if let Ok(saved) = &result {
                    let _ = self.handle.emit_to(
                        "main",
                        "watch_collected",
                        json!({"entryId":saved.entry_id}),
                    );
                }
                if session == self.state.session_id && revision == self.state.revision {
                    match result {
                        Ok(result) => {
                            self.state.collected_entry_id = Some(result.entry_id);
                            self.state.status = "saved".into();
                        }
                        Err(error) => {
                            self.state.error = Some(error.message);
                            self.state.status = "failed".into();
                        }
                    }
                }
            }
            Message::Shutdown(reply) => {
                self.invalidate();
                self.stopped = true;
                self.shutdown_reply = Some(reply);
            }
        }
        self.publish();
    }
    fn invalidate(&mut self) {
        self.clock.cancel();
        self.user_pause = false;
        self.pending_context = None;
        self.pending_display = false;
        self.state.revision += 1;
        self.state.context = None;
        self.state.remaining_seconds = None;
        self.state.translation_visible_at = None;
        self.state.collection_started_at = None;
        self.state.collected_entry_id = None;
        self.state.status = "paused".into();
        self.overlay.clear();
        let _ = self
            .engine
            .translation("", self.viewport.0, self.viewport.1);
    }
    fn open(&mut self, media: WatchMedia) -> Result<(), AppError> {
        self.save_progress();
        self.invalidate();
        self.state.error = None;
        self.restore = self
            .application
            .store
            .watch_progress()?
            .filter(|p| p.path == media.path.to_string_lossy());
        self.state.session_id = uuid::Uuid::new_v4().to_string();
        self.state.path = media.path.to_string_lossy().into_owned();
        self.state.title = media.title.clone();
        self.state.source_id = media.source_id.clone();
        self.state.loaded = false;
        self.state.playing = false;
        self.state.position = 0.0;
        self.state.duration = 0.0;
        self.state.status = "loading".into();
        self.bound_sid = None;
        self.engine
            .command(json!(["set_property", "pause", true]))?;
        self.engine.command(json!(["loadfile", self.state.path]))?;
        self.media = Some(media);
        Ok(())
    }
    fn control(&mut self, control: WatchControl) -> Result<(), AppError> {
        if control
            .expected_revision
            .is_some_and(|v| v != self.state.revision)
        {
            return Err(AppError::new("conflict", "播放位置已变化，请重试。"));
        }
        self.state.error = None;
        match control.action.as_str() {
            "play" | "pause" | "toggle" => {
                if !self.state.loaded {
                    return Ok(());
                }
                let pause = match control.action.as_str() {
                    "play" => false,
                    "pause" => true,
                    _ => !self.engine.flag("pause"),
                };
                self.invalidate();
                self.engine
                    .command(json!(["set_property", "pause", pause]))?;
                self.state.playing = !pause;
                if pause {
                    self.user_pause = true;
                    self.pending_context = Some(Instant::now() + Duration::from_millis(60));
                    self.state.status = "reading".into();
                } else {
                    self.state.status = "playing".into();
                }
            }
            "seek" => {
                self.invalidate();
                let position = control
                    .value
                    .ok_or_else(|| failure("缺少播放位置"))?
                    .max(0.0);
                self.engine
                    .command(json!(["seek", position, "absolute+exact"]))?;
                self.state.position = position;
            }
            "volume" => {
                let volume = control.value.unwrap_or(100.0).clamp(0.0, 100.0);
                self.engine
                    .command(json!(["set_property", "volume", volume]))?;
                self.state.volume = volume;
            }
            "fullscreen" | "exit_fullscreen" => {
                if let Some(window) = self.handle.get_webview_window("main") {
                    let fullscreen = control.action != "exit_fullscreen"
                        && !window.is_fullscreen().unwrap_or(false);
                    window
                        .set_fullscreen(fullscreen)
                        .map_err(|e| failure(e.to_string()))?;
                }
            }
            "audio" | "subtitle" => {
                self.invalidate();
                let key = if control.action == "audio" {
                    "aid"
                } else {
                    "sid"
                };
                self.engine.command(json!([
                    "set_property",
                    key,
                    control
                        .value
                        .map(|n| json!(n as i64))
                        .unwrap_or(json!("no"))
                ]))?;
            }
            "subtitle_delay" => {
                self.invalidate();
                self.engine.command(json!([
                    "set_property",
                    "sub-delay",
                    control.value.unwrap_or(0.0)
                ]))?;
            }
            "subtitle_file" => {
                self.invalidate();
                let file = control.text.ok_or_else(|| failure("缺少字幕文件"))?;
                self.engine.command(json!(["sub-add", file, "select"]))?;
            }
            "cancel_auto" => {
                self.clock.cancel();
                self.pending_display = false;
                self.state.remaining_seconds = None;
                self.state.status = "cancelled".into();
            }
            "configure" => {
                let settings = self.application.settings()?;
                self.enabled = settings.watch_auto_collect;
                self.delay = settings.watch_collect_delay_seconds;
                self.clock.cancel();
                self.pending_display = false;
                self.state.remaining_seconds = None;
                self.state.status = "cancelled".into();
            }
            "refresh_subtitles" => {
                let paused=self.user_pause && self.engine.flag("pause");
                if let Some(media)=self.media.as_mut() {
                    media.subtitles=media.source_id.as_deref().map(|id|self.application.store.subtitle_bundle(id)).transpose()?.flatten();
                }
                self.invalidate();
                self.user_pause=paused;
                if paused { self.pending_context=Some(Instant::now()+Duration::from_millis(60)); }
            }
            "hide" => {
                self.engine
                    .command(json!(["set_property", "pause", true]))?;
                self.invalidate();
                self.state.playing = false;
                self.save_progress();
            }
            "close" => {
                self.save_progress();
                self.invalidate();
                self.engine.command(json!(["stop"]))?;
                self.media = None;
                self.state.loaded = false;
                self.state.title.clear();
                self.state.path.clear();
                self.state.source_id = None;
                self.state.status = "idle".into();
            }
            #[cfg(debug_assertions)]
            "capture_frame" => {
                self.engine.capture(
                    &self.application.store.root().join("jobs/watch-frame.png"),
                    self.viewport.0,
                    self.viewport.1,
                )?;
            }
            _ => return Err(failure("未知播放操作")),
        }
        Ok(())
    }
    fn loaded(&mut self) {
        self.state.loaded = true;
        self.state.status = "paused".into();
        let tracks = self.engine.property("track-list");
        if let Some(media) = &self.media {
            for (kind, index, key) in [
                ("sub", media.subtitle_stream, "sid"),
                ("audio", media.audio_stream, "aid"),
            ] {
                if let Some(index) = index
                    && let Some(track) = tracks.as_array().and_then(|t| {
                        t.iter().find(|t| {
                            t["type"] == kind && t["ff-index"].as_u64() == Some(index as u64)
                        })
                    })
                {
                    let _ = self
                        .engine
                        .command(json!(["set_property", key, track["id"]]));
                }
            }
        }
        self.bound_sid = self.engine.number("sid").map(|n| n as i64);
        if let Some(progress) = self.restore.take() {
            if progress.position > 0.0 {
                let _ = self
                    .engine
                    .command(json!(["seek", progress.position, "absolute+exact"]));
            }
            if let Some(audio) = progress.audio_id {
                let _ = self.engine.command(json!(["set_property", "aid", audio]));
            }
            if let Some(sub) = progress.subtitle_id {
                let _ = self.engine.command(json!(["set_property", "sid", sub]));
            }
            let _ = self.engine.command(json!([
                "set_property",
                "sub-delay",
                progress.subtitle_delay
            ]));
        }
    }
    fn prepare_context(&mut self) -> Result<(), AppError> {
        let Some(media) = &self.media else {
            return Ok(());
        };
        if !self.user_pause || !self.engine.flag("pause") {
            return Ok(());
        }
        if self.engine.number("sid").map(|n| n as i64) != self.bound_sid {
            self.state.status = "subtitle_changed".into();
            return Ok(());
        }
        let position = self.engine.number("time-pos").unwrap_or(0.0);
        let delay = self.engine.number("sub-delay").unwrap_or(0.0);
        let context = self.application.store.watch_context(
            media,
            (position * 1000.0) as i64,
            (delay * 1000.0) as i64,
            self.state.revision,
        )?;
        let Some(context) = context else {
            self.state.status = "no_subtitle".into();
            return Ok(());
        };
        let has_translation = context
            .translation
            .chars()
            .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c));
        self.state.context = Some(context.clone());
        if !has_translation {
            self.state.status = "missing_translation".into();
            return Ok(());
        }
        if let Some(receipt) = self.application.store.watch_receipt(&context)? {
            self.state.collected_entry_id = Some(receipt.entry_id);
            self.state.status = "already_saved".into();
        } else {
            self.state.status = if self.enabled {
                "awaiting_display"
            } else {
                "translation"
            }
            .into();
        }
        self.overlay = context.translation.clone();
        self.engine
            .translation(&self.overlay, self.viewport.0, self.viewport.1)?;
        self.pending_display = self.enabled && self.state.collected_entry_id.is_none();
        Ok(())
    }
    fn tick(&mut self) {
        for _ in 0..32 {
            match self.engine.event() {
                Some(8) => self.loaded(),
                Some(20) => {
                    self.invalidate();
                    self.state.playing = self.state.loaded && !self.engine.flag("pause");
                }
                Some(7) => {
                    self.state.playing = false;
                    self.state.status = "paused".into();
                    self.clock.cancel();
                    self.pending_display = false;
                }
                Some(_) => {}
                None => break,
            }
        }
        let now = Instant::now();
        // The built-in mouse controller changes pause directly. Route those user
        // transitions through the same context/timer path as Space and React.
        if self.state.loaded && self.state.playing == self.engine.flag("pause") {
            let pause = self.engine.flag("pause");
            self.invalidate();
            self.state.playing = !pause;
            if pause {
                if !self.engine.flag("eof-reached") && !self.engine.flag("seeking") {
                    self.user_pause = true;
                    self.pending_context = Some(now + Duration::from_millis(60));
                    self.state.status = "reading".into();
                } else {
                    self.state.status = "paused".into();
                }
            } else {
                self.state.status = "playing".into();
            }
        }
        let visible = self.viewport.2
            && unsafe {
                IsWindowVisible(HWND(self.window as *mut _)).as_bool()
                    && !IsIconic(GetAncestor(HWND(self.window as *mut _), GA_ROOT)).as_bool()
            };
        if !visible {
            self.clock.cancel();
            self.pending_display = false;
        }
        if self.pending_context.is_some_and(|at| now >= at) {
            self.pending_context = None;
            if let Err(error) = self.prepare_context() {
                self.state.error = Some(error.message);
                self.state.status = "failed".into();
            }
        }
        // A background import/preparation may finish while this exact sentence is paused.
        // Waiting for it never arms or backdates the display clock, and never queries a model.
        if visible
            && self.user_pause
            && self.state.status == "missing_translation"
            && now >= self.translation_retry_at
        {
            self.translation_retry_at = now + Duration::from_secs(1);
            if let Err(error) = self.prepare_context() {
                self.state.error = Some(error.message);
                self.state.status = "failed".into();
            }
        }
        if visible {
            let force = std::mem::take(&mut self.force_draw);
            match self.engine.draw(self.viewport.0, self.viewport.1, force) {
                Ok(true) if self.pending_display => {
                    self.pending_display = false;
                    if self.user_pause && self.engine.flag("pause") {
                        self.clock.arm(self.state.revision, Instant::now());
                        self.state.translation_visible_at =
                            Some(chrono::Utc::now().timestamp_millis());
                        self.state.status = "counting".into();
                    }
                }
                Err(error) => {
                    self.clock.cancel();
                    self.pending_display = false;
                    self.state.error = Some(error.message);
                    self.state.status = "failed".into();
                }
                _ => {}
            }
        }
        self.state.remaining_seconds =
            self.clock
                .remaining(self.state.revision, Instant::now(), self.delay);
        if self.state.remaining_seconds.is_some_and(|n| n <= 0.0)
            && visible
            && self.user_pause
            && self.engine.flag("pause")
        {
            self.clock.cancel();
            self.state.remaining_seconds = None;
            self.state.status = "saving".into();
            self.state.collection_started_at = Some(chrono::Utc::now().timestamp_millis());
            if let Some(context) = self.state.context.clone() {
                let app = self.application.clone();
                let sender = self.sender.clone();
                let session = self.state.session_id.clone();
                let revision = self.state.revision;
                std::thread::spawn(move || {
                    let result = (|| {
                        if let Some(receipt) = app.store.watch_receipt(&context)? {
                            return Ok(receipt);
                        }
                        let asset = app.store.ensure_clip(
                            &app.settings()?.tools,
                            &context.source_id,
                            &context.example_id,
                            &std::sync::atomic::AtomicBool::new(false),
                        )?;
                        app.store.watch_collect(&context, Some(&asset.id))
                    })();
                    let _ = sender.send(Message::Saved(session, revision, result));
                });
            }
        }
        if now.duration_since(self.poll_at) >= Duration::from_millis(200) {
            self.poll_at = now;
            if let Some(window) = self.handle.get_webview_window("main") {
                let was_fullscreen = self.fullscreen;
                let desktop = window.is_fullscreen().unwrap_or(false);
                if desktop != self.fullscreen {
                    self.fullscreen = desktop;
                    let _ = self
                        .engine
                        .command(json!(["set_property", "fullscreen", desktop]));
                } else {
                    let native = self.engine.flag("fullscreen");
                    if native != self.fullscreen {
                        self.fullscreen = native;
                        let _ = window.set_fullscreen(native);
                    }
                }
                if was_fullscreen && !self.fullscreen && self.state.status == "counting" {
                    self.clock.cancel();
                    self.state.remaining_seconds = None;
                    self.state.status = "cancelled".into();
                }
            }
            let notice = if self.fullscreen {
                match self.state.status.as_str() {
                    "counting" => format!(
                        "{} 秒后收录整句 · Esc 返回并取消",
                        self.state
                            .remaining_seconds
                            .unwrap_or(self.delay as f64)
                            .ceil() as u32
                    ),
                    "saved" => "已自动收录整句".into(),
                    "already_saved" => "本句已收录".into(),
                    "cancelled" => "本次不收录".into(),
                    "missing_translation" => "暂无已保存译文".into(),
                    "saving" => "正在收录整句…".into(),
                    _ => String::new(),
                }
            } else {
                String::new()
            };
            if notice != self.last_notice {
                let _ = self.engine.command(json!(["show-text", notice, 2000]));
                self.last_notice = notice;
            }
            self.state.position = self.engine.number("time-pos").unwrap_or(0.0);
            self.state.duration = self.engine.number("duration").unwrap_or(0.0);
            self.state.playing = self.state.loaded && !self.engine.flag("pause");
            self.state.volume = self.engine.number("volume").unwrap_or(self.state.volume);
            self.state.subtitle_delay = self.engine.number("sub-delay").unwrap_or(0.0);
            self.state.tracks = self
                .engine
                .property("track-list")
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|t| {
                            Some(WatchTrack {
                                id: t["id"].as_i64()?,
                                kind: t["type"].as_str()?.into(),
                                title: format!(
                                    "{} {}",
                                    t["lang"].as_str().unwrap_or(""),
                                    t["title"].as_str().or(t["codec"].as_str()).unwrap_or("")
                                )
                                .trim()
                                .to_owned(),
                                selected: t["selected"].as_bool().unwrap_or(false),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.publish();
        }
    }
    fn publish(&mut self) {
        let value = serde_json::to_string(&self.state).unwrap_or_default();
        if value != self.last_publish {
            self.last_publish = value;
            if let Ok(mut state) = self.published.lock() {
                *state = self.state.clone();
            }
            let _ = self.handle.emit_to(
                "main",
                "watch_updated",
                json!({"sessionId":self.state.session_id,"revision":self.state.revision}),
            );
        }
    }
    fn save_progress(&self) {
        if self.state.path.is_empty() {
            return;
        }
        let _ = self.application.store.save_watch_progress(&WatchProgress {
            source_id: self.state.source_id.clone(),
            path: self.state.path.clone(),
            position: self
                .engine
                .number("time-pos")
                .unwrap_or(self.state.position),
            subtitle_id: self.engine.number("sid").map(|n| n as i64),
            audio_id: self.engine.number("aid").map(|n| n as i64),
            subtitle_delay: self.engine.number("sub-delay").unwrap_or(0.0),
        });
    }
}
