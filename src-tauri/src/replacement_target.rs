//! Native editing target. All COM objects stay on the operation's worker thread.
use crate::{error::AppError, windows_native::CaptureOrigin};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
        System::{Com::*, DataExchange::*, Memory::*},
        UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{Interface, Ref, implement},
};

fn native(error: windows::core::Error) -> AppError {
    AppError::new(
        "replacement_unavailable",
        format!("此输入区域暂不能自动替换：{error}"),
    )
}
fn unsupported() -> AppError {
    AppError::new(
        "unsupported",
        "此输入区域未提供可验证的可编辑单一选区，请使用输入翻译。",
    )
}
fn changed() -> AppError {
    AppError::new(
        "target_changed",
        "原窗口、输入内容或选区已改变，本次只保留译文。",
    )
}

pub struct Apartment;
impl Apartment {
    pub fn new() -> Result<Self, AppError> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(native)?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

#[implement(IUIAutomationEventHandler, IUIAutomationFocusChangedEventHandler)]
struct TargetEvents {
    changed: Arc<AtomicBool>,
    armed: Arc<AtomicBool>,
    focused: windows_core::AgileReference<IUIAutomationElement>,
    range: windows_core::AgileReference<IUIAutomationTextRange>,
    before: String,
}
impl TargetEvents_Impl {
    fn invalidate(&self) {
        if self.armed.load(Ordering::Acquire) {
            self.changed.store(true, Ordering::Release);
        }
    }
}
impl IUIAutomationEventHandler_Impl for TargetEvents_Impl {
    fn HandleAutomationEvent(
        &self,
        _: Ref<IUIAutomationElement>,
        event: UIA_EVENT_ID,
    ) -> windows::core::Result<()> {
        // Providers may emit subscription/initialization notifications without
        // an edit. Verify that the announced selection actually differs.
        let unchanged = (|| unsafe {
            let focused = self.focused.resolve()?;
            if event == UIA_Text_TextChangedEventId {
                return Ok::<_, windows::core::Error>(
                    focused
                        .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)?
                        .CurrentValue()?
                        .to_string()
                        == self.before,
                );
            }
            let pattern =
                focused.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)?;
            let ranges = pattern.GetSelection()?;
            if ranges.Length()? != 1 {
                return Ok(false);
            }
            let selected = ranges.GetElement(0)?;
            let original = self.range.resolve()?;
            Ok(selected.CompareEndpoints(
                TextPatternRangeEndpoint_Start,
                &original,
                TextPatternRangeEndpoint_Start,
            )? == 0
                && selected.CompareEndpoints(
                    TextPatternRangeEndpoint_End,
                    &original,
                    TextPatternRangeEndpoint_End,
                )? == 0)
        })();
        #[cfg(debug_assertions)]
        eprintln!(
            "[replacement] target event {}: unchanged={unchanged:?}",
            event.0
        );
        let unchanged = unchanged.unwrap_or(false);
        if unchanged {
            return Ok(());
        }
        self.invalidate();
        Ok(())
    }
}
impl IUIAutomationFocusChangedEventHandler_Impl for TargetEvents_Impl {
    fn HandleFocusChangedEvent(&self, _: Ref<IUIAutomationElement>) -> windows::core::Result<()> {
        // UIA can announce the already-focused element after subscription.
        // A reaffirmation is not a target change; leaving it remains permanent.
        let same_focus = (|| unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            Ok::<_, windows::core::Error>(
                automation
                    .CompareElements(&self.focused.resolve()?, &automation.GetFocusedElement()?)?
                    .as_bool(),
            )
        })();
        #[cfg(debug_assertions)]
        eprintln!("[replacement] focus event: same={same_focus:?}");
        if same_focus.unwrap_or(false) {
            return Ok(());
        }
        self.invalidate();
        Ok(())
    }
}

pub struct EditingTarget {
    automation: IUIAutomation,
    focused: IUIAutomationElement,
    pattern: IUIAutomationTextPattern,
    value: IUIAutomationValuePattern,
    range: IUIAutomationTextRange,
    origin: CaptureOrigin,
    pub original: String,
    before: String,
    prefix: String,
    suffix: String,
    dirty: Arc<AtomicBool>,
    armed: Arc<AtomicBool>,
    events: IUIAutomationEventHandler,
    focus_events: IUIAutomationFocusChangedEventHandler,
}
impl EditingTarget {
    pub fn capture(origin: CaptureOrigin) -> Result<Self, AppError> {
        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(native)?;
            if let Ok(options) = automation.cast::<IUIAutomation2>() {
                options.SetConnectionTimeout(1000).map_err(native)?;
                options.SetTransactionTimeout(1000).map_err(native)?;
            }
            let focused = automation.GetFocusedElement().map_err(native)?;
            if focused.CurrentIsPassword().map_err(native)?.as_bool()
                || !focused.CurrentIsEnabled().map_err(native)?.as_bool()
                || focused.CurrentProcessId().map_err(native)? as u32 != origin.process_id
                || origin.process_id == std::process::id()
            {
                return Err(unsupported());
            }
            let value = focused
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .map_err(|_| unsupported())?;
            if value.CurrentIsReadOnly().map_err(native)?.as_bool() {
                return Err(unsupported());
            }
            let pattern = focused
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .map_err(|_| unsupported())?;
            let selected = pattern.GetSelection().map_err(native)?;
            if selected.Length().map_err(native)? != 1 {
                return Err(unsupported());
            }
            let range = selected
                .GetElement(0)
                .map_err(native)?
                .Clone()
                .map_err(native)?;
            let original = range.GetText(4001).map_err(native)?.to_string();
            if original.trim().is_empty()
                || original.chars().count() > 4000
                || original.contains('\0')
            {
                return Err(AppError::new(
                    "invalid_input",
                    "请选中不超过 4000 字符的英文句子。",
                ));
            }
            let before = value.CurrentValue().map_err(native)?.to_string();
            if before.chars().count() > 65536 {
                return Err(unsupported());
            }
            // Surrounding text is local verification data only, never sent to providers.
            let prefix_range = pattern.DocumentRange().map_err(native)?;
            prefix_range
                .MoveEndpointByRange(
                    TextPatternRangeEndpoint_End,
                    &range,
                    TextPatternRangeEndpoint_Start,
                )
                .map_err(native)?;
            let prefix = prefix_range.GetText(65537).map_err(native)?.to_string();
            if !before.starts_with(&prefix) || !before[prefix.len()..].starts_with(&original) {
                return Err(unsupported());
            }
            let suffix = before[prefix.len() + original.len()..].to_string();
            let dirty = Arc::new(AtomicBool::new(false));
            let armed = Arc::new(AtomicBool::new(true));
            let events: IUIAutomationEventHandler = TargetEvents {
                changed: dirty.clone(),
                armed: armed.clone(),
                focused: windows_core::AgileReference::new(&focused).map_err(native)?,
                range: windows_core::AgileReference::new(&range).map_err(native)?,
                before: before.clone(),
            }
            .into();
            let focus_events = events
                .cast::<IUIAutomationFocusChangedEventHandler>()
                .map_err(native)?;
            let target = Self {
                automation,
                focused,
                pattern,
                value,
                range,
                origin,
                original,
                before,
                prefix,
                suffix,
                dirty,
                armed,
                events,
                focus_events,
            };
            target
                .automation
                .AddAutomationEventHandler(
                    UIA_Text_TextSelectionChangedEventId,
                    &target.focused,
                    TreeScope_Element,
                    None,
                    &target.events,
                )
                .map_err(native)?;
            target
                .automation
                .AddAutomationEventHandler(
                    UIA_Text_TextChangedEventId,
                    &target.focused,
                    TreeScope_Element,
                    None,
                    &target.events,
                )
                .map_err(native)?;
            target
                .automation
                .AddFocusChangedEventHandler(None, &target.focus_events)
                .map_err(native)?;
            target.validate()?;
            Ok(target)
        }
    }
    pub fn validate(&self) -> Result<(), AppError> {
        unsafe {
            let window = GetForegroundWindow();
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if self.dirty.load(Ordering::Acquire) {
                return Err(AppError::new(
                    "target_changed",
                    "检测到输入焦点或选区变化，本次只保留译文。",
                ));
            }
            if window != HWND(self.origin.window as *mut _) || pid != self.origin.process_id {
                return Err(AppError::new(
                    "target_changed",
                    "前台窗口已变化，本次只保留译文。",
                ));
            }
            if !self
                .automation
                .CompareElements(
                    &self.focused,
                    &self.automation.GetFocusedElement().map_err(native)?,
                )
                .map_err(native)?
                .as_bool()
            {
                return Err(AppError::new(
                    "target_changed",
                    "当前输入焦点已变化，本次只保留译文。",
                ));
            }
            if self.value.CurrentIsReadOnly().map_err(native)?.as_bool()
                || self.value.CurrentValue().map_err(native)?.to_string() != self.before
            {
                return Err(AppError::new(
                    "target_changed",
                    "输入内容或可编辑状态已变化，本次只保留译文。",
                ));
            }
            let ranges = self.pattern.GetSelection().map_err(native)?;
            if ranges.Length().map_err(native)? != 1 {
                return Err(changed());
            }
            let selected = ranges.GetElement(0).map_err(native)?;
            if selected
                .CompareEndpoints(
                    TextPatternRangeEndpoint_Start,
                    &self.range,
                    TextPatternRangeEndpoint_Start,
                )
                .map_err(native)?
                != 0
                || selected
                    .CompareEndpoints(
                        TextPatternRangeEndpoint_End,
                        &self.range,
                        TextPatternRangeEndpoint_End,
                    )
                    .map_err(native)?
                    != 0
                || selected.GetText(4001).map_err(native)?.to_string() != self.original
            {
                return Err(changed());
            }
            Ok(())
        }
    }
    pub fn expected(&self, translation: &str) -> String {
        format!("{}{}{}", self.prefix, translation, self.suffix)
    }
    pub fn disarm(&self) {
        self.armed.store(false, Ordering::Release);
    }
    pub fn read_value(&self) -> Result<String, AppError> {
        unsafe {
            self.value
                .CurrentValue()
                .map(|v| v.to_string())
                .map_err(native)
        }
    }
}
impl Drop for EditingTarget {
    fn drop(&mut self) {
        unsafe {
            self.disarm();
            let _ = self.automation.RemoveAutomationEventHandler(
                UIA_Text_TextSelectionChangedEventId,
                &self.focused,
                &self.events,
            );
            let _ = self.automation.RemoveAutomationEventHandler(
                UIA_Text_TextChangedEventId,
                &self.focused,
                &self.events,
            );
            let _ = self
                .automation
                .RemoveFocusChangedEventHandler(&self.focus_events);
        }
    }
}

pub fn modifiers_released() -> bool {
    unsafe {
        [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN, VK_V]
            .iter()
            .all(|key| GetAsyncKeyState(key.0 as i32) >= 0)
    }
}
pub fn paste_once() -> Result<(), AppError> {
    let key = |vk, flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let input = [
        key(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        key(VK_V, KEYBD_EVENT_FLAGS(0)),
        key(VK_V, KEYEVENTF_KEYUP),
        key(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    let sent = unsafe { SendInput(&input, std::mem::size_of::<INPUT>() as i32) };
    if sent == 4 {
        Ok(())
    } else {
        // Release only keys from our partial injection; never retry the paste.
        if sent > 0 {
            let _ = unsafe { SendInput(&input[2..], std::mem::size_of::<INPUT>() as i32) };
        }
        Err(AppError::new(
            "paste_unverified",
            "系统未确认完整粘贴操作（可能是应用权限限制），请核对原输入，勿重复自动替换。",
        ))
    }
}

struct ClipboardOpen;
impl ClipboardOpen {
    fn new(owner: isize) -> Result<Self, AppError> {
        unsafe {
            OpenClipboard(Some(HWND(owner as *mut _))).map_err(native)?;
        }
        Ok(Self)
    }
}
impl Drop for ClipboardOpen {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}
struct GlobalBlock(HGLOBAL);
impl GlobalBlock {
    fn bytes(bytes: &[u8]) -> Result<Self, AppError> {
        unsafe {
            let block = Self(GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(native)?);
            let pointer = GlobalLock(block.0);
            if pointer.is_null() {
                return Err(unsupported());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
            let _ = GlobalUnlock(block.0);
            Ok(block)
        }
    }
    fn publish(&mut self, format: u32) -> Result<(), AppError> {
        unsafe {
            SetClipboardData(format, Some(HANDLE(self.0.0))).map_err(native)?;
        }
        self.0 = HGLOBAL::default();
        Ok(())
    }
}
impl Drop for GlobalBlock {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            unsafe {
                let _ = GlobalFree(Some(self.0));
            }
        }
    }
}

/// First implementation preserves materialized HGLOBAL formats (text/HTML/etc).
/// Unsupported non-memory formats are rejected before EmptyClipboard is called.
pub struct ClipboardLease {
    saved: Vec<(u32, Vec<u8>)>,
    pub sequence: u32,
    owner: isize,
}
impl ClipboardLease {
    pub fn install(text: &str, owner: isize) -> Result<Self, AppError> {
        let open = ClipboardOpen::new(owner)?;
        let mut lease = Self {
            saved: vec![],
            sequence: 0,
            owner,
        };
        let mut format = 0;
        let mut size = 0;
        unsafe {
            loop {
                format = EnumClipboardFormats(format);
                if format == 0 {
                    break;
                }
                // Windows synthesizes CF_BITMAP from a DIB. Preserve its memory
                // representation; Windows synthesizes the bitmap again on restore.
                if format == 2
                    && (IsClipboardFormatAvailable(8).is_ok()
                        || IsClipboardFormatAvailable(17).is_ok())
                {
                    continue;
                }
                if matches!(format, 2 | 3 | 9 | 14 | 0x80..=0x8f | 0x200..=0x2ff) {
                    return Err(AppError::new(
                        "clipboard_unsupported",
                        "剪贴板含无法保留的对象格式，本次只提供译文。",
                    ));
                }
                let handle = GetClipboardData(format).map_err(native)?;
                let global = HGLOBAL(handle.0);
                let bytes = GlobalSize(global);
                size += bytes;
                if bytes == 0 || size > 64 * 1024 * 1024 {
                    return Err(AppError::new(
                        "clipboard_unsupported",
                        "当前剪贴板内容无法完整保留，本次只提供译文。",
                    ));
                }
                let pointer = GlobalLock(global);
                if pointer.is_null() {
                    return Err(unsupported());
                }
                let copy = std::slice::from_raw_parts(pointer.cast(), bytes).to_vec();
                let _ = GlobalUnlock(global);
                lease.saved.push((format, copy));
            }
            let utf16 = text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let mut replacement = GlobalBlock::bytes(std::slice::from_raw_parts(
                utf16.as_ptr().cast(),
                utf16.len() * 2,
            ))?;
            EmptyClipboard().map_err(native)?;
            // Keep a recoverable lease even if publishing fails after clearing.
            let result = replacement.publish(13);
            lease.sequence = GetClipboardSequenceNumber();
            if let Err(error) = result {
                lease.restore_open()?;
                return Err(error);
            }
        }
        // Closing the clipboard can finish Windows' synthesized formats. Track
        // the published clipboard, rather than the intermediate open state.
        drop(open);
        let sequence = unsafe { GetClipboardSequenceNumber() };
        #[cfg(debug_assertions)]
        eprintln!(
            "[replacement] clipboard published: {} -> {sequence}",
            lease.sequence
        );
        if unsafe { GetClipboardOwner() }.is_ok_and(|window| window == HWND(owner as *mut _)) {
            lease.sequence = sequence;
        }
        Ok(lease)
    }
    fn restore_open(&mut self) -> Result<(), AppError> {
        unsafe {
            if GetClipboardSequenceNumber() != self.sequence {
                return Ok(());
            }
            let mut blocks = self
                .saved
                .iter()
                .map(|(format, bytes)| Ok((*format, GlobalBlock::bytes(bytes)?)))
                .collect::<Result<Vec<_>, AppError>>()?;
            EmptyClipboard().map_err(native)?;
            for (format, block) in &mut blocks {
                block.publish(*format)?;
            }
            Ok(())
        }
    }
    pub fn restore(&mut self) -> Result<(), AppError> {
        let _open = ClipboardOpen::new(self.owner)?;
        self.restore_open()
    }
    pub fn still_owned(&self) -> bool {
        unsafe {
            GetClipboardSequenceNumber() == self.sequence
                && GetClipboardOwner().is_ok_and(|window| window == HWND(self.owner as *mut _))
        }
    }
}

pub fn confirm_paste(target: &EditingTarget, expected: &str) -> bool {
    // Readback confirms consumption, rather than assuming a fixed delay is enough.
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(2) {
        if target.read_value().is_ok_and(|v| v == expected) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}
