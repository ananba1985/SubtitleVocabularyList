//! One real OpenGL subtitle/capture path in an invisible window, without audio or user data.
#[cfg(all(windows, debug_assertions))]
pub use subtitle_vocabulary_list_core::error;
#[cfg(all(windows, debug_assertions))]
#[path = "../src/player/mpv.rs"]
#[allow(dead_code)]
mod mpv;

#[cfg(all(windows, debug_assertions))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::json;
    use std::{
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
                WindowsAndMessaging::*,
            },
        },
        core::w,
    };

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or("new isolated output directory required")?,
    );
    let video = dunce::canonicalize(args.next().ok_or("silent bilingual video required")?)?;
    if root.exists() {
        return Err(
            "Existing acceptance evidence must be preserved; choose a new directory".into(),
        );
    }
    std::fs::create_dir_all(&root)?;
    let root = dunce::canonicalize(root)?;
    let dll = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/runtime/mpv/libmpv-2.dll");
    let window = unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            style: CS_OWNDC,
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: w!("SVL_InvisibleSubtitleAcceptance"),
            ..Default::default()
        };
        assert_ne!(RegisterClassW(&class), 0);
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class.lpszClassName,
            w!("SVL invisible subtitle acceptance"),
            WS_POPUP,
            0,
            0,
            960,
            600,
            None,
            None,
            Some(instance.into()),
            None,
        )?
    };
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        assert!(!unsafe { IsWindowVisible(window) }.as_bool());
        let engine = mpv::Engine::new(window, &dll, &root.join("cache"))?;
        engine.command(json!(["set_property", "aid", "no"]))?;
        engine.command(json!(["set_property", "mute", true]))?;
        engine.command(json!(["loadfile", video]))?;
        let pump_until = |ready: &dyn Fn() -> bool| -> Result<(), Box<dyn std::error::Error>> {
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline {
                while let Some(event) = engine.event() {
                    if let Some(hook) = event.hook {
                        engine.continue_load(hook)?;
                    }
                    if event.id == 7 && event.error < 0 {
                        return Err(format!("mpv load error {}", event.error).into());
                    }
                }
                engine.draw(960, 600, false)?;
                if ready() {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(8));
            }
            Err("Timed out waiting for normal subtitle state".into())
        };
        pump_until(&|| engine.number("duration").is_some())?;
        engine.command(json!(["set_property", "sid", 1]))?;
        engine.command(json!(["seek", 20, "absolute+exact"]))?;
        pump_until(&|| {
            engine
                .number("time-pos")
                .is_some_and(|n| (n - 20.0).abs() < 0.05)
                && engine.property("sub-text") == json!("I will arrive tomorrow.")
        })?;
        engine.capture(&root.join("primary.png"))?;
        engine.command(json!(["set_property", "secondary-sid", 2]))?;
        pump_until(&|| engine.property("secondary-sub-text") == json!("我明天会到。"))?;
        engine.capture(&root.join("secondary.png"))?;
        assert!(engine.flag("pause"));
        assert!(!unsafe { IsWindowVisible(window) }.as_bool());
        let primary = image::open(root.join("primary.png"))?.to_rgba8();
        let secondary = image::open(root.join("secondary.png"))?.to_rgba8();
        assert_eq!(primary.dimensions(), secondary.dimensions());
        let (width, height) = secondary.dimensions();
        let mut changed = 0;
        for y in 0..height / 4 {
            for x in width / 4..width * 3 / 4 {
                let (a, b) = (primary.get_pixel(x, y), secondary.get_pixel(x, y));
                if (0..3).map(|c| a[c].abs_diff(b[c]) as u32).sum::<u32>() > 48 {
                    changed += 1;
                }
            }
        }
        assert!(
            changed > 40,
            "Selected Chinese subtitle is absent from the captured frame"
        );
        std::fs::write(
            root.join("result.json"),
            serde_json::to_vec_pretty(&json!({
                "windowVisible": false, "audioOutput": false, "renderer": "libmpv/OpenGL",
                "primaryText": engine.property("sub-text"), "secondaryText": engine.property("secondary-sub-text"),
                "paused": engine.flag("pause"), "position": engine.number("time-pos"),
                "capturedWidth": width, "capturedHeight": height, "subtitlePixelsChanged": changed
            }))?,
        )?;
        println!(
            "PASS: real Engine OpenGL dual subtitles and frame capture; hidden window, no audio, no personal Data. {}",
            root.display()
        );
        Ok(())
    })();
    unsafe {
        DestroyWindow(window)?;
    }
    result
}

#[cfg(not(all(windows, debug_assertions)))]
fn main() {
    panic!("This native OpenGL acceptance requires a Windows debug build");
}
