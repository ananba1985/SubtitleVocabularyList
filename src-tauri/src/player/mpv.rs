//! libmpv client API with mpv-owned native Windows video output and rendering threads.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::error::AppError;
use libloading::Library;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    ptr,
    time::{Duration, Instant},
};
use windows::Win32::{Foundation::HWND, Graphics::Dwm::DwmFlush};

#[repr(C)]
pub struct Event {
    pub id: i32,
    pub error: i32,
    pub userdata: u64,
    pub data: *mut c_void,
}
#[repr(C)]
struct Hook {
    name: *const c_char,
    id: u64,
}
#[repr(C)]
struct ClientMessage {
    count: i32,
    args: *const *const c_char,
}
pub struct PlayerEvent {
    pub id: i32,
    pub hook: Option<u64>,
    pub error: i32,
    pub end_reason: Option<i32>,
    pub messages: Vec<String>,
}
#[repr(C)]
union NodeValue {
    string: *mut c_char,
    flag: i32,
    integer: i64,
    double: f64,
    list: *mut NodeList,
    bytes: *mut ByteArray,
}
#[repr(C)]
struct ByteArray {
    data: *mut u8,
    size: usize,
}
struct Screenshot {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}
#[repr(C)]
struct Node {
    value: NodeValue,
    format: i32,
}
#[repr(C)]
struct NodeList {
    num: i32,
    values: *mut Node,
    keys: *mut *mut c_char,
}
unsafe fn node_value(node: &Node) -> Value {
    match node.format {
        1 => Value::String(
            CStr::from_ptr(node.value.string)
                .to_string_lossy()
                .into_owned(),
        ),
        3 => json!(node.value.flag != 0),
        4 => json!(node.value.integer),
        5 => json!(node.value.double),
        7 | 8 => {
            let list = &*node.value.list;
            let mut map = serde_json::Map::new();
            let mut array = vec![];
            for index in 0..list.num.max(0) as usize {
                let value = node_value(&*list.values.add(index));
                if node.format == 8 {
                    map.insert(
                        CStr::from_ptr(*list.keys.add(index))
                            .to_string_lossy()
                            .into_owned(),
                        value,
                    );
                } else {
                    array.push(value);
                }
            }
            if node.format == 8 {
                Value::Object(map)
            } else {
                Value::Array(array)
            }
        }
        _ => Value::Null,
    }
}

pub struct Engine {
    _library: Library,
    handle: *mut c_void,
    pending_display: Cell<Option<Instant>>,
    command_ret: unsafe extern "C" fn(*mut c_void, *const *const c_char, *mut Node) -> i32,
    get: unsafe extern "C" fn(*mut c_void, *const c_char, i32, *mut c_void) -> i32,
    wait: unsafe extern "C" fn(*mut c_void, f64) -> *const Event,
    free_node: unsafe extern "C" fn(*mut Node),
    destroy: unsafe extern "C" fn(*mut c_void),
    hook_continue: unsafe extern "C" fn(*mut c_void, u64) -> i32,
    client_name: unsafe extern "C" fn(*mut c_void) -> *const c_char,
}

fn error(detail: impl std::fmt::Display) -> AppError {
    AppError::new("player_unavailable", format!("内嵌播放器：{detail}"))
}

impl Engine {
    pub fn new(window: HWND, dll: &Path, cache: &Path) -> Result<Self, AppError> {
        unsafe {
            let library = Library::new(dll).map_err(error)?;
            macro_rules! sym {
                ($name:literal,$ty:ty) => {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(error)?
                };
            }
            let create = sym!("mpv_create", unsafe extern "C" fn() -> *mut c_void);
            let option = sym!(
                "mpv_set_option_string",
                unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> i32
            );
            let initialize = sym!("mpv_initialize", unsafe extern "C" fn(*mut c_void) -> i32);
            let destroy = sym!("mpv_terminate_destroy", unsafe extern "C" fn(*mut c_void));
            let handle = create();
            if handle.is_null() {
                return Err(error("无法创建播放会话"));
            }
            let prepare = (|| {
                std::fs::create_dir_all(cache)?;
                let parent = (window.0 as usize).to_string();
                for (key, value) in [
                    ("vo", "gpu-next,gpu"),
                    ("gpu-api", "d3d11"),
                    ("wid", parent.as_str()),
                    ("config", "no"),
                    ("load-scripts", "no"),
                    ("osc", "yes"),
                    (
                        "script-opts",
                        concat!(
                            "osc-showwindowed=yes,osc-showfullscreen=yes,osc-boxvideo=no,",
                            "osc-windowcontrols=no,osc-idlescreen=no,",
                            "osc-custom_button_1_content=■,",
                            "osc-custom_button_1_mbtn_left_command=script-message svl-stop"
                        ),
                    ),
                    ("osd-align-x", "right"),
                    ("osd-align-y", "top"),
                    ("input-default-bindings", "yes"),
                    ("input-vo-keyboard", "no"),
                    ("terminal", "no"),
                    ("idle", "yes"),
                    ("keep-open", "yes"),
                    ("pause", "yes"),
                    ("hwdec", "auto-safe"),
                    ("osd-level", "0"),
                    ("sub-font-size", "30"),
                    ("screenshot-directory", cache.to_str().unwrap_or("")),
                    ("gpu-shader-cache-dir", cache.to_str().unwrap_or("")),
                ] {
                    let k = CString::new(key).unwrap();
                    let v = CString::new(value).map_err(error)?;
                    let result = option(handle, k.as_ptr(), v.as_ptr());
                    if result < 0 {
                        return Err(error(format!("参数 {key} 不可用（{result}）")));
                    }
                }
                if initialize(handle) < 0 {
                    return Err(error("初始化失败"));
                }
                let hook_add = sym!(
                    "mpv_hook_add",
                    unsafe extern "C" fn(*mut c_void, u64, *const c_char, i32) -> i32
                );
                if hook_add(handle, 0, c"on_load".as_ptr(), 0) < 0 {
                    return Err(error("无法连接换集事件"));
                }
                Ok(())
            })();
            if let Err(e) = prepare {
                destroy(handle);
                return Err(e);
            }
            Ok(Self {
                command_ret: sym!(
                    "mpv_command_ret",
                    unsafe extern "C" fn(*mut c_void, *const *const c_char, *mut Node) -> i32
                ),
                get: sym!(
                    "mpv_get_property",
                    unsafe extern "C" fn(*mut c_void, *const c_char, i32, *mut c_void) -> i32
                ),
                wait: sym!(
                    "mpv_wait_event",
                    unsafe extern "C" fn(*mut c_void, f64) -> *const Event
                ),
                free_node: sym!("mpv_free_node_contents", unsafe extern "C" fn(*mut Node)),
                hook_continue: sym!(
                    "mpv_hook_continue",
                    unsafe extern "C" fn(*mut c_void, u64) -> i32
                ),
                client_name: sym!(
                    "mpv_client_name",
                    unsafe extern "C" fn(*mut c_void) -> *const c_char
                ),
                _library: library,
                handle,
                destroy,
                pending_display: Cell::new(None),
            })
        }
    }
    pub fn command(&self, args: Value) -> Result<Value, AppError> {
        self.with_command(args, |node| unsafe { Ok(node_value(node)) })
    }
    fn with_command<T>(
        &self,
        args: Value,
        read: impl FnOnce(&Node) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        unsafe {
            let args = if args.is_object() && args["name"] == "osd-overlay" {
                if args["format"] == "none" {
                    json!(["osd-overlay", args["id"], "none", ""])
                } else {
                    json!([
                        "osd-overlay",
                        args["id"],
                        args["format"],
                        args["data"],
                        args["res_x"],
                        args["res_y"]
                    ])
                }
            } else {
                args
            };
            let mut args = args;
            // set_property is a JSON IPC operation; the C command interface calls it set.
            if args[0] == "set_property" {
                args[0] = "set".into();
            }
            let values = args.as_array().ok_or_else(|| error("播放命令格式无效"))?;
            let strings = values
                .iter()
                .map(|value| {
                    CString::new(match value {
                        Value::String(s) => s.clone(),
                        Value::Bool(b) => {
                            if *b {
                                "yes".into()
                            } else {
                                "no".into()
                            }
                        }
                        _ => value.to_string(),
                    })
                    .map_err(error)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut argv = strings.iter().map(|s| s.as_ptr()).collect::<Vec<_>>();
            argv.push(ptr::null());
            let mut node = Node {
                value: NodeValue { integer: 0 },
                format: 0,
            };
            let result = (self.command_ret)(self.handle, argv.as_ptr(), &mut node);
            if result < 0 {
                return Err(error(format!(
                    "播放命令 {} 失败（{result}）",
                    strings
                        .first()
                        .map(|s| s.to_string_lossy())
                        .unwrap_or_default()
                )));
            }
            let value = read(&node);
            (self.free_node)(&mut node);
            value
        }
    }
    fn screenshot(&self, mode: &str) -> Result<Screenshot, AppError> {
        self.with_command(json!(["screenshot-raw", mode, "rgba"]), |node| unsafe {
            if node.format != 8 {
                return Err(error("暂停帧格式不可用"));
            }
            let map = &*node.value.list;
            let field = |name: &str| -> Option<&Node> {
                (0..map.num.max(0) as usize).find_map(|i| {
                    (CStr::from_ptr(*map.keys.add(i)).to_bytes() == name.as_bytes())
                        .then(|| &*map.values.add(i))
                })
            };
            let integer = |key| {
                field(key)
                    .filter(|n| n.format == 4)
                    .map(|n| n.value.integer)
            };
            let width = integer("w").unwrap_or(0).max(0) as usize;
            let height = integer("h").unwrap_or(0).max(0) as usize;
            let stride = integer("stride").unwrap_or(0) as isize;
            let bytes = field("data")
                .filter(|n| n.format == 9)
                .ok_or_else(|| error("暂停帧像素不可用"))?;
            let bytes = &*bytes.value.bytes;
            if width == 0
                || height == 0
                || width
                    .checked_mul(height)
                    .and_then(|n| n.checked_mul(4))
                    .is_none_or(|n| n > 256 * 1024 * 1024)
                || stride.unsigned_abs() < width * 4
                || stride
                    .unsigned_abs()
                    .saturating_mul(height - 1)
                    .saturating_add(width * 4)
                    > bytes.size
                || bytes.data.is_null()
            {
                return Err(error("暂停帧尺寸不可用"));
            }
            let mut pixels = vec![0; width * height * 4];
            for y in 0..height {
                ptr::copy_nonoverlapping(
                    bytes.data.offset(y as isize * stride),
                    pixels.as_mut_ptr().add(y * width * 4),
                    width * 4,
                );
            }
            Ok(Screenshot {
                width,
                height,
                pixels,
            })
        })
    }
    pub fn save_subtitle_frame(&self, path: &Path) -> Result<(u32, u32), AppError> {
        // Includes the player's subtitles, excludes this app's translation/OSC OSD.
        let shot = self.screenshot("subtitles")?;
        let (width, height) = (shot.width as u32, shot.height as u32);
        let image = image::RgbaImage::from_raw(width, height, shot.pixels)
            .ok_or_else(|| error("暂停帧像素无效"))?;
        image
            .save(path)
            .map_err(|e| error(format!("无法保存暂停帧：{e}")))?;
        Ok((width, height))
    }
    // Compare the same frozen frame with/without the player's own subtitle layer.
    // This locates rendered ASS/PGS text even when it follows a character, without
    // changing subtitle visibility on screen or guessing positions from plain text.
    fn subtitle_bounds(&self, width: i32, height: i32) -> Option<[f64; 4]> {
        let plain = self.screenshot("video").ok()?;
        let caption = self.screenshot("subtitles").ok()?;
        if plain.width != caption.width || plain.height != caption.height {
            return None;
        }
        let (mut left, mut top, mut right, mut bottom) = (plain.width, plain.height, 0, 0);
        let mut count = 0;
        for (i, (a, b)) in plain
            .pixels
            .chunks_exact(4)
            .zip(caption.pixels.chunks_exact(4))
            .enumerate()
        {
            if (0..3).map(|c| a[c].abs_diff(b[c]) as u32).sum::<u32>() > 48 {
                let (x, y) = (i % plain.width, i / plain.width);
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
                count += 1;
            }
        }
        if count < 8 {
            return None;
        }
        let aspect = self
            .number("video-out-params/dw")
            .zip(self.number("video-out-params/dh"))
            .filter(|(w, h)| *w > 0.0 && *h > 0.0)
            .map(|(w, h)| w / h)
            .unwrap_or(plain.width as f64 / plain.height as f64);
        let display_width = (width as f64).min(height as f64 * aspect);
        let display_height = display_width / aspect;
        let (x, y) = (
            (width as f64 - display_width) / 2.0,
            (height as f64 - display_height) / 2.0,
        );
        Some([
            x + left as f64 / plain.width as f64 * display_width,
            y + top as f64 / plain.height as f64 * display_height,
            x + (right + 1) as f64 / plain.width as f64 * display_width,
            y + (bottom + 1) as f64 / plain.height as f64 * display_height,
        ])
    }
    pub fn number(&self, name: &str) -> Option<f64> {
        unsafe {
            let key = CString::new(name).ok()?;
            let mut value = 0.0;
            ((self.get)(
                self.handle,
                key.as_ptr(),
                5,
                (&mut value as *mut f64).cast(),
            ) >= 0)
                .then_some(value)
        }
    }
    pub fn flag(&self, name: &str) -> bool {
        unsafe {
            let key = CString::new(name).unwrap();
            let mut value = 0i32;
            (self.get)(
                self.handle,
                key.as_ptr(),
                3,
                (&mut value as *mut i32).cast(),
            ) >= 0
                && value != 0
        }
    }
    pub fn property(&self, name: &str) -> Value {
        unsafe {
            let Ok(key) = CString::new(name) else {
                return Value::Null;
            };
            let mut node = Node {
                value: NodeValue { integer: 0 },
                format: 0,
            };
            if (self.get)(
                self.handle,
                key.as_ptr(),
                6,
                (&mut node as *mut Node).cast(),
            ) < 0
            {
                return Value::Null;
            }
            let value = node_value(&node);
            (self.free_node)(&mut node);
            value
        }
    }
    pub fn event(&self) -> Option<PlayerEvent> {
        unsafe {
            let e = (self.wait)(self.handle, 0.0);
            (!e.is_null() && (*e).id != 0).then(|| PlayerEvent {
                id: (*e).id,
                hook: ((*e).id == 25 && !(*e).data.is_null())
                    .then(|| (*((*e).data as *const Hook)).id),
                error: if (*e).id == 7 && !(*e).data.is_null() {
                    *((*e).data.cast::<i32>().add(1))
                } else {
                    (*e).error
                },
                end_reason: ((*e).id == 7 && !(*e).data.is_null())
                    .then(|| *((*e).data.cast::<i32>())),
                messages: if (*e).id == 16 && !(*e).data.is_null() {
                    let message = &*((*e).data as *const ClientMessage);
                    (0..message.count.max(0) as usize)
                        .map(|i| {
                            CStr::from_ptr(*message.args.add(i))
                                .to_string_lossy()
                                .into_owned()
                        })
                        .collect()
                } else {
                    vec![]
                },
            })
        }
    }
    pub fn continue_load(&self, id: u64) -> Result<(), AppError> {
        let result = unsafe { (self.hook_continue)(self.handle, id) };
        if result < 0 {
            Err(error("无法继续加载视频"))
        } else {
            Ok(())
        }
    }
    pub fn time_prompt(&self, session: &str, position: f64) -> Result<(), AppError> {
        let name = unsafe { CStr::from_ptr((self.client_name)(self.handle)) }.to_string_lossy();
        let args = json!({"client_name":name,"handler_id":format!("svl-time:{session}"),"prompt":"跳转时间（秒 / 分:秒 / 时:分:秒）", "default_text":format!("{position:.1}"), "id":"svl-time"});
        self.command(json!([
            "script-message-to",
            "console",
            "get-input",
            args.to_string()
        ]))?;
        Ok(())
    }
    pub fn translation(&self, text: &str, width: i32, height: i32) -> Result<(), AppError> {
        if text.is_empty() {
            self.pending_display.set(None);
            self.command(json!({"name":"osd-overlay","id":11,"format":"none","data":""}))?;
            return Ok(());
        }
        let escaped = self
            .command(json!(["escape-ass", text]))?
            .as_str()
            .unwrap_or(text)
            .to_owned();
        let detected = self.subtitle_bounds(width, height);
        let bounds = detected.unwrap_or([
            width as f64 * 0.25,
            height as f64 * 0.82,
            width as f64 * 0.75,
            height as f64 * 0.9,
        ]);
        let line_count = self
            .property("sub-text")
            .as_str()
            .map(|s| s.lines().count().max(1))
            .unwrap_or(1);
        let font = detected
            .map(|b| (b[3] - b[1]) / line_count as f64 * 0.9)
            .unwrap_or(height as f64 * 0.046)
            .clamp(18.0, 48.0);
        let line_width = ((width as f64 - 32.0) / font).floor().max(1.0) as usize;
        let lines = text.chars().count().div_ceil(line_width).max(1);
        let translation_height = font * 1.35 * lines as f64;
        let gap = (font * 0.25).max(6.0);
        let below = bounds[3] + gap + translation_height < height as f64 - 8.0;
        let y = if below {
            bounds[3] + gap
        } else {
            (bounds[1] - gap).max(translation_height + 8.0)
        };
        let half_width = (text.chars().count().min(line_width) as f64 * font / 2.0)
            .min((width as f64 - 16.0) / 2.0);
        let x = ((bounds[0] + bounds[2]) / 2.0).clamp(
            half_width + 8.0,
            (width as f64 - half_width - 8.0).max(half_width + 8.0),
        );
        let data = format!(
            "{{\\an{}\\pos({x},{y})\\fs{font}\\bord2\\shad1\\c&HFFFFFF&}}{}",
            if below { 8 } else { 2 },
            escaped
        );
        self.pending_display.set(None);
        self.command(json!({"name":"osd-overlay","id":11,"format":"ass-events","data":data,"res_x":width,"res_y":height}))?;
        self.pending_display.set(Some(Instant::now()));
        Ok(())
    }
    #[cfg(debug_assertions)]
    pub fn capture(&self, path: &Path) -> Result<(), AppError> {
        // Ask mpv's native VO for its composite, including both subtitles and OSD.
        // This is rendering evidence, not proof of the physical window's presentation.
        self.command(json!(["screenshot-to-file", path, "window"]))?;
        Ok(())
    }
    /// Normal playback is completely owned by mpv's native VO thread.
    /// Only a paused translation needs a one-shot output/compositor fence before timing.
    pub fn translation_presented(&self, width: i32, height: i32) -> Result<bool, AppError> {
        let Some(since) = self.pending_display.get() else {
            return Ok(false);
        };
        if width <= 0 || height <= 0 || since.elapsed() < Duration::from_millis(50) {
            return Ok(false);
        }
        if !self.flag("pause") {
            self.pending_display.set(None);
            return Ok(false);
        }
        // Native screenshot commands wait for the VO's pending frame and execute
        // on its rendering thread. This is only once per paused overlay, never per frame.
        let _frame = self.screenshot("window")?;
        unsafe { DwmFlush().map_err(error)? };
        self.pending_display.set(None);
        Ok(true)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.handle);
        }
    }
}
