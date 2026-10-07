use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND},
        Media::Speech::*,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::{
            Accessibility::*,
            WindowsAndMessaging::{
                GA_ROOT, GetAncestor, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
            },
        },
    },
    core::{Interface, PCWSTR, PWSTR, w},
};

pub fn screen_bounds() -> Result<crate::ocr::ScreenBounds, AppError> {
    use windows::Win32::UI::{HiDpi::*, WindowsAndMessaging::*};
    struct DpiGuard(DPI_AWARENESS_CONTEXT);
    impl Drop for DpiGuard {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.0.is_null() {
        return Err(AppError::new(
            "provider_unavailable",
            "无法取得物理像素坐标，请检查屏幕配置。",
        ));
    }
    let _dpi = DpiGuard(previous);
    let (x, y, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if width <= 0 || height <= 0 || i64::from(width) * i64::from(height) > 100_000_000 {
        return Err(AppError::new("unsupported", "当前屏幕尺寸无法用于截图。"));
    }
    Ok(crate::ocr::ScreenBounds {
        x,
        y,
        width: width as u32,
        height: height as u32,
    })
}

pub fn capture_screen(output: &Path) -> Result<crate::ocr::ScreenBounds, AppError> {
    use windows::Win32::{Graphics::Gdi::*, UI::HiDpi::*};
    struct DpiGuard(DPI_AWARENESS_CONTEXT);
    impl Drop for DpiGuard {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.0.is_null() {
        return Err(AppError::new(
            "provider_unavailable",
            "截图坐标配置不可用。",
        ));
    }
    let _dpi = DpiGuard(previous);
    let bounds = screen_bounds()?;
    struct BitmapGuard {
        screen: HDC,
        memory: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
    }
    impl Drop for BitmapGuard {
        fn drop(&mut self) {
            unsafe {
                if !self.previous.0.is_null() {
                    SelectObject(self.memory, self.previous);
                }
                if !self.bitmap.0.is_null() {
                    let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
                }
                if !self.memory.0.is_null() {
                    let _ = DeleteDC(self.memory);
                }
                if !self.screen.0.is_null() {
                    ReleaseDC(None, self.screen);
                }
            }
        }
    }
    let rgba = unsafe {
        let mut guard = BitmapGuard {
            screen: GetDC(None),
            memory: HDC::default(),
            bitmap: HBITMAP::default(),
            previous: HGDIOBJ::default(),
        };
        if guard.screen.0.is_null() {
            return Err(AppError::new("provider_unavailable", "无法读取当前屏幕。"));
        }
        guard.memory = CreateCompatibleDC(Some(guard.screen));
        if guard.memory.0.is_null() {
            return Err(AppError::new(
                "provider_unavailable",
                "无法创建截图缓冲区。",
            ));
        }
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = bounds.width as i32;
        info.bmiHeader.biHeight = -(bounds.height as i32);
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut pixels = std::ptr::null_mut();
        guard.bitmap = CreateDIBSection(
            Some(guard.screen),
            &info,
            DIB_RGB_COLORS,
            &mut pixels,
            None,
            0,
        )
        .map_err(native_error)?;
        guard.previous = SelectObject(guard.memory, HGDIOBJ(guard.bitmap.0));
        if pixels.is_null() || guard.previous.0.is_null() {
            return Err(AppError::new(
                "provider_unavailable",
                "无法访问截图缓冲区。",
            ));
        }
        BitBlt(
            guard.memory,
            0,
            0,
            bounds.width as i32,
            bounds.height as i32,
            Some(guard.screen),
            bounds.x,
            bounds.y,
            SRCCOPY | CAPTUREBLT,
        )
        .map_err(native_error)?;
        if !GdiFlush().as_bool() {
            return Err(AppError::new("provider_unavailable", "本次截图未完成。"));
        }
        let mut data = std::slice::from_raw_parts(
            pixels.cast::<u8>(),
            bounds.width as usize * bounds.height as usize * 4,
        )
        .to_vec();
        for pixel in data.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        data
    };
    image::RgbaImage::from_raw(bounds.width, bounds.height, rgba)
        .ok_or_else(|| AppError::new("invalid_data", "截图尺寸无效。"))?
        .save(output)
        .map_err(|error| AppError::new("io_error", format!("无法保存本次截图：{error}")))?;
    Ok(bounds)
}

pub fn fit_capture_window(
    handle: isize,
    bounds: &crate::ocr::ScreenBounds,
) -> Result<(), AppError> {
    use windows::Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        UI::{
            HiDpi::*,
            WindowsAndMessaging::{
                GetClientRect, GetWindowRect, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
            },
        },
    };
    struct DpiGuard(DPI_AWARENESS_CONTEXT);
    impl Drop for DpiGuard {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.0.is_null() {
        return Err(AppError::new(
            "provider_unavailable",
            "选区窗口像素配置不可用。",
        ));
    }
    let _dpi = DpiGuard(previous);
    let window = HWND(handle as *mut _);
    unsafe {
        let mut client = RECT::default();
        let mut outer = RECT::default();
        let mut origin = POINT::default();
        GetClientRect(window, &mut client).map_err(native_error)?;
        GetWindowRect(window, &mut outer).map_err(native_error)?;
        if !ClientToScreen(window, &mut origin).as_bool() {
            return Err(AppError::new("provider_unavailable", "无法定位选区窗口。"));
        }
        let border_x = (outer.right - outer.left) - (client.right - client.left);
        let border_y = (outer.bottom - outer.top) - (client.bottom - client.top);
        SetWindowPos(
            window,
            None,
            bounds.x - (origin.x - outer.left),
            bounds.y - (origin.y - outer.top),
            bounds.width as i32 + border_x,
            bounds.height as i32 + border_y,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
        .map_err(native_error)?;
        GetClientRect(window, &mut client).map_err(native_error)?;
        let mut origin = POINT::default();
        if !ClientToScreen(window, &mut origin).as_bool()
            || origin.x != bounds.x
            || origin.y != bounds.y
            || client.right - client.left != bounds.width as i32
            || client.bottom - client.top != bounds.height as i32
        {
            return Err(AppError::new(
                "provider_unavailable",
                "选区窗口未覆盖完整屏幕，请重新截图。",
            ));
        }
    }
    Ok(())
}

fn native_error(error: windows::core::Error) -> AppError {
    AppError::new(
        "provider_unavailable",
        format!("Windows 本地能力暂时不可用：{error}"),
    )
}

struct ComApartment;
impl ComApartment {
    fn initialize() -> Result<Self, AppError> {
        // These functions run on a worker, never on the captured application's UI thread.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(native_error)?;
        }
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
unsafe fn owned_string(value: PWSTR) -> String {
    let result = unsafe { value.to_string().unwrap_or_default() };
    unsafe {
        CoTaskMemFree(Some(value.0.cast()));
    }
    result
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemVoice {
    pub id: String,
    pub name: String,
}

fn english_tokens() -> Result<Vec<(SystemVoice, ISpObjectToken)>, AppError> {
    unsafe {
        let category: ISpObjectTokenCategory =
            CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_INPROC_SERVER)
                .map_err(native_error)?;
        category.SetId(SPCAT_VOICES, false).map_err(native_error)?;
        let tokens = category
            .EnumTokens(PCWSTR::null(), PCWSTR::null())
            .map_err(native_error)?;
        let mut count = 0;
        tokens.GetCount(&mut count).map_err(native_error)?;
        let mut result = Vec::new();
        for index in 0..count {
            let token = tokens.Item(index).map_err(native_error)?;
            let attributes = token.OpenKey(w!("Attributes")).map_err(native_error)?;
            let languages = owned_string(
                attributes
                    .GetStringValue(w!("Language"))
                    .map_err(native_error)?,
            );
            if languages.split(';').any(|language| {
                u32::from_str_radix(language.trim(), 16).is_ok_and(|value| value & 0x3ff == 9)
            }) {
                let id = owned_string(token.GetId().map_err(native_error)?);
                let name =
                    owned_string(token.GetStringValue(PCWSTR::null()).map_err(native_error)?);
                result.push((SystemVoice { id, name }, token));
            }
        }
        Ok(result)
    }
}

pub fn system_voices() -> Result<Vec<SystemVoice>, AppError> {
    let _apartment = ComApartment::initialize()?;
    Ok(english_tokens()?
        .into_iter()
        .map(|(voice, _)| voice)
        .collect())
}

pub fn synthesize(
    text: &str,
    voice_id: &str,
    output: &Path,
    cancelled: &AtomicBool,
) -> Result<SystemVoice, AppError> {
    if text.trim().is_empty() || text.chars().count() > 20000 || text.contains('\0') {
        return Err(AppError::new(
            "invalid_input",
            "请提供非空、长度合适的英语文本。",
        ));
    }
    let _apartment = ComApartment::initialize()?;
    let (selected, token) = english_tokens()?
        .into_iter()
        .find(|(voice, _)| voice_id.is_empty() || voice.id == voice_id)
        .ok_or_else(|| {
            AppError::new(
                "resource_missing",
                "未发现所选的 Windows 本地英语声音，可重新选择声音或使用已保存原声。",
            )
        })?;
    let output = wide(&output.to_string_lossy());
    let text = wide(text);
    unsafe {
        let voice: ISpVoice =
            CoCreateInstance(&SpVoice, None, CLSCTX_INPROC_SERVER).map_err(native_error)?;
        let stream: ISpStream =
            CoCreateInstance(&SpStream, None, CLSCTX_INPROC_SERVER).map_err(native_error)?;
        let format = windows::Win32::Media::Audio::WAVEFORMATEX {
            wFormatTag: 1,
            nChannels: 1,
            nSamplesPerSec: 22050,
            nAvgBytesPerSec: 44100,
            nBlockAlign: 2,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let wave_format = windows::core::GUID::from_u128(0xc31adbae_527f_4ff5_a230_f62bb61ff70c);
        stream
            .BindToFile(
                PCWSTR(output.as_ptr()),
                SPFM_CREATE_ALWAYS,
                Some(&wave_format),
                Some(&format),
                0,
            )
            .map_err(native_error)?;
        let result = (|| {
            voice.SetVoice(&token).map_err(native_error)?;
            voice.SetOutput(&stream, false).map_err(native_error)?;
            voice
                .Speak(
                    PCWSTR(text.as_ptr()),
                    (SPF_ASYNC.0 | SPF_IS_NOT_XML.0) as u32,
                    None,
                )
                .map_err(native_error)?;
            let started = Instant::now();
            loop {
                if cancelled.load(Ordering::Relaxed) {
                    voice
                        .Speak(PCWSTR::null(), SPF_PURGEBEFORESPEAK.0 as u32, None)
                        .map_err(native_error)?;
                    return Err(AppError::new("cancelled", "系统语音准备已取消。"));
                }
                // Preserve S_FALSE (timeout); the generated Result<()> wrapper erases it.
                let state =
                    (Interface::vtable(&voice).WaitUntilDone)(Interface::as_raw(&voice), 15);
                if state.0 == 0 {
                    break;
                }
                state.ok().map_err(native_error)?;
                if started.elapsed() > Duration::from_secs(60) {
                    let _ = voice.Speak(PCWSTR::null(), SPF_PURGEBEFORESPEAK.0 as u32, None);
                    return Err(AppError::new(
                        "provider_unavailable",
                        "系统语音准备超时，可以重试或缩短文本。",
                    ));
                }
            }
            Ok(())
        })();
        stream.Close().map_err(native_error)?;
        result?;
    }
    Ok(selected)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureOrigin {
    pub window: isize,
    pub process_id: u32,
    pub application: String,
    pub title: String,
}

pub fn foreground_origin() -> Result<CaptureOrigin, AppError> {
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return Err(AppError::new("not_found", "当前没有可采集的前台窗口。"));
        }
        let mut process_id = 0;
        GetWindowThreadProcessId(window, Some(&mut process_id));
        let mut title = vec![0u16; 512];
        let length = GetWindowTextW(window, &mut title);
        let title = String::from_utf16_lossy(&title[..length.max(0) as usize]);
        let application = if let Ok(process) =
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
        {
            let mut path = vec![0u16; 32768];
            let mut length = path.len() as u32;
            let result = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut length,
            );
            let _ = CloseHandle(process);
            if result.is_ok() {
                Path::new(&String::from_utf16_lossy(&path[..length as usize]))
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                "Windows 应用".into()
            }
        } else {
            "Windows 应用".into()
        };
        Ok(CaptureOrigin {
            window: window.0 as isize,
            process_id,
            application,
            title,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedText {
    pub text: String,
    pub context: String,
    pub origin: CaptureOrigin,
}

pub fn selected_text(origin: &CaptureOrigin) -> Result<SelectedText, AppError> {
    let _apartment = ComApartment::initialize()?;
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(native_error)?;
        if let Ok(options) = automation.cast::<IUIAutomation2>() {
            let _ = options.SetConnectionTimeout(2000);
            let _ = options.SetTransactionTimeout(2000);
        }
        let element = automation.GetFocusedElement().map_err(native_error)?;
        let walker = automation.ControlViewWalker().map_err(native_error)?;
        let mut ancestor = element.clone();
        let mut belongs = false;
        for _ in 0..32 {
            if let Ok(window) = ancestor.CurrentNativeWindowHandle()
                && !window.0.is_null()
                && GetAncestor(window, GA_ROOT) == HWND(origin.window as *mut _)
            {
                belongs = true;
                break;
            }
            match walker.GetParentElement(&ancestor) {
                Ok(parent) => ancestor = parent,
                Err(_) => break,
            }
        }
        if GetForegroundWindow() != HWND(origin.window as *mut _) || !belongs {
            return Err(AppError::new(
                "capture_changed",
                "前台窗口已变化，请在原应用重新选择文字。",
            ));
        }
        if element.CurrentIsPassword().map_err(native_error)?.as_bool() {
            return Err(AppError::new("unsupported", "此输入区域不支持划词采集。"));
        }
        let mut candidate = element;
        let mut found = None;
        for _ in 0..16 {
            if let Ok(pattern) =
                candidate.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            {
                found = Some(pattern);
                break;
            }
            if let Ok(window) = candidate.CurrentNativeWindowHandle()
                && window == HWND(origin.window as *mut _)
            {
                break;
            }
            match walker.GetParentElement(&candidate) {
                Ok(parent) => candidate = parent,
                Err(_) => break,
            }
        }
        let pattern = found.ok_or_else(|| {
            AppError::new(
                "unsupported",
                "此应用未提供可用的文字选区，可以使用截图 OCR 或手动收录。",
            )
        })?;
        let ranges = pattern.GetSelection().map_err(native_error)?;
        let mut selected = Vec::new();
        let mut contexts = Vec::new();
        for index in 0..ranges.Length().map_err(native_error)?.min(8) {
            let range = ranges.GetElement(index).map_err(native_error)?;
            let text = range.GetText(4001).map_err(native_error)?.to_string();
            if !text.trim().is_empty() {
                if text.chars().count() > 4000 {
                    return Err(AppError::new("invalid_input", "选中文字过长，请缩小选区。"));
                }
                selected.push(text.trim().to_owned());
                let context = range.Clone().map_err(native_error)?;
                if context.ExpandToEnclosingUnit(TextUnit_Line).is_ok() {
                    contexts.push(context.GetText(20000).map_err(native_error)?.to_string());
                }
            }
        }
        if selected.is_empty() {
            return Err(AppError::new(
                "empty_selection",
                "没有选中文字。本次未使用剪贴板旧内容，请先选择后重试。",
            ));
        }
        if GetForegroundWindow() != HWND(origin.window as *mut _) {
            return Err(AppError::new(
                "capture_changed",
                "采集期间窗口已变化，请重试。",
            ));
        }
        let text = selected.join("\n");
        if text.chars().count() > 4000 {
            return Err(AppError::new("invalid_input", "选中文字过长，请缩小选区。"));
        }
        let context = contexts.join("\n").trim().to_owned();
        let context = if context.is_empty() {
            text.clone()
        } else {
            context
        };
        Ok(SelectedText {
            text,
            context,
            origin: origin.clone(),
        })
    }
}
