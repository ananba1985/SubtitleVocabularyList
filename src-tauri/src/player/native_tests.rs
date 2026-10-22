//! Explicit opt-in normal path using the real actor, an invisible HWND and isolated storage.
use super::*;
use crate::{application::Settings, store::Store, tasks::TaskManager};
use std::{path::PathBuf, sync::mpsc::Receiver, thread};

struct HiddenWindow(HWND);
impl HiddenWindow {
    fn new() -> Self {
        unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let class = WNDCLASSW {
                style: CS_OWNDC,
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: w!("SVL_BackgroundPlaybackTest"),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            let window = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class.lpszClassName,
                w!("SVL invisible playback test"),
                WS_POPUP,
                0,
                0,
                960,
                600,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            assert!(!IsWindowVisible(window).as_bool());
            Self(window)
        }
    }
}
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
impl Drop for HiddenWindow {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0).unwrap();
        }
    }
}

fn wait(actor: &mut Actor, messages: &Receiver<Message>, ready: impl Fn(&Actor) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        while let Ok(message) = messages.try_recv() {
            actor.receive(message);
        }
        actor.tick();
        if ready(actor) {
            return;
        }
        thread::sleep(Duration::from_millis(8));
    }
    panic!(
        "Normal playback did not reach expected state: {}",
        serde_json::to_string(&actor.state).unwrap()
    );
}
fn control(actor: &mut Actor, action: &str, value: Option<f64>, text: Option<&str>) {
    actor
        .control(WatchControl {
            action: action.into(),
            value,
            text: text.map(String::from),
            expected_revision: None,
        })
        .unwrap();
}
fn select(actor: &mut Actor, action: &str) {
    actor
        .playlist(
            PlaylistChange {
                action: action.into(),
                paths: vec![],
                index: Some(0),
                destination: None,
            },
            vec![],
        )
        .unwrap();
}

#[test]
#[ignore = "requires an explicit fresh output directory and silent bilingual fixture"]
fn normal_background_flow() {
    let root = PathBuf::from(
        std::env::var_os("SVL_PLAYER_TEST_OUTPUT").expect("SVL_PLAYER_TEST_OUTPUT required"),
    );
    let fixture = PathBuf::from(
        std::env::var_os("SVL_PLAYER_TEST_VIDEO").expect("SVL_PLAYER_TEST_VIDEO required"),
    );
    assert!(
        !root.exists(),
        "Preserve old acceptance data; choose a fresh directory"
    );
    std::fs::create_dir_all(root.join("Series")).unwrap();
    let root = dunce::canonicalize(root).unwrap();
    for name in ["Episode2.mkv", "Episode10.mkv"] {
        std::fs::copy(&fixture, root.join("Series").join(name)).unwrap();
    }
    let english = root.join("External English.srt");
    let chinese = root.join("External Chinese.srt");
    std::fs::write(
        &english,
        "1\n00:00:00,000 --> 00:01:00,000\nWe are ready.\n",
    )
    .unwrap();
    std::fs::write(
        &chinese,
        "1\n00:00:00,000 --> 00:01:00,000\n我们准备好了。\n",
    )
    .unwrap();
    let store = Arc::new(Store::open(root.join("Data")).unwrap());
    let mut playback = PlaybackState::default();
    playback.preferences.muted = true;
    store.save_playback_state(&playback).unwrap();
    let tasks = TaskManager::new(store.clone(), Arc::new(|_| {}));
    let application = Arc::new(Application::new(
        store.clone(),
        tasks,
        Settings {
            watch_auto_collect: false,
            offline_mode: true,
            ..Default::default()
        },
    ));
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .any_thread()
        .build(context)
        .unwrap();
    assert!(app.webview_windows().is_empty());
    let window = HiddenWindow::new();
    let dll = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/runtime/mpv/libmpv-2.dll");
    let engine = mpv::Engine::new(window.0, &dll, &root.join("cache")).unwrap();
    engine
        .command(json!(["set_property", "aid", "no"]))
        .unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut actor = Actor::new(
        engine,
        application,
        app.handle().clone(),
        Arc::default(),
        sender,
        window.0.0 as isize,
    );
    let inbox = files::FileInbox::default();
    inbox.push(
        vec![root.join("Series").to_string_lossy().into_owned()],
        false,
    );
    inbox.push(vec![english.to_string_lossy().into_owned()], false);
    inbox.push(vec![chinese.to_string_lossy().into_owned()], false);
    while let Some(request) = inbox.take().unwrap() {
        actor
            .files(
                store
                    .playback_files(&request.paths, request.activate)
                    .unwrap(),
            )
            .unwrap();
    }
    wait(&mut actor, &receiver, |a| {
        a.state.loaded && a.engine.property("secondary-sub-text") == json!("我们准备好了。")
    });
    assert_eq!(actor.state.playback.playlist.len(), 2);
    assert_eq!(actor.state.playback.playlist[0].title, "Episode2");
    assert_eq!(actor.state.playback.playlist[1].title, "Episode10");
    let first_path = actor.state.path.clone();
    let first_session = actor.state.session_id.clone();
    assert!(!actor.engine.flag("pause"));
    control(&mut actor, "pause", None, None);
    assert_eq!(actor.engine.property("sub-text"), json!("We are ready."));

    control(&mut actor, "subtitle", Some(1.0), None);
    control(&mut actor, "subtitle_delay", Some(0.25), None);
    select(&mut actor, "next");
    wait(&mut actor, &receiver, |a| {
        a.state.loaded && a.state.path != first_path
    });
    let saved = store.watch_progress_for(&first_path).unwrap().unwrap();
    assert_eq!(
        saved.subtitle_id,
        Some(1),
        "Switching files must preserve the just-selected subtitle"
    );
    assert_eq!(saved.subtitle_delay, 0.25);
    assert_eq!(saved.external_subtitles.len(), 2);
    assert_ne!(actor.state.session_id, first_session);
    assert!(actor.state.context.is_none());
    assert!(actor.state.remaining_seconds.is_none());
    assert!(
        actor
            .engine
            .number("secondary-sid")
            .is_none_or(|id| id <= 0.0),
        "New file secondary track: {}",
        actor.engine.property("secondary-sid")
    );

    select(&mut actor, "previous");
    wait(&mut actor, &receiver, |a| {
        a.state.loaded && a.state.path == first_path
    });
    assert_eq!(actor.engine.number("sid"), Some(1.0));
    assert_eq!(actor.engine.number("sub-delay"), Some(0.25));
    assert_eq!(
        actor.engine.property("secondary-sub-text"),
        json!("我们准备好了。")
    );
    control(&mut actor, "speed", Some(1.25), None);
    let end = actor.engine.number("duration").unwrap() - 0.15;
    control(&mut actor, "seek", Some(end), None);
    control(&mut actor, "play", None, None);
    wait(&mut actor, &receiver, |a| {
        a.state.loaded && a.state.path != first_path && !a.engine.flag("pause")
    });
    assert!(!actor.user_pause);
    assert!(actor.state.context.is_none());
    assert!(actor.state.remaining_seconds.is_none());
    assert_eq!(actor.engine.number("speed"), Some(1.25));
    let second_path = actor.state.path.clone();
    actor
        .files(
            store
                .playback_files(&[chinese.to_string_lossy().into_owned()], false)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(actor.state.path, second_path);
    assert!(!actor.engine.flag("pause"));
    assert_eq!(actor.state.status, "playing");
    control(&mut actor, "pause", None, None);
    wait(&mut actor, &receiver, |a| {
        a.engine.property("sub-text") == json!("我们准备好了。")
    });
    let saved = store.watch_progress_for(&second_path).unwrap().unwrap();
    assert_eq!(
        saved.external_subtitles,
        vec![chinese.to_string_lossy().into_owned()]
    );
    assert_eq!(actor.state.playback.playlist.len(), 2);
    actor.save_progress();
    assert!(!unsafe { IsWindowVisible(window.0) }.as_bool());
    assert!(app.webview_windows().is_empty());
    std::fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&json!({
            "nativeActor": true, "windowVisible": false, "audioOutput": false,
            "directoryAndDualSubtitles": true, "recentSubtitleRestored": true,
            "autoNext": true, "speed": 1.25, "subtitleDropWhilePlaying": true,
            "firstProgress": store.watch_progress_for(&first_path).unwrap(),
            "secondProgress": store.watch_progress_for(&second_path).unwrap(),
            "playback": store.playback_state().unwrap()
        }))
        .unwrap(),
    )
    .unwrap();
    println!(
        "PASS: real actor subtitle restore, file batches and EOF auto-next; isolated output {}",
        root.display()
    );
}

fn with_isolated_player(
    check: impl FnOnce(&mut Actor, &Receiver<Message>, &Store, &std::path::Path, &str),
) {
    let root = PathBuf::from(
        std::env::var_os("SVL_PLAYER_TEST_OUTPUT").expect("SVL_PLAYER_TEST_OUTPUT required"),
    );
    let fixture = PathBuf::from(
        std::env::var_os("SVL_PLAYER_TEST_VIDEO").expect("SVL_PLAYER_TEST_VIDEO required"),
    );
    assert!(!root.exists(), "Choose a fresh output directory");
    std::fs::create_dir_all(&root).unwrap();
    let root = dunce::canonicalize(root).unwrap();
    let path = root.join("Episode1.mkv");
    std::fs::copy(fixture, &path).unwrap();
    let path = path.to_string_lossy().into_owned();
    let store = Arc::new(Store::open(root.join("Data")).unwrap());
    let mut playback = PlaybackState::default();
    playback.preferences.muted = true;
    store.save_playback_state(&playback).unwrap();
    let tasks = TaskManager::new(store.clone(), Arc::new(|_| {}));
    let application = Arc::new(Application::new(
        store.clone(),
        tasks,
        Settings {
            watch_auto_collect: false,
            offline_mode: true,
            ..Default::default()
        },
    ));
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .any_thread()
        .build(context)
        .unwrap();
    let window = HiddenWindow::new();
    let dll = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/runtime/mpv/libmpv-2.dll");
    let engine = mpv::Engine::new(window.0, &dll, &root.join("cache")).unwrap();
    engine
        .command(json!(["set_property", "aid", "no"]))
        .unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut actor = Actor::new(
        engine,
        application,
        app.handle().clone(),
        Arc::default(),
        sender,
        window.0.0 as isize,
    );
    check(&mut actor, &receiver, &store, &root, &path);
    control(&mut actor, "close", None, None);
    assert!(!unsafe { IsWindowVisible(window.0) }.as_bool());
    assert!(app.webview_windows().is_empty());
}

#[test]
#[ignore = "requires an explicit fresh output directory and silent video fixture"]
fn open_autoplays() {
    with_isolated_player(|actor, receiver, store, root, path| {
        let mut results = Vec::new();
        for action in ["open", "select", "append", "file-open", "restore"] {
            let previous_session = actor.state.session_id.clone();
            if actor.state.loaded {
                control(actor, "pause", None, None);
                if action != "select" {
                    control(actor, "close", None, None);
                }
            }
            match action {
                "open" => actor
                    .open(store.watch_media(Some(&path), None).unwrap())
                    .unwrap(),
                "select" | "restore" => select(actor, action),
                _ => actor
                    .files(
                        store
                            .playback_files(&[path.to_owned()], action == "file-open")
                            .unwrap(),
                    )
                    .unwrap(),
            }
            wait(actor, receiver, |a| {
                a.state.session_id != previous_session
                    && a.state.loaded
                    && a.state.playing
                    && !a.engine.flag("pause")
                    && a.engine.number("time-pos").unwrap_or(0.0) > 0.1
            });
            assert!(!actor.user_pause);
            results.push(
                json!({"action":action,"playing":true,"position":actor.engine.number("time-pos")}),
            );
        }
        std::fs::write(
            root.join("result.json"),
            serde_json::to_vec_pretty(&json!({
                "nativeActor":true,"windowVisible":false,"audioOutput":false,"opens":results
            }))
            .unwrap(),
        )
        .unwrap();
        println!(
            "PASS: opening starts real mpv playback without a second play action; {}",
            root.display()
        );
    });
}

#[test]
#[ignore = "requires an explicit fresh output directory and silent video fixture"]
fn mouse_stop() {
    with_isolated_player(|actor, receiver, store, root, path| {
        actor
            .open(store.watch_media(Some(path), None).unwrap())
            .unwrap();
        wait(actor, receiver, |a| {
            a.state.loaded && a.state.playing && a.state.position > 0.1
        });
        let playlist = actor.state.playback.playlist.clone();
        actor
            .engine
            .command(json!([
                "script-message-to",
                "osc",
                "osc-visibility",
                "always"
            ]))
            .unwrap();
        wait(actor, receiver, |a| {
            a.engine.number("user-data/osc/margins/b").unwrap_or(0.0) > 0.0
        });
        let dimensions = actor.engine.property("osd-dimensions");
        let width = dimensions["w"].as_f64().unwrap();
        let height = dimensions["h"].as_f64().unwrap();
        // Coordinates of the built-in bottombar's rightmost custom button, scaled
        // using the native OSC's own margin. This never moves the OS cursor.
        let scale = height * actor.engine.number("user-data/osc/margins/b").unwrap() / 56.0;
        let x = (width - 16.0 * scale).round() as i32;
        let y = (height - 42.0 * scale).round() as i32;
        actor.engine.command(json!(["mouse", x, y])).unwrap();
        actor.engine.capture(&root.join("stop-button.png")).unwrap();
        actor
            .engine
            .command(json!(["keydown", "MBTN_LEFT"]))
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        actor.engine.command(json!(["keyup", "MBTN_LEFT"])).unwrap();
        wait(actor, receiver, |a| {
            !a.state.loaded && !a.state.playing && a.state.path.is_empty()
        });
        assert_eq!(actor.state.playback.playlist, playlist);
        let progress = store.watch_progress_for(path).unwrap().unwrap();
        assert!(progress.position > 0.1);
        assert!(!actor.fullscreen);
        assert!(!actor.engine.flag("fullscreen"));
        assert!(actor.state.context.is_none());
        assert!(actor.state.remaining_seconds.is_none());
        std::fs::write(
            root.join("result.json"),
            serde_json::to_vec_pretty(&json!({
                "nativeOscMouseClick":true,"windowVisible":false,"audioOutput":false,
                "buttonPosition":{"x":x,"y":y},"playlistPreserved":true,
                "savedPosition":progress.position,"playing":actor.state.playing,
                "path":actor.state.path,"fullscreen":actor.fullscreen
            }))
            .unwrap(),
        )
        .unwrap();
        println!(
            "PASS: native OSC mouse stop preserves progress and playlist; {}",
            root.display()
        );
    });
}

#[test]
#[ignore = "requires an explicit fresh output directory and silent video fixture"]
fn native_settings_button() {
    with_isolated_player(|actor, receiver, store, root, path| {
        actor
            .open(store.watch_media(Some(path), None).unwrap())
            .unwrap();
        wait(actor, receiver, |a| a.state.loaded && a.state.playing);
        actor
            .engine
            .command(json!([
                "script-message-to",
                "osc",
                "osc-visibility",
                "always"
            ]))
            .unwrap();
        wait(actor, receiver, |a| {
            a.engine.number("user-data/osc/margins/b").unwrap_or(0.0) > 0.0
        });
        let dimensions = actor.engine.property("osd-dimensions");
        let width = dimensions["w"].as_f64().unwrap();
        let height = dimensions["h"].as_f64().unwrap();
        let scale = height * actor.engine.number("user-data/osc/margins/b").unwrap() / 56.0;
        let x = (width - 43.0 * scale).round() as i32;
        let y = (height - 42.0 * scale).round() as i32;
        actor.engine.command(json!(["mouse", x, y])).unwrap();
        actor
            .engine
            .capture(&root.join("settings-button.png"))
            .unwrap();
        actor
            .engine
            .command(json!(["keydown", "MBTN_LEFT"]))
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        actor.engine.command(json!(["keyup", "MBTN_LEFT"])).unwrap();
        // Read the native button event without executing a desktop popup.
        let until = Instant::now() + Duration::from_secs(8);
        let mut received = false;
        while !received && Instant::now() < until {
            while let Some(event) = actor.engine.event() {
                if event
                    .messages
                    .first()
                    .is_some_and(|name| name == "svl-settings")
                {
                    received = true;
                    break;
                }
            }
            if !received {
                thread::sleep(Duration::from_millis(8));
            }
        }
        assert!(
            received,
            "Native settings button must emit its menu request"
        );
        std::fs::write(
            root.join("result.json"),
            serde_json::to_vec_pretty(&json!({
                "nativeOscMouseClick":true,"settingsMessage":"svl-settings",
                "windowVisible":false,"audioOutput":false,"desktopPopupOpened":false,
                "buttonPosition":{"x":x,"y":y}
            }))
            .unwrap(),
        )
        .unwrap();
        println!(
            "PASS: native settings button emits its menu request; {}",
            root.display()
        );
    });
}
