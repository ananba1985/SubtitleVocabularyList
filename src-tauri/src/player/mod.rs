pub(crate) mod files;
mod menu;
mod mpv;

use crate::{
    application::Application,
    error::AppError,
    playback::{PlaybackFiles, PlaybackState, PlaylistChange, PlaylistItem},
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
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::{
            CombineRgn, CreateRectRgn, DeleteObject, RGN_DIFF, ScreenToClient, SetWindowRgn,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                GetCapture, GetKeyboardState, ReleaseCapture, SetCapture, SetFocus, ToUnicode,
            },
            Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP},
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
    pub secondary: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WatchChapter {
    pub index: usize,
    pub title: String,
    pub position: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WatchFrame {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub position_ms: i64,
    pub text: String,
    pub busy: bool,
    pub error: Option<String>,
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
    pub frame: Option<WatchFrame>,
    pub playback: PlaybackState,
    pub playlist_index: Option<usize>,
    pub chapters: Vec<WatchChapter>,
    pub chapter_index: Option<usize>,
    pub loop_a: Option<f64>,
    pub loop_b: Option<f64>,
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
    Mouse(f64, f64, Option<(String, bool)>),
    Wheel(f64, f64, String, f64),
    ContextMenu,
    MenuAction(String, WatchControl),
    MenuClosed(Option<String>),
    Open(WatchMedia, Reply),
    Playlist(PlaylistChange, Vec<PlaylistItem>, Reply),
    Files(PlaybackFiles, Reply),
    Dropped(Vec<String>),
    Prepared(u64, u64, Result<WatchMedia, AppError>),
    Control(WatchControl, Option<Reply>),
    Viewport(i32, i32, bool),
    Saved(
        String,
        u64,
        Result<crate::vocabulary::CollectionResult, AppError>,
    ),
    Recognized(u64, Result<String, AppError>),
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
        if message == WM_DROPFILES {
            let drop = HDROP(wparam.0 as *mut _);
            let mut paths = vec![];
            for i in 0..DragQueryFileW(drop, u32::MAX, None) {
                let length = DragQueryFileW(drop, i, None) as usize;
                let mut path = vec![0u16; length + 1];
                DragQueryFileW(drop, i, Some(&mut path));
                paths.push(String::from_utf16_lossy(&path[..length]));
            }
            DragFinish(drop);
            if let Some(sender) = KEYBOARD
                .get()
                .and_then(|k| k.lock().ok())
                .and_then(|s| s.clone())
            {
                let _ = sender.send(Message::Dropped(paths));
            }
            return LRESULT(0);
        }
        if matches!(message, WM_RBUTTONDOWN | WM_RBUTTONUP) {
            if message == WM_RBUTTONDOWN {
                let _ = SetFocus(Some(hwnd));
            }
            if message == WM_RBUTTONUP
                && let Some(sender) = KEYBOARD
                    .get()
                    .and_then(|k| k.lock().ok())
                    .and_then(|s| s.clone())
            {
                let _ = sender.send(Message::ContextMenu);
            }
            return LRESULT(0);
        }
        if matches!(
            message,
            WM_MOUSEMOVE
                | WM_LBUTTONDOWN
                | WM_LBUTTONUP
                | WM_MBUTTONDOWN
                | WM_MBUTTONUP
                | WM_XBUTTONDOWN
                | WM_XBUTTONUP
                | WM_MOUSEWHEEL
                | WM_MOUSEHWHEEL
        ) {
            if matches!(message, WM_LBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN) {
                let _ = SetFocus(Some(hwnd));
                SetCapture(hwnd);
            } else if matches!(message, WM_LBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP)
                && GetCapture() == hwnd
            {
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
                let mut point = POINT {
                    x: lparam.0 as i16 as i32,
                    y: (lparam.0 >> 16) as i16 as i32,
                };
                let wheel = matches!(message, WM_MOUSEWHEEL | WM_MOUSEHWHEEL);
                if wheel {
                    let _ = ScreenToClient(hwnd, &mut point);
                }
                let x = point.x as f64 / (client.right - client.left).max(1) as f64;
                let y = point.y as f64 / (client.bottom - client.top).max(1) as f64;
                if wheel {
                    let delta = ((wparam.0 >> 16) as i16) as f64 / 120.0;
                    let key = if message == WM_MOUSEWHEEL {
                        if delta > 0.0 {
                            "WHEEL_UP"
                        } else {
                            "WHEEL_DOWN"
                        }
                    } else if delta > 0.0 {
                        "WHEEL_RIGHT"
                    } else {
                        "WHEEL_LEFT"
                    };
                    let _ = sender.send(Message::Wheel(x, y, mouse_key(key), delta.abs()));
                    return LRESULT(0);
                }
                let _ = sender.send(Message::Mouse(
                    x,
                    y,
                    match message {
                        WM_LBUTTONDOWN | WM_LBUTTONUP => {
                            Some((mouse_key("MBTN_LEFT"), message == WM_LBUTTONDOWN))
                        }
                        WM_MBUTTONDOWN | WM_MBUTTONUP => {
                            Some((mouse_key("MBTN_MID"), message == WM_MBUTTONDOWN))
                        }
                        WM_XBUTTONDOWN | WM_XBUTTONUP => Some((
                            mouse_key(if (wparam.0 >> 16) as u16 == 1 {
                                "MBTN_BACK"
                            } else {
                                "MBTN_FORWARD"
                            }),
                            message == WM_XBUTTONDOWN,
                        )),
                        _ => None,
                    },
                ));
            }
            return LRESULT(if matches!(message, WM_XBUTTONDOWN | WM_XBUTTONUP) {
                1
            } else {
                0
            });
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

unsafe fn mouse_key(key: &str) -> String {
    unsafe {
        let mut keys = [0u8; 256];
        let _ = GetKeyboardState(&mut keys);
        format!(
            "{}{}{}{}",
            if keys[0x11] & 0x80 != 0 { "Ctrl+" } else { "" },
            if keys[0x12] & 0x80 != 0 { "Alt+" } else { "" },
            if keys[0x10] & 0x80 != 0 { "Shift+" } else { "" },
            key
        )
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
            0x5d => Some("MENU"),
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
                        DragAcceptFiles(host, true);
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
    pub fn playlist(
        &self,
        handle: &tauri::AppHandle,
        change: PlaylistChange,
        items: Vec<PlaylistItem>,
    ) -> Result<PlayerSnapshot, AppError> {
        let sender = self.ensure_worker(handle)?;
        let (tx, rx) = mpsc::channel();
        sender
            .send(Message::Playlist(change, items, tx))
            .map_err(|_| failure("播放会话已结束"))?;
        rx.recv_timeout(Duration::from_secs(15))
            .map_err(|_| failure("等待播放列表操作超时"))?
    }
    pub fn files(
        &self,
        handle: &tauri::AppHandle,
        files: PlaybackFiles,
    ) -> Result<PlayerSnapshot, AppError> {
        let sender = self.ensure_worker(handle)?;
        let (tx, rx) = mpsc::channel();
        sender
            .send(Message::Files(files, tx))
            .map_err(|_| failure("播放会话已结束"))?;
        rx.recv_timeout(Duration::from_secs(15))
            .map_err(|_| failure("等待文件打开超时"))?
    }
    pub fn control_ready(
        &self,
        handle: &tauri::AppHandle,
        control: WatchControl,
    ) -> Result<PlayerSnapshot, AppError> {
        self.ensure_worker(handle)?;
        self.control(control)
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
    ocr_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    audio_active: bool,
    prepared_media: Option<WatchMedia>,
    load_generation: u64,
    progress: Option<WatchProgress>,
    progress_at: Instant,
    menu_open: bool,
    subtitle_dialog_open: bool,
    pending_subtitles: Option<(String, Vec<String>)>,
    refresh_after_seek: bool,
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
        let playback = application.store.playback_state().unwrap_or_default();
        for (key, value) in [
            ("speed", json!(playback.preferences.speed)),
            ("volume", json!(playback.preferences.volume)),
            ("mute", json!(playback.preferences.muted)),
            ("sub-scale", json!(playback.preferences.subtitle_scale)),
            ("sub-pos", json!(playback.preferences.subtitle_position)),
            (
                "sub-visibility",
                json!(playback.preferences.subtitle_visible),
            ),
            (
                "secondary-sub-visibility",
                json!(playback.preferences.secondary_subtitle_visible),
            ),
            (
                "loop-file",
                json!(if playback.preferences.loop_mode == "file" {
                    "inf"
                } else {
                    "no"
                }),
            ),
            (
                "loop-playlist",
                json!(if playback.preferences.loop_mode == "playlist" {
                    "inf"
                } else {
                    "no"
                }),
            ),
            (
                "keep-open",
                json!(if playback.preferences.auto_next {
                    "yes"
                } else {
                    "always"
                }),
            ),
        ] {
            let _ = engine.command(json!(["set_property", key, value]));
        }
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
                playback,
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
            ocr_cancel: None,
            audio_active: false,
            prepared_media: None,
            load_generation: 0,
            progress: None,
            progress_at: Instant::now(),
            menu_open: false,
            subtitle_dialog_open: false,
            pending_subtitles: None,
            refresh_after_seek: false,
        }
    }
    fn receive(&mut self, message: Message) {
        match message {
            Message::Key(key) => {
                if let Some(key) = key {
                    if matches!(key.as_str(), "MENU" | "Shift+F10") {
                        if let Err(error) = self.show_menu() {
                            self.state.error = Some(error.message);
                        }
                    } else if matches!(key.as_str(), "q" | "Q")
                        && !self.engine.flag("user-data/mpv/console/open")
                    {
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
                        if !self.engine.flag("user-data/mpv/console/open")
                            && ["LEFT", "RIGHT", "UP", "DOWN", "PGUP", "PGDWN", "HOME"]
                                .iter()
                                .any(|k| key.ends_with(k))
                        {
                            self.refresh_after_seek =
                                self.state.loaded && self.engine.flag("pause");
                        }
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
                if let Some((key, down)) = button {
                    if down && self.state.loaded && self.engine.flag("pause") {
                        self.refresh_after_seek = true;
                    }
                    if let Err(error) = self
                        .engine
                        .command(json!([if down { "keydown" } else { "keyup" }, key]))
                    {
                        self.state.error = Some(error.message);
                    }
                }
            }
            Message::Wheel(x, y, key, scale) => {
                if key.ends_with("WHEEL_LEFT") || key.ends_with("WHEEL_RIGHT") {
                    self.refresh_after_seek = self.state.loaded && self.engine.flag("pause");
                }
                let _ = self.engine.command(json!([
                    "mouse",
                    (x * self.viewport.0 as f64).round() as i32,
                    (y * self.viewport.1 as f64).round() as i32
                ]));
                if let Err(error) = self.engine.command(json!(["keypress", key, scale])) {
                    self.state.error = Some(error.message);
                }
            }
            Message::ContextMenu => {
                if let Err(error) = self.show_menu() {
                    self.state.error = Some(error.message);
                }
            }
            Message::MenuAction(session, control) => {
                if session == self.state.session_id
                    && let Err(error) = self.control(control)
                {
                    let _ = self
                        .engine
                        .command(json!(["show-text", error.message, 3000]));
                    self.state.error = Some(error.message);
                }
            }
            Message::MenuClosed(error) => {
                self.menu_open = false;
                if let Some(error) = error {
                    self.state.error = Some(error);
                }
            }
            Message::Open(media, reply) => {
                let result = self.open(media).map(|_| self.state.clone());
                let _ = reply.send(result);
            }
            Message::Playlist(change, items, reply) => {
                let result = self.playlist(change, items).map(|_| self.state.clone());
                let _ = reply.send(result);
            }
            Message::Files(files, reply) => {
                let result = self.files(files).map(|_| self.state.clone());
                let _ = reply.send(result);
            }
            Message::Dropped(paths) => files::receive(&self.handle, paths, false),
            Message::Prepared(generation, hook, result) => {
                let mut failed = false;
                if generation == self.load_generation {
                    match result {
                        Ok(media) => self.accept_media(media),
                        Err(error) => {
                            self.state.error = Some(error.message);
                            self.state.status = "failed".into();
                            failed = true;
                        }
                    }
                }
                let _ = self.engine.continue_load(hook);
                if failed {
                    let _ = self.engine.command(json!(["stop"]));
                }
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
            Message::Recognized(revision, result) => {
                if revision == self.state.revision && self.user_pause && self.engine.flag("pause") {
                    if let Some(frame) = self.state.frame.as_mut() {
                        self.ocr_cancel = None;
                        frame.busy = false;
                        match result {
                            Ok(text) => {
                                frame.text = text;
                                frame.error = None;
                            }
                            Err(error) => frame.error = Some(error.message),
                        }
                        self.state.status = "ocr_review".into();
                    }
                }
            }
            Message::Shutdown(reply) => {
                self.save_progress();
                self.invalidate();
                self.stopped = true;
                self.shutdown_reply = Some(reply);
            }
        }
        self.publish();
    }
    fn invalidate(&mut self) {
        if let Some(flag) = self.ocr_cancel.take() {
            flag.store(true, Ordering::Relaxed);
        }
        if let Some(frame) = self.state.frame.take() {
            let _ = std::fs::remove_file(frame.path);
        }
        if let Some(query) = self.application.queries.owner("watch") {
            let _ = self.application.queries.cancel(&query.session_id, None);
        }
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
        self.hydrate_queue()?;
        let path = media.path.to_string_lossy().into_owned();
        let index = if let Some(index) = self
            .state
            .playback
            .playlist
            .iter()
            .position(|p| p.path == path)
        {
            index
        } else {
            self.engine.command(json!(["loadfile", path, "append"]))?;
            self.state.playback.playlist.push(PlaylistItem {
                path,
                title: media.title.clone(),
                source_id: media.source_id.clone(),
            });
            self.state.playback.playlist.len() - 1
        };
        self.prepared_media = Some(media);
        self.engine
            .command(json!(["set_property", "pause", true]))?;
        self.engine.command(json!(["playlist-play-index", index]))?;
        self.sync_playlist()?;
        Ok(())
    }
    fn hydrate_queue(&self) -> Result<(), AppError> {
        if self.engine.number("playlist-count").unwrap_or(0.0) == 0.0 {
            for item in &self.state.playback.playlist {
                self.engine
                    .command(json!(["loadfile", item.path, "append"]))?;
            }
        }
        Ok(())
    }
    fn playlist(
        &mut self,
        change: PlaylistChange,
        items: Vec<PlaylistItem>,
    ) -> Result<(), AppError> {
        self.save_progress();
        self.hydrate_queue()?;
        let index = change.index.unwrap_or(0);
        let count = self.state.playback.playlist.len();
        match change.action.as_str() {
            "append" | "enqueue" => {
                let first = items.first().map(|item| item.path.clone());
                for item in items {
                    if !self
                        .state
                        .playback
                        .playlist
                        .iter()
                        .any(|p| p.path == item.path)
                    {
                        self.engine
                            .command(json!(["loadfile", item.path, "append"]))?;
                        self.state.playback.playlist.push(item);
                    }
                }
                if change.action == "append"
                    && self.state.path.is_empty()
                    && !self.state.playback.playlist.is_empty()
                {
                    let index = first
                        .and_then(|path| {
                            self.state
                                .playback
                                .playlist
                                .iter()
                                .position(|p| p.path == path)
                        })
                        .unwrap_or(0);
                    self.engine
                        .command(json!(["set_property", "pause", true]))?;
                    self.engine.command(json!(["playlist-play-index", index]))?;
                }
            }
            "select" | "restore" => {
                let index = if change.action == "restore" {
                    self.state
                        .playback
                        .current_path
                        .as_ref()
                        .and_then(|path| {
                            self.state
                                .playback
                                .playlist
                                .iter()
                                .position(|p| &p.path == path)
                        })
                        .unwrap_or(0)
                } else {
                    index
                };
                if index >= count {
                    return Err(failure("请选择播放列表中的视频"));
                }
                self.invalidate();
                if !self.state.loaded {
                    self.engine
                        .command(json!(["set_property", "pause", true]))?;
                }
                self.engine.command(json!(["playlist-play-index", index]))?;
            }
            "previous" | "next" => {
                self.invalidate();
                self.engine.command(json!([
                    if change.action == "next" {
                        "playlist-next"
                    } else {
                        "playlist-prev"
                    },
                    "weak"
                ]))?;
            }
            "remove" if index < count => {
                if self.state.playlist_index == Some(index) {
                    self.invalidate();
                }
                self.engine.command(json!(["playlist-remove", index]))?;
            }
            "move" if index < count => {
                let dest = change
                    .destination
                    .filter(|d| *d < count)
                    .ok_or_else(|| failure("无效的播放列表位置"))?;
                // mpv's destination is an insertion point in the original list.
                self.engine.command(json!([
                    "playlist-move",
                    index,
                    if dest > index { dest + 1 } else { dest }
                ]))?;
            }
            "clear" => {
                self.control(WatchControl {
                    action: "close".into(),
                    value: None,
                    text: None,
                    expected_revision: None,
                })?;
                self.engine.command(json!(["playlist-clear"]))?;
                self.state.playback.current_path = None;
            }
            _ => return Err(failure("无效的播放列表操作")),
        }
        self.sync_playlist()?;
        Ok(())
    }
    fn files(&mut self, files: PlaybackFiles) -> Result<(), AppError> {
        let empty = self.state.path.is_empty();
        let first = files.items.first().map(|i| i.path.clone());
        let target = files.open_path.clone().or_else(|| {
            if empty {
                first
            } else {
                // path can still describe the previous file while START_FILE is pending.
                self.engine
                    .property("path")
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| Some(self.state.path.clone()))
            }
        });
        if !files.subtitles.is_empty() && target.is_none() {
            return Err(failure("请先打开视频，再拖入字幕。"));
        }
        if !files.items.is_empty() {
            self.playlist(
                PlaylistChange {
                    action: if files.open_path.is_some() {
                        "enqueue"
                    } else {
                        "append"
                    }
                    .into(),
                    paths: vec![],
                    index: None,
                    destination: None,
                },
                files.items,
            )?;
        }
        if let Some(path) = &files.open_path {
            let index = self
                .state
                .playback
                .playlist
                .iter()
                .position(|p| &p.path == path)
                .ok_or_else(|| failure("视频不在播放列表中"))?;
            self.pending_subtitles = None;
            self.invalidate();
            self.engine
                .command(json!(["set_property", "pause", true]))?;
            self.engine.command(json!(["playlist-play-index", index]))?;
        }
        if !files.subtitles.is_empty()
            && let Some(path) = target
        {
            if files.open_path.is_none() && !empty && self.state.loaded && self.state.path == path {
                self.load_subtitles(files.subtitles)?;
            } else {
                self.pending_subtitles = Some((path, files.subtitles));
            }
        }
        self.sync_playlist()?;
        Ok(())
    }
    fn load_subtitles(&mut self, paths: Vec<String>) -> Result<(), AppError> {
        let paused = self.engine.flag("pause");
        self.invalidate();
        for (index, path) in paths.iter().enumerate() {
            self.engine.command(json!([
                "sub-add",
                path,
                if index == 0 { "select" } else { "auto" }
            ]))?;
            if index == 1 {
                let tracks = self.engine.property("track-list");
                if let Some(id) = tracks
                    .as_array()
                    .and_then(|t| {
                        t.iter()
                            .find(|t| t["external-filename"].as_str() == Some(path))
                    })
                    .and_then(|t| t["id"].as_i64())
                {
                    self.engine
                        .command(json!(["set_property", "secondary-sid", id]))?;
                }
            }
        }
        self.engine
            .command(json!(["set_property", "sub-visibility", true]))?;
        self.refresh_controls();
        self.sync_preferences()?;
        self.refresh_paused(paused);
        self.save_progress();
        let _ = self.engine.command(json!([
            "show-text",
            format!("已加载 {} 份字幕", paths.len()),
            2000
        ]));
        Ok(())
    }
    fn sync_playlist(&mut self) -> Result<(), AppError> {
        let before = self.state.playback.clone();
        let entries = self.engine.property("playlist");
        let old = &self.state.playback.playlist;
        let queue = entries
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let path = p["filename"].as_str()?.to_owned();
                Some(
                    old.iter()
                        .find(|e| e.path == path)
                        .cloned()
                        .unwrap_or_else(|| PlaylistItem {
                            title: std::path::Path::new(&path)
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                            path,
                            source_id: None,
                        }),
                )
            })
            .collect();
        self.state.playback.playlist = queue;
        if self.state.playback.playlist.is_empty() {
            self.state.playback.current_path = None;
        }
        self.state.playlist_index = self
            .engine
            .number("playlist-pos")
            .filter(|n| *n >= 0.0)
            .map(|n| n as usize);
        if before != self.state.playback {
            self.application
                .store
                .save_playback_state(&self.state.playback)?;
        }
        Ok(())
    }
    fn begin_file(&mut self) {
        self.save_progress();
        self.invalidate();
        self.load_generation += 1;
        self.refresh_after_seek = false;
        let _ = self
            .engine
            .command(json!(["set_property", "ab-loop-a", "no"]));
        let _ = self
            .engine
            .command(json!(["set_property", "ab-loop-b", "no"]));
        self.state.session_id = uuid::Uuid::new_v4().to_string();
        self.state.loaded = false;
        self.state.playing = false;
        self.state.error = None;
        self.state.status = "loading".into();
        self.state.position = 0.0;
        self.state.duration = 0.0;
        self.state.tracks.clear();
        self.media = None;
        self.progress = None;
        self.bound_sid = None;
    }
    fn prepare_file(&mut self, hook: u64) {
        let path = self
            .engine
            .property("path")
            .as_str()
            .unwrap_or("")
            .to_owned();
        self.state.path = path.clone();
        if self
            .pending_subtitles
            .as_ref()
            .is_some_and(|(target, _)| target != &path)
        {
            self.pending_subtitles = None;
        }
        self.state.playback.current_path = Some(path.clone());
        let item = self.state.playback.playlist.iter().find(|i| i.path == path);
        self.state.source_id = item.and_then(|i| i.source_id.clone());
        self.state.title = item
            .map(|i| i.title.clone())
            .unwrap_or_else(|| path.clone());
        if self
            .prepared_media
            .as_ref()
            .is_some_and(|m| m.path.to_string_lossy() == path)
        {
            let media = self.prepared_media.take().unwrap();
            self.accept_media(media);
            let _ = self.engine.continue_load(hook);
        } else {
            let app = self.application.clone();
            let sender = self.sender.clone();
            let generation = self.load_generation;
            let source = self.state.source_id.clone();
            std::thread::spawn(move || {
                let media = app.store.watch_media(Some(&path), source.as_deref());
                let _ = sender.send(Message::Prepared(generation, hook, media));
            });
        }
        if let Err(error) = self.sync_playlist() {
            self.state.error = Some(error.message);
        }
    }
    fn accept_media(&mut self, media: WatchMedia) {
        self.restore = self
            .application
            .store
            .watch_progress_for(&self.state.path)
            .ok()
            .flatten();
        self.state.source_id = media.source_id.clone();
        self.state.title = media.title.clone();
        for item in &mut self.state.playback.playlist {
            if item.path == self.state.path {
                item.source_id = media.source_id.clone();
                item.title = media.title.clone();
            }
        }
        self.media = Some(media);
        let _ = self
            .application
            .store
            .save_playback_state(&self.state.playback);
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
            "subtitle_dialog" => {
                self.clock.cancel();
                self.pending_display = false;
                self.state.remaining_seconds = None;
                self.state.status = "cancelled".into();
                self.subtitle_dialog_open = true;
                self.handle
                    .emit_to(
                        "main",
                        "watch_subtitle_file_requested",
                        json!({"revision":self.state.revision}),
                    )
                    .map_err(|e| failure(e.to_string()))?;
            }
            "subtitle_dialog_closed" => self.subtitle_dialog_open = false,
            "menu" => self.show_menu()?,
            "playlist_previous" | "playlist_next" => self.playlist(
                PlaylistChange {
                    action: if control.action == "playlist_next" {
                        "next"
                    } else {
                        "previous"
                    }
                    .into(),
                    paths: vec![],
                    index: None,
                    destination: None,
                },
                vec![],
            )?,
            "seek_prompt" => {
                self.clock.cancel();
                self.pending_display = false;
                self.state.remaining_seconds = None;
                self.state.status = "cancelled".into();
                self.engine.time_prompt(
                    &self.state.session_id,
                    self.engine.number("time-pos").unwrap_or(0.0),
                )?;
            }
            "seek_time" => {
                let position = crate::playback::parse_time(control.text.as_deref().unwrap_or(""))?;
                if position > self.engine.number("duration").unwrap_or(0.0) {
                    return Err(failure("跳转位置超过视频时长。"));
                }
                self.refresh_after_seek = self.engine.flag("pause");
                self.invalidate();
                self.engine
                    .command(json!(["seek", position, "absolute+exact"]))?;
            }
            "seek_relative" | "subtitle_seek" | "chapter" => {
                let value = control
                    .value
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| failure("缺少有效的定位位置"))?;
                self.refresh_after_seek = self.engine.flag("pause");
                self.invalidate();
                let command = match control.action.as_str() {
                    "seek_relative" => json!(["seek", value, "relative+exact"]),
                    "subtitle_seek" => {
                        let target = if self.engine.number("sid").is_some() {
                            "primary"
                        } else if self.engine.number("secondary-sid").is_some() {
                            "secondary"
                        } else {
                            return Err(failure("当前没有可定位的字幕。"));
                        };
                        json!(["sub-seek", value as i64, target])
                    }
                    _ => json!(["set_property", "chapter", value as i64]),
                };
                self.engine.command(command)?;
                if control.action == "subtitle_seek" && control.value == Some(0.0) {
                    self.refresh_after_seek = false;
                    self.engine
                        .command(json!(["set_property", "pause", false]))?;
                    self.state.playing = true;
                    self.state.status = "playing".into();
                }
            }
            "loop_mode" => {
                let mode = control.text.as_deref().unwrap_or("none");
                if !matches!(mode, "none" | "file" | "playlist") {
                    return Err(failure("未知循环模式"));
                }
                self.engine.command(json!([
                    "set_property",
                    "loop-file",
                    if mode == "file" { "inf" } else { "no" }
                ]))?;
                self.engine.command(json!([
                    "set_property",
                    "loop-playlist",
                    if mode == "playlist" { "inf" } else { "no" }
                ]))?;
                self.engine
                    .command(json!(["set_property", "ab-loop-a", "no"]))?;
                self.engine
                    .command(json!(["set_property", "ab-loop-b", "no"]))?;
                if mode == "playlist" {
                    self.state.playback.preferences.auto_next = true;
                    self.engine
                        .command(json!(["set_property", "keep-open", "yes"]))?;
                }
            }
            "loop_a" | "loop_b" | "loop_clear" => {
                if control.action == "loop_clear" {
                    self.engine
                        .command(json!(["set_property", "ab-loop-a", "no"]))?;
                    self.engine
                        .command(json!(["set_property", "ab-loop-b", "no"]))?;
                } else {
                    let position = self
                        .engine
                        .number("time-pos")
                        .ok_or_else(|| failure("请先打开视频"))?;
                    if control.action == "loop_b"
                        && self
                            .engine
                            .number("ab-loop-a")
                            .is_none_or(|a| a >= position)
                    {
                        return Err(failure("请先设置 A，B 必须在 A 之后。"));
                    }
                    if control.action == "loop_a" {
                        self.engine
                            .command(json!(["set_property", "ab-loop-b", "no"]))?;
                    }
                    self.engine.command(json!([
                        "set_property",
                        if control.action == "loop_a" {
                            "ab-loop-a"
                        } else {
                            "ab-loop-b"
                        },
                        position
                    ]))?;
                }
            }
            "subtitle_scale"
            | "subtitle_position"
            | "subtitle_visible"
            | "secondary_subtitle_visible" => {
                let paused = self.state.loaded && self.engine.flag("pause");
                let value = control
                    .value
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| failure("缺少字幕设置值"))?;
                let (key, value) = match control.action.as_str() {
                    "subtitle_scale" => ("sub-scale", json!(value.clamp(0.5, 3.0))),
                    "subtitle_position" => ("sub-pos", json!(value.clamp(0.0, 100.0))),
                    "subtitle_visible" => ("sub-visibility", json!(value != 0.0)),
                    _ => ("secondary-sub-visibility", json!(value != 0.0)),
                };
                self.invalidate();
                self.engine.command(json!(["set_property", key, value]))?;
                self.refresh_paused(paused);
            }
            "speed" => {
                let speed = control
                    .value
                    .filter(|n| n.is_finite() && (0.1..=4.0).contains(n))
                    .ok_or_else(|| failure("倍速范围为 0.1 至 4"))?;
                self.engine
                    .command(json!(["set_property", "speed", speed]))?;
            }
            "mute" => {
                self.engine.command(json!([
                    "set_property",
                    "mute",
                    control.value.unwrap_or(0.0) != 0.0
                ]))?;
            }
            "auto_next" => {
                self.state.playback.preferences.auto_next = control.value.unwrap_or(1.0) != 0.0;
                if !self.state.playback.preferences.auto_next {
                    self.engine
                        .command(json!(["set_property", "loop-playlist", "no"]))?;
                }
                self.engine.command(json!([
                    "set_property",
                    "keep-open",
                    if self.state.playback.preferences.auto_next {
                        "yes"
                    } else {
                        "always"
                    }
                ]))?;
                self.application
                    .store
                    .save_playback_state(&self.state.playback)?;
            }
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
                self.refresh_after_seek = self.state.loaded && self.engine.flag("pause");
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
            "audio" | "subtitle" | "secondary_subtitle" => {
                let paused = self.state.loaded && self.engine.flag("pause");
                self.invalidate();
                let key = if control.action == "audio" {
                    "aid"
                } else if control.action == "subtitle" {
                    "sid"
                } else {
                    "secondary-sid"
                };
                self.engine.command(json!([
                    "set_property",
                    key,
                    control
                        .value
                        .map(|n| json!(n as i64))
                        .unwrap_or(json!("no"))
                ]))?;
                self.refresh_paused(paused);
            }
            "subtitle_delay" => {
                let paused = self.state.loaded && self.engine.flag("pause");
                self.invalidate();
                self.engine.command(json!([
                    "set_property",
                    "sub-delay",
                    control.value.unwrap_or(0.0)
                ]))?;
                self.refresh_paused(paused);
            }
            "subtitle_file" => {
                let paused = self.state.loaded && self.engine.flag("pause");
                self.invalidate();
                let file = control.text.ok_or_else(|| failure("缺少字幕文件"))?;
                self.engine.command(json!(["sub-add", file, "select"]))?;
                self.refresh_paused(paused);
            }
            "subtitle_asset" => {
                let id = control.text.ok_or_else(|| failure("缺少字幕资产"))?;
                let media = self.media.as_ref().ok_or_else(|| failure("请先打开剧集"))?;
                let source = media
                    .source_id
                    .as_deref()
                    .ok_or_else(|| failure("请先选择已导入剧集"))?;
                let bundle = self
                    .application
                    .store
                    .subtitle_bundle(source)?
                    .ok_or_else(|| failure("字幕资产不存在"))?;
                let asset = bundle
                    .assets
                    .iter()
                    .find(|a| a.id == id)
                    .ok_or_else(|| failure("字幕资产不存在"))?;
                let path = self.application.store.root().join(&asset.relative_path);
                self.invalidate();
                self.engine
                    .command(json!(["sub-add", path.to_string_lossy(), "select"]))?;
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
            "query_translation" => {
                let query = self
                    .application
                    .queries
                    .get(control.text.as_deref())?
                    .ok_or_else(|| failure("当前查询已结束"))?;
                let context = self
                    .state
                    .context
                    .as_ref()
                    .ok_or_else(|| failure("当前没有可翻译原句"))?;
                if !self.user_pause
                    || !self.engine.flag("pause")
                    || query.input.origin.kind != "watch"
                    || query.input.origin.location_key
                        != format!("{}:{}", self.state.session_id, self.state.revision)
                    || query.input.mode != "translate"
                    || query.input.target != context.text
                {
                    return Err(AppError::new(
                        "conflict",
                        "播放上下文已变化，未采用旧译文。",
                    ));
                }
                if !query.results.is_empty()
                    && query
                        .results
                        .iter()
                        .all(|r| matches!(r.state.as_str(), "failed" | "cancelled"))
                {
                    self.overlay.clear();
                    self.engine
                        .translation("", self.viewport.0, self.viewport.1)?;
                    self.state.status = "translation_failed".into();
                }
                if let Some(result) = query.results.iter().find(|r| r.value.is_some()) {
                    let value = result.value.as_ref().unwrap();
                    if result.state == "ready" {
                        if context.translation == value.translation && !self.overlay.is_empty() {
                            return Ok(());
                        }
                        let settings = self.application.settings()?;
                        self.application.store.save_sentence_translation(
                            &context.text,
                            &value.translation,
                            &settings.model_url,
                            &settings.model_name,
                        )?;
                        self.refresh_translation()?;
                    } else if result.state == "running"
                        && !value.translation.is_empty()
                        && context.translation.is_empty()
                    {
                        self.clock.cancel();
                        self.pending_display = false;
                        self.state.remaining_seconds = None;
                        self.state.status = "translating".into();
                        self.overlay = value.translation.clone();
                        self.engine
                            .translation(&self.overlay, self.viewport.0, self.viewport.1)?;
                    }
                }
            }
            "ocr_crop" => {
                let rect: crate::ocr::PixelRect =
                    serde_json::from_str(control.text.as_deref().unwrap_or(""))?;
                self.recognize_frame(Some(rect))?;
            }
            "confirm_ocr" => {
                if !self.user_pause || !self.engine.flag("pause") {
                    return Err(failure("请先暂停后核对文字"));
                }
                let position = self
                    .state
                    .frame
                    .as_ref()
                    .ok_or_else(|| failure("暂停帧已失效"))?
                    .position_ms;
                let text = control.text.unwrap_or_default().trim().to_owned();
                crate::explanations::validate_input(&text, "")?;
                if text.is_empty() {
                    return Err(failure("请核对识别到的英文原文"));
                }
                let media = self.media.as_ref().ok_or_else(|| failure("视频已关闭"))?;
                let translation = self
                    .application
                    .store
                    .sentence_translation(&text)?
                    .unwrap_or_default();
                let context = WatchContext {
                    revision: self.state.revision + 1,
                    source_id: media.source_id.clone().unwrap_or_default(),
                    example_id: String::new(),
                    text: text.clone(),
                    translation_revision: crate::vocabulary::digest(translation.as_bytes()),
                    translation,
                    start_ms: position,
                    end_ms: position,
                    text_source: "frame_ocr".into(),
                    previous: false,
                    location_key: format!(
                        "watch-ocr:{position}:{}",
                        crate::vocabulary::digest(text.as_bytes())
                    ),
                    media_sha256: media.media_sha256.clone(),
                    audio_stream: self.selected_audio_stream(),
                    precise_timing: false,
                    subtitle_start_ms: position,
                    subtitle_end_ms: position,
                };
                self.invalidate();
                self.user_pause = true;
                self.present_context(context)?;
            }
            "refresh_subtitles" => {
                let paused = self.user_pause && self.engine.flag("pause");
                if let Some(media) = self.media.as_mut() {
                    media.subtitles = media
                        .source_id
                        .as_deref()
                        .map(|id| self.application.store.subtitle_bundle(id))
                        .transpose()?
                        .flatten();
                }
                self.invalidate();
                self.user_pause = paused;
                if paused {
                    self.pending_context = Some(Instant::now() + Duration::from_millis(60));
                }
            }
            "audio_focus" if self.engine.flag("pause") => {
                // Previewing an OCR clip must not discard the paused sentence or its editor.
                self.clock.cancel();
                self.pending_display = false;
                self.state.remaining_seconds = None;
                if self.state.status == "counting" || self.state.status == "awaiting_display" {
                    self.state.status = "cancelled".into();
                }
            }
            "hide" | "audio_focus" => {
                self.engine
                    .command(json!(["set_property", "pause", true]))?;
                self.invalidate();
                self.state.playing = false;
                self.save_progress();
            }
            "close" => {
                self.pending_subtitles = None;
                self.save_progress();
                self.invalidate();
                self.engine.command(json!(["stop"]))?;
                self.load_generation += 1;
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
        self.sync_preferences()?;
        self.refresh_controls();
        if matches!(
            control.action.as_str(),
            "audio"
                | "subtitle"
                | "secondary_subtitle"
                | "subtitle_delay"
                | "subtitle_file"
                | "subtitle_scale"
                | "subtitle_position"
                | "subtitle_visible"
                | "secondary_subtitle_visible"
        ) {
            self.save_progress();
        }
        Ok(())
    }
    fn refresh_paused(&mut self, paused: bool) {
        if paused && !self.engine.flag("eof-reached") {
            self.user_pause = true;
            self.pending_context = Some(Instant::now() + Duration::from_millis(60));
        }
    }
    fn show_menu(&mut self) -> Result<(), AppError> {
        if self.menu_open || !self.state.loaded {
            return Ok(());
        }
        self.clock.cancel();
        self.pending_display = false;
        self.state.remaining_seconds = None;
        self.state.status = "cancelled".into();
        self.refresh_controls();
        let state = self.state.clone();
        let sender = self.sender.clone();
        let host = self.window;
        let fullscreen = self.fullscreen;
        self.menu_open = true;
        if let Err(error) = self.handle.run_on_main_thread(move || {
            let result = menu::popup(host, &state, fullscreen);
            let error = match result {
                Ok(Some(control)) => {
                    let _ = sender.send(Message::MenuAction(state.session_id, control));
                    None
                }
                Ok(None) => None,
                Err(error) => Some(error.message),
            };
            let _ = sender.send(Message::MenuClosed(error));
        }) {
            self.menu_open = false;
            return Err(failure(error.to_string()));
        }
        Ok(())
    }
    fn loaded(&mut self) {
        if self.media.is_none() {
            return;
        }
        self.state.loaded = true;
        self.state.status = "paused".into();
        let _ = self
            .engine
            .command(json!(["set_property", "secondary-sid", "no"]));
        let _ = self.engine.command(json!(["set_property", "sub-delay", 0]));
        if let Some(progress) = &self.restore {
            for path in &progress.external_subtitles {
                if std::path::Path::new(path).is_file() {
                    let _ = self.engine.command(json!(["sub-add", path, "auto"]));
                }
            }
        }
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
            let duration = self.engine.number("duration").unwrap_or(0.0);
            if progress.position > 0.0 && progress.position < (duration - 0.5).max(0.0) {
                let _ = self
                    .engine
                    .command(json!(["seek", progress.position, "absolute+exact"]));
            }
            if let Some(audio) = progress.audio_id {
                let _ = self.engine.command(json!([
                    "set_property",
                    "aid",
                    if audio > 0 { json!(audio) } else { json!("no") }
                ]));
            }
            if let Some(sub) = progress.subtitle_id {
                let _ = self.engine.command(json!([
                    "set_property",
                    "sid",
                    if sub > 0 { json!(sub) } else { json!("no") }
                ]));
            }
            let _ = self.engine.command(json!([
                "set_property",
                "secondary-sid",
                progress
                    .secondary_subtitle_id
                    .filter(|s| *s > 0)
                    .map(|s| json!(s))
                    .unwrap_or(json!("no"))
            ]));
            let _ = self.engine.command(json!([
                "set_property",
                "sub-delay",
                progress.subtitle_delay
            ]));
        }
        self.state.playing = !self.engine.flag("pause");
        self.state.status = if self.state.playing {
            "playing"
        } else {
            "paused"
        }
        .into();
        if let Some((path, subtitles)) = self.pending_subtitles.take()
            && path == self.state.path
            && let Err(error) = self.load_subtitles(subtitles)
        {
            self.state.error = Some(error.message);
        }
        self.progress = self.current_progress();
    }
    fn selected_audio_stream(&self) -> Option<usize> {
        self.engine
            .property("track-list")
            .as_array()
            .and_then(|tracks| {
                tracks
                    .iter()
                    .find(|track| track["type"] == "audio" && track["selected"] == true)
            })
            .and_then(|track| track["ff-index"].as_u64())
            .map(|index| index as usize)
    }

    fn prepare_context(&mut self) -> Result<(), AppError> {
        let Some(media) = &self.media else {
            return Ok(());
        };
        if !self.user_pause || !self.engine.flag("pause") {
            return Ok(());
        }
        let position = self.engine.number("time-pos").unwrap_or(0.0);
        let delay = self.engine.number("sub-delay").unwrap_or(0.0);
        let tracks = self.engine.property("track-list");
        let audio = self.selected_audio_stream();
        let mut context = None;
        for prefix in ["", "secondary-"] {
            let text = self.engine.property(&format!("{prefix}sub-text"));
            let (Some(text), Some(start), Some(end)) = (
                text.as_str(),
                self.engine.number(&format!("{prefix}sub-start")),
                self.engine.number(&format!("{prefix}sub-end")),
            ) else {
                continue;
            };
            let track_delay = self
                .engine
                .number(&format!("{prefix}sub-delay"))
                .unwrap_or(0.0);
            let subtitle_speed = self.engine.number("sub-speed").unwrap_or(1.0);
            let sid = self.engine.property(if prefix.is_empty() {
                "sid"
            } else {
                "secondary-sid"
            });
            let track = tracks
                .as_array()
                .and_then(|t| t.iter().find(|t| t["type"] == "sub" && t["id"] == sid));
            let key = track
                .map(|t| format!("{}:{}", t["ff-index"], t["external-filename"]))
                .unwrap_or_else(|| sid.to_string());
            context = self.application.store.watch_text_context(
                media,
                text,
                ((start * subtitle_speed + track_delay) * 1000.0).round() as i64,
                ((end * subtitle_speed + track_delay) * 1000.0).round() as i64,
                self.state.revision,
                &key,
                audio,
                Some((
                    (start * 1000.0).round() as i64,
                    (end * 1000.0).round() as i64,
                )),
            )?;
            if context.is_some() {
                break;
            }
        }
        if context.is_none() && self.engine.number("sid").map(|n| n as i64) == self.bound_sid {
            context = self.application.store.watch_context(
                media,
                (position * 1000.0) as i64,
                (delay * 1000.0) as i64,
                self.state.revision,
            )?;
        }
        let Some(context) = context else {
            self.capture_ocr_frame()?;
            return Ok(());
        };
        self.present_context(context)
    }

    fn refresh_translation(&mut self) -> Result<(), AppError> {
        if let Some(mut context) = self
            .state
            .context
            .clone()
            .filter(|c| c.text_source == "frame_ocr")
        {
            context.translation = self
                .application
                .store
                .sentence_translation(&context.text)?
                .unwrap_or_default();
            context.translation_revision =
                crate::vocabulary::digest(context.translation.as_bytes());
            self.present_context(context)
        } else {
            self.prepare_context()
        }
    }

    fn capture_ocr_frame(&mut self) -> Result<(), AppError> {
        let path = self.application.store.root().join("jobs").join(format!(
            "watch-{}-{}.png",
            self.state.session_id, self.state.revision
        ));
        let (width, height) = self.engine.save_subtitle_frame(&path)?;
        self.state.frame = Some(WatchFrame {
            path: path.to_string_lossy().into_owned(),
            width,
            height,
            position_ms: (self.engine.number("time-pos").unwrap_or(0.0) * 1000.0).round() as i64,
            text: String::new(),
            busy: false,
            error: None,
        });
        self.recognize_frame(None)
    }

    fn recognize_frame(&mut self, crop: Option<crate::ocr::PixelRect>) -> Result<(), AppError> {
        if !self.user_pause || !self.engine.flag("pause") {
            return Err(failure("请先暂停视频"));
        }
        let frame = self
            .state
            .frame
            .as_mut()
            .ok_or_else(|| failure("暂停帧已失效"))?;
        if frame.busy {
            return Err(failure("正在识别，请稍候"));
        }
        if let Some(rect) = &crop {
            rect.validate(&crate::ocr::ScreenBounds {
                x: 0,
                y: 0,
                width: frame.width,
                height: frame.height,
            })?;
        }
        frame.busy = true;
        frame.error = None;
        let frame_path = std::path::PathBuf::from(&frame.path);
        let program = self.application.settings()?.tools.tesseract;
        let revision = self.state.revision;
        let sender = self.sender.clone();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.ocr_cancel = Some(cancelled.clone());
        self.state.status = "recognizing".into();
        std::thread::spawn(move || {
            let temporary =
                frame_path.with_file_name(format!("watch-crop-{}.png", uuid::Uuid::new_v4()));
            let result = (|| {
                let input = if let Some(rect) = crop {
                    let image = image::open(&frame_path).map_err(|e| failure(e.to_string()))?;
                    image
                        .crop_imm(rect.x, rect.y, rect.width, rect.height)
                        .save(&temporary)
                        .map_err(|e| failure(e.to_string()))?;
                    &temporary
                } else {
                    &frame_path
                };
                crate::ocr::recognize(&program, input, &cancelled)
            })();
            let _ = std::fs::remove_file(temporary);
            let _ = sender.send(Message::Recognized(revision, result));
        });
        Ok(())
    }

    fn present_context(&mut self, context: WatchContext) -> Result<(), AppError> {
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
                Some(mpv::PlayerEvent { id: 6, .. }) => self.begin_file(),
                Some(mpv::PlayerEvent {
                    hook: Some(hook), ..
                }) => self.prepare_file(hook),
                Some(mpv::PlayerEvent { id: 8, .. }) => self.loaded(),
                Some(mpv::PlayerEvent { id: 20, .. }) => {
                    let refresh = std::mem::take(&mut self.refresh_after_seek);
                    self.invalidate();
                    self.state.playing = self.state.loaded && !self.engine.flag("pause");
                    self.refresh_paused(refresh && self.state.loaded && self.engine.flag("pause"));
                }
                Some(mpv::PlayerEvent {
                    id: 16, messages, ..
                }) => {
                    if messages
                        .first()
                        .is_some_and(|name| name == &format!("svl-time:{}", self.state.session_id))
                        && messages.get(1).is_some_and(|event| event == "submit")
                    {
                        let text = messages
                            .get(2)
                            .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
                            .and_then(|v| v.get(0).and_then(|v| v.as_str()).map(String::from));
                        if let Some(text) = text
                            && let Err(error) = self.control(WatchControl {
                                action: "seek_time".into(),
                                text: Some(text),
                                value: None,
                                expected_revision: None,
                            })
                        {
                            let _ = self
                                .engine
                                .command(json!(["show-text", error.message, 3000]));
                            self.state.error = Some(error.message);
                        }
                    }
                }
                Some(mpv::PlayerEvent {
                    id: 7,
                    error,
                    end_reason,
                    ..
                }) => {
                    // END_FILE has already unloaded the old file. Never read the next
                    // file's properties into the previous item's resume record.
                    self.state.loaded = false;
                    if end_reason == Some(0)
                        && let Some(progress) = self.progress.as_mut()
                    {
                        progress.position = self.state.duration;
                    }
                    self.save_progress();
                    self.invalidate();
                    self.state.playing = false;
                    if error < 0 {
                        self.state.error =
                            Some(format!("视频加载失败（mpv {error}），请检查文件是否可用。"));
                    }
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
        let interacting = self.menu_open
            || self.subtitle_dialog_open
            || self.engine.flag("user-data/mpv/console/open");
        if !visible || interacting {
            self.clock.cancel();
            self.pending_display = false;
        }
        if self.pending_context.is_some_and(|at| now >= at) {
            self.pending_context = None;
            if let Err(error) = self.refresh_translation() {
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
            if let Err(error) = self.refresh_translation() {
                self.state.error = Some(error.message);
                self.state.status = "failed".into();
            }
        }
        if visible {
            let force = std::mem::take(&mut self.force_draw);
            match self.engine.draw(self.viewport.0, self.viewport.1, force) {
                Ok(true) if self.pending_display => {
                    self.pending_display = false;
                    if self.user_pause && self.engine.flag("pause") && !interacting {
                        self.clock.arm(self.state.revision, Instant::now());
                        self.state.translation_visible_at =
                            Some(chrono::Utc::now().timestamp_millis());
                        self.state.status = "counting".into();
                    } else if interacting {
                        self.state.status = "cancelled".into();
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
                let path = self.state.path.clone();
                let title = self.state.title.clone();
                std::thread::spawn(move || {
                    let result = (|| {
                        let context = app.store.bind_watch_context(&path, &title, &context)?;
                        if let Some(receipt) = app.store.watch_receipt(&context)? {
                            return Ok(receipt);
                        }
                        let asset = if context.precise_timing {
                            Some(app.store.ensure_clip_from_video(
                                &app.settings()?.tools,
                                &context.source_id,
                                &context.example_id,
                                std::path::Path::new(&path),
                                context.audio_stream,
                                &std::sync::atomic::AtomicBool::new(false),
                            )?)
                        } else {
                            None
                        };
                        app.store
                            .watch_collect(&context, asset.as_ref().map(|a| a.id.as_str()))
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
                    "recognizing" => "正在识别暂停帧字幕…".into(),
                    "ocr_review" => "按 Esc 核对识别原文".into(),
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
            if self.engine.number("playlist-count").unwrap_or(0.0) > 0.0
                && let Err(error) = self.sync_playlist()
            {
                self.state.error = Some(error.message);
            }
            if let Err(error) = self.sync_preferences() {
                self.state.error = Some(error.message);
            }
            if self.state.loaded {
                self.progress = self.current_progress();
            }
            if now.duration_since(self.progress_at) >= Duration::from_secs(5) {
                self.save_progress();
                self.progress_at = now;
            }
            self.state.subtitle_delay = self.engine.number("sub-delay").unwrap_or(0.0);
            self.refresh_controls();
            let primary_sub = self.engine.number("sid").map(|v| v as i64);
            let secondary_sub = self.engine.number("secondary-sid").map(|v| v as i64);
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
                                selected: if t["type"] == "sub" {
                                    t["id"].as_i64() == primary_sub
                                } else {
                                    t["selected"].as_bool().unwrap_or(false)
                                },
                                secondary: t["type"] == "sub" && t["id"].as_i64() == secondary_sub,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.publish();
        }
    }
    fn publish(&mut self) {
        if self.audio_active != self.state.playing {
            self.audio_active = self.state.playing;
            if self.audio_active {
                let _ = self.handle.emit("audio_focus", json!({"owner":"watch"}));
            }
        }
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
    fn sync_preferences(&mut self) -> Result<(), AppError> {
        let old = self.state.playback.preferences.clone();
        let p = &mut self.state.playback.preferences;
        p.speed = self.engine.number("speed").unwrap_or(p.speed);
        p.volume = self.engine.number("volume").unwrap_or(p.volume);
        p.muted = self.engine.flag("mute");
        p.subtitle_scale = self.engine.number("sub-scale").unwrap_or(p.subtitle_scale);
        p.subtitle_position = self.engine.number("sub-pos").unwrap_or(p.subtitle_position);
        p.subtitle_visible = self.engine.flag("sub-visibility");
        p.secondary_subtitle_visible = self.engine.flag("secondary-sub-visibility");
        let active = |v: serde_json::Value| {
            !v.is_null() && v != json!("no") && v != json!(0) && v != json!(false)
        };
        p.loop_mode = if active(self.engine.property("loop-file")) {
            "file"
        } else if active(self.engine.property("loop-playlist")) {
            "playlist"
        } else {
            "none"
        }
        .into();
        if old != *p {
            if old.speed != p.speed {
                let _ = self.engine.command(json!([
                    "show-text",
                    format!("倍速 {:.2}×", p.speed),
                    1500
                ]));
            }
            self.application
                .store
                .save_playback_state(&self.state.playback)?;
        }
        Ok(())
    }
    fn refresh_controls(&mut self) {
        self.state.loop_a = self.engine.number("ab-loop-a");
        self.state.loop_b = self.engine.number("ab-loop-b");
        self.state.chapter_index = self
            .engine
            .number("chapter")
            .filter(|n| *n >= 0.0)
            .map(|n| n as usize);
        self.state.chapters = self
            .engine
            .property("chapter-list")
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, c)| WatchChapter {
                index,
                title: c["title"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| format!("第 {} 章", index + 1)),
                position: c["time"].as_f64().unwrap_or(0.0),
            })
            .collect();
    }
    fn current_progress(&self) -> Option<WatchProgress> {
        if !self.state.loaded
            || self.engine.property("path").as_str() != Some(self.state.path.as_str())
        {
            return None;
        }
        Some(WatchProgress {
            source_id: self.state.source_id.clone(),
            path: self.state.path.clone(),
            position: if self.engine.flag("eof-reached") {
                self.engine
                    .number("duration")
                    .unwrap_or(self.state.duration)
            } else {
                self.engine
                    .number("time-pos")
                    .unwrap_or(self.state.position)
            },
            subtitle_id: Some(self.engine.number("sid").unwrap_or(0.0) as i64),
            audio_id: Some(self.engine.number("aid").unwrap_or(0.0) as i64),
            subtitle_delay: self.engine.number("sub-delay").unwrap_or(0.0),
            secondary_subtitle_id: Some(self.engine.number("secondary-sid").unwrap_or(0.0) as i64),
            external_subtitles: self
                .engine
                .property("track-list")
                .as_array()
                .into_iter()
                .flatten()
                .filter(|t| t["type"] == "sub" && t["external"] == true)
                .filter_map(|t| t["external-filename"].as_str().map(String::from))
                .collect(),
        })
    }
    fn save_progress(&self) {
        if let Some(progress) = self.current_progress().or_else(|| self.progress.clone()) {
            let _ = self.application.store.save_watch_progress(&progress);
        }
    }
}
