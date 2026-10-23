//! Fixed-source Tesseract C API; Android supplies decoded pixels, never a desktop process.
use crate::{
    error::AppError,
    ocr::{PixelRect, ScreenBounds},
};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

unsafe extern "C" {
    fn TessBaseAPICreate() -> *mut c_void;
    fn TessBaseAPIDelete(api: *mut c_void);
    fn TessBaseAPIEnd(api: *mut c_void);
    fn TessBaseAPIInit3(api: *mut c_void, path: *const c_char, language: *const c_char) -> c_int;
    fn TessBaseAPISetPageSegMode(api: *mut c_void, mode: c_int);
    fn TessBaseAPISetImage(
        api: *mut c_void,
        data: *const u8,
        width: c_int,
        height: c_int,
        bytes_per_pixel: c_int,
        bytes_per_line: c_int,
    );
    fn TessBaseAPIRecognize(api: *mut c_void, monitor: *mut c_void) -> c_int;
    fn TessBaseAPIGetUTF8Text(api: *mut c_void) -> *mut c_char;
    fn TessDeleteText(text: *mut c_char);
    fn TessMonitorCreate() -> *mut c_void;
    fn TessMonitorDelete(monitor: *mut c_void);
    fn TessMonitorSetCancelThis(monitor: *mut c_void, data: *mut c_void);
    fn TessMonitorSetCancelFunc(
        monitor: *mut c_void,
        callback: unsafe extern "C" fn(*mut c_void, c_int) -> bool,
    );
}
struct Engine(*mut c_void);
impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            TessBaseAPIEnd(self.0);
            TessBaseAPIDelete(self.0);
        }
    }
}
struct Monitor(*mut c_void);
impl Drop for Monitor {
    fn drop(&mut self) {
        unsafe {
            TessMonitorDelete(self.0);
        }
    }
}
unsafe extern "C" fn cancelled(data: *mut c_void, _words: c_int) -> bool {
    unsafe { &*(data as *const AtomicBool) }.load(Ordering::Relaxed)
}

pub(crate) fn recognize(
    data: &Path,
    image: &Path,
    rect: &PixelRect,
    stop: &AtomicBool,
) -> Result<String, AppError> {
    if stop.load(Ordering::Relaxed) {
        return Err(AppError::new("cancelled", "OCR 已取消。"));
    }
    let image = image::open(image)
        .map_err(|_| AppError::new("invalid_data", "图片无法读取，请重新选择。"))?;
    rect.validate(&ScreenBounds {
        x: 0,
        y: 0,
        width: image.width(),
        height: image.height(),
    })?;
    let pixels = image
        .crop_imm(rect.x, rect.y, rect.width, rect.height)
        .to_luma8();
    let path = CString::new(data.to_string_lossy().as_bytes())
        .map_err(|_| AppError::new("resource_missing", "OCR 英语资源路径无效。"))?;
    unsafe {
        let api = Engine(TessBaseAPICreate());
        if api.0.is_null() {
            std::mem::forget(api);
            return Err(AppError::new("resource_missing", "无法创建离线 OCR 引擎。"));
        }
        if TessBaseAPIInit3(api.0, path.as_ptr(), c"eng".as_ptr()) != 0 {
            return Err(AppError::new(
                "resource_missing",
                "OCR 英语资源不可用，请重新准备应用资源。",
            ));
        }
        TessBaseAPISetPageSegMode(api.0, 6);
        TessBaseAPISetImage(
            api.0,
            pixels.as_raw().as_ptr(),
            pixels.width() as c_int,
            pixels.height() as c_int,
            1,
            pixels.width() as c_int,
        );
        let monitor = Monitor(TessMonitorCreate());
        if monitor.0.is_null() {
            std::mem::forget(monitor);
            return Err(AppError::new("resource_missing", "无法准备 OCR 识别任务。"));
        }
        TessMonitorSetCancelThis(monitor.0, stop as *const AtomicBool as *mut c_void);
        TessMonitorSetCancelFunc(monitor.0, cancelled);
        let result = TessBaseAPIRecognize(api.0, monitor.0);
        if stop.load(Ordering::Relaxed) {
            return Err(AppError::new("cancelled", "OCR 已取消。"));
        }
        if result != 0 {
            return Err(AppError::new(
                "invalid_data",
                "本次 OCR 未完成，请选择清晰的英语区域。",
            ));
        }
        let text = TessBaseAPIGetUTF8Text(api.0);
        if text.is_null() {
            return Err(AppError::new("empty_recognition", "选区没有可用英语文字。"));
        }
        let value = CStr::from_ptr(text).to_string_lossy().into_owned();
        TessDeleteText(text);
        crate::ocr::recognized_text(&value)
    }
}
