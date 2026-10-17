mod commands;

use crate::{
    application::{Application, Settings},
    desktop_capture::{self, NativeDesktop},
    error::AppError,
    store::Store,
    tasks::TaskManager,
};
use commands::system::TrayAction;
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;

fn tray_navigate(app: &tauri::AppHandle, action: &str) {
    if let Ok(mut pending) = app.state::<TrayAction>().0.lock() {
        *pending = Some(action.to_owned());
    }
    desktop_capture::show_main(app);
    let _ = app.emit_to("main", "tray_action", ());
}

pub fn run() {
    let _startup = std::time::Instant::now();
    let root = crate::app_paths::data_directory().expect("Cannot resolve the data directory");
    crate::app_paths::prepare_process(&root)
        .expect("Cannot prepare the data and cache directories");
    let mut context = tauri::generate_context!();
    let main_config = context.config_mut().app.windows.remove(0);
    let first_page = Arc::new(std::sync::atomic::AtomicBool::new(true));
    tauri::Builder::default()
        .on_page_load(move |webview, event| {
            if webview.label()=="translator" && event.event()==tauri::webview::PageLoadEvent::Finished {let _=webview.window().show();}
            if webview.label() == "main" {
                #[cfg(debug_assertions)]
                eprintln!(
                    "[startup] webview {:?}: {} ms",
                    event.event(),
                    _startup.elapsed().as_millis()
                );
                if event.event() == tauri::webview::PageLoadEvent::Finished
                    && first_page.swap(false, std::sync::atomic::Ordering::AcqRel)
                {
                    let window = webview.window();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if let Some(native) = app.try_state::<NativeDesktop>() && matches!(native.action(shortcut), Some(crate::desktop_capture::CaptureAction::Replacement)) {
                        if event.state() == ShortcutState::Released { native.replacement_key_down.store(false, std::sync::atomic::Ordering::Release); }
                        else if !native.replacement_key_down.swap(true, std::sync::atomic::Ordering::AcqRel) { app.state::<crate::desktop_replacement::NativeReplacement>().toggle(app); }
                        return;
                    }
                    if event.state() == ShortcutState::Pressed
                        && let Some(native) = app.try_state::<NativeDesktop>()
                    {
                        let result = match native.action(shortcut) {
                            Some(crate::desktop_capture::CaptureAction::Selection) => {
                                Some(native.capture(app))
                            }
                            Some(crate::desktop_capture::CaptureAction::Ocr) => {
                                Some(native.ocr_start(app))
                            }
                            Some(crate::desktop_capture::CaptureAction::Input) => {
                                if let Err(error)=crate::desktop_query::open_input(app){desktop_capture::capture_failed(app,&error);}
                                None
                            }
                            Some(crate::desktop_capture::CaptureAction::Replacement) => None,
                            None => None,
                        };
                        if let Some(Err(error)) = result
                            && error.code != "capture_busy"
                        {
                            desktop_capture::capture_failed(app, &error);
                        }
                    }
                })
                .build(),
        )
        .setup(move |app| {
            // Use the builder's absolute path API; window-config dataDirectory
            // is resolved relative to the system local-data directory by Tauri.
            tauri::WebviewWindowBuilder::from_config(app.handle(), &main_config)?
                .data_directory(crate::app_paths::webview_directory(&root))
                .build()?;
            #[cfg(debug_assertions)]
            eprintln!(
                "[startup] native setup begins: {} ms",
                _startup.elapsed().as_millis()
            );
            if let Some(window) = app.get_webview_window("main")
                && let Some(monitor) = window.current_monitor()?.or(app.primary_monitor()?)
            {
                let work = monitor.work_area();
                let scale = monitor.scale_factor();
                let size = crate::desktop_layout::initial_window_size(
                    work.size.width as f64 / scale,
                    work.size.height as f64 / scale,
                );
                window.set_min_size(Some(tauri::LogicalSize::new(
                    size.min_width,
                    size.min_height,
                )))?;
                window.set_size(tauri::LogicalSize::new(size.width, size.height))?;
                let outer = window.outer_size()?;
                window.set_position(tauri::PhysicalPosition::new(
                    work.position.x + (work.size.width.saturating_sub(outer.width) / 2) as i32,
                    work.position.y + (work.size.height.saturating_sub(outer.height) / 2) as i32,
                ))?;
            }
            #[cfg(debug_assertions)]
            eprintln!(
                "[startup] window geometry ready: {} ms",
                _startup.elapsed().as_millis()
            );
            let store = Arc::new(Store::open(root)?);
            #[cfg(debug_assertions)]
            eprintln!(
                "[startup] database ready: {} ms",
                _startup.elapsed().as_millis()
            );
            app.asset_protocol_scope()
                .allow_directory(store.root().join("media"), true)?;
            app.asset_protocol_scope()
                .allow_directory(store.root().join("jobs"), true)?;
            let handle = app.handle().clone();
            let tasks = TaskManager::new(
                Arc::clone(&store),
                Arc::new(move |snapshot| {
                    let _ = handle.emit("task_updated", snapshot.update_event());
                }),
            );
            tasks.recover_interrupted()?;
            #[cfg(debug_assertions)]
            let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .ok_or("Missing project directory")?
                .join(".tools/runtime");
            #[cfg(not(debug_assertions))]
            let runtime = app.path().resource_dir()?.join("tools");
            #[cfg(debug_assertions)]
            let tools = crate::media::MediaTools::development(runtime);
            #[cfg(not(debug_assertions))]
            let tools = crate::media::MediaTools::bundled(runtime);
            let defaults = Settings {
                tools,
                ..Default::default()
            };
            let application = Arc::new(Application::new(store, tasks, defaults));
            let settings = application.settings()?;
            application.start_preparation_scheduler();
            let query_handle=app.handle().clone();
            application.queries.observe(Arc::new(move|session|{let _=query_handle.emit_to("translator","query_updated",serde_json::json!({"sessionId":session.session_id,"inputRevision":session.input_revision,"queryId":session.query_id,"origin":session.input.origin.kind}));}));
            let clipboard=crate::desktop_query::ClipboardWatcher::default();clipboard.initialize(&application)?;app.manage(clipboard);
            app.manage(application);
            app.manage(NativeDesktop::default());
            app.manage(TrayAction::default());
            app.manage(crate::desktop_query::QueryWindowState::default());
            app.manage(crate::desktop_replacement::NativeReplacement::default());
            let native = app.state::<NativeDesktop>();
            let mut available = ["", "", "", ""];
            let mut failures = Vec::new();
            for (index, shortcut) in [
                settings.selection_shortcut.as_str(),
                settings.ocr_shortcut.as_str(),
                settings.input_shortcut.as_str(),
                settings.replacement_shortcut.as_str(),
            ]
            .into_iter()
            .enumerate()
            {
                available[index] = shortcut;
                if let Err(error) =
                    native.configure(app.handle(), available[0], available[1], available[2], available[3])
                {
                    available[index] = "";
                    failures.push(error.message);
                }
            }
            if !failures.is_empty() {
                native.remember_error(&AppError::new("shortcut_unavailable", failures.join("\n")));
            }
            use tauri::{
                menu::{CheckMenuItem,Menu, MenuItem, PredefinedMenuItem},
                tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
            };
            let show = MenuItem::with_id(app, "show", "打开单词本", true, None::<&str>)?;
            let collect = MenuItem::with_id(app, "collect", "收录词条", true, None::<&str>)?;
            let query = MenuItem::with_id(app,"query","输入翻译",true,None::<&str>)?;
            let replacement = MenuItem::with_id(app,"replacement","翻译替换：尚无操作（查看）",true,None::<&str>)?;
            let replacement_cancel_item = MenuItem::with_id(app,"replacement_cancel","取消翻译替换",true,None::<&str>)?;
            app.manage(crate::desktop_replacement::ReplacementMenu(replacement.clone()));
            let clipboard = CheckMenuItem::with_id(app,"clipboard","监听剪贴板",true,false,None::<&str>)?;
            app.manage(crate::desktop_query::ClipboardMenu(clipboard.clone()));
            let ocr = MenuItem::with_id(app, "ocr", "截图收录", true, None::<&str>)?;
            let import = MenuItem::with_id(app, "import", "导入剧集", true, None::<&str>)?;
            let episodes = MenuItem::with_id(app, "episodes", "观看前预习", true, None::<&str>)?;
            let practice = MenuItem::with_id(app, "practice", "听读练习", true, None::<&str>)?;
            let reviews = MenuItem::with_id(app, "reviews", "主动回忆测验", true, None::<&str>)?;
            let leech = MenuItem::with_id(app, "leech", "易忘词专项", true, None::<&str>)?;
            let sync = MenuItem::with_id(app, "sync", "站点同步", true, None::<&str>)?;
            let tasks = MenuItem::with_id(app, "tasks", "后台任务", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "本地设置", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "完全退出", true, None::<&str>)?;
            let capture_separator = PredefinedMenuItem::separator(app)?;
            let learning_separator = PredefinedMenuItem::separator(app)?;
            let settings_separator = PredefinedMenuItem::separator(app)?;
            let exit_separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(
                app,
                &[
                    &show,
                    &capture_separator,
                    &query,
                    &replacement,
                    &replacement_cancel_item,
                    &collect,
                    &ocr,
                    &import,
                    &learning_separator,
                    &episodes,
                    &practice,
                    &reviews,
                    &leech,
                    &settings_separator,
                    &sync,
                    &tasks,
                    &settings,
                    &clipboard,
                    &exit_separator,
                    &exit,
                ],
            )?;
            let icon = image::load_from_memory(include_bytes!("../icons/32x32.png"))?.into_rgba8();
            TrayIconBuilder::with_id("main-tray")
                .icon(tauri::image::Image::new_owned(icon.into_raw(), 32, 32))
                .tooltip("SubtitleVocabularyList")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } | TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        }
                    ) {
                        desktop_capture::show_main(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "replacement"=>{if let Err(error)=app.state::<crate::desktop_replacement::NativeReplacement>().open_result(app){desktop_capture::capture_failed(app,&error);}},
                    "replacement_cancel"=>app.state::<crate::desktop_replacement::NativeReplacement>().cancel(),
                    "query"=>{if let Err(error)=crate::desktop_query::open_input(app){desktop_capture::capture_failed(app,&error);}},
                    "clipboard"=>{let watcher=app.state::<crate::desktop_query::ClipboardWatcher>();let status=watcher.status();let _=watcher.set(app,!status.enabled,crate::desktop_query::ClipboardPreferences{debounce_ms:status.debounce_ms,persist_results:status.persist_results});crate::desktop_query::refresh_clipboard_menu(app);},
                    "show" => tray_navigate(app, "library"),
                    "collect" | "import" | "episodes" | "practice" | "reviews" | "leech"
                    | "sync" | "tasks" | "settings" => tray_navigate(app, event.id.as_ref()),
                    "ocr" => {
                        if let Err(error) = app.state::<NativeDesktop>().ocr_start(app) {
                            desktop_capture::capture_failed(app, &error);
                        }
                    }
                    "exit" => desktop_capture::quit(app),
                    _ => {}
                })
                .build(app)?;
            #[cfg(debug_assertions)]
            eprintln!(
                "[startup] services ready: {} ms",
                _startup.elapsed().as_millis()
            );
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label()=="translator" && let tauri::WindowEvent::CloseRequested{api,..}=event{api.prevent_close();crate::desktop_query::hide_query(window.app_handle());}
            if let Some(id) = window.label().strip_prefix("capture-")
                && matches!(event, tauri::WindowEvent::CloseRequested { .. })
            {
                window.app_handle().state::<NativeDesktop>().ocr_dismiss(id);
            }
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let app = window.app_handle();
                if app
                    .state::<Arc<Application>>()
                    .settings()
                    .is_ok_and(|settings| settings.close_to_tray)
                {
                    let _ = window.hide();
                } else {
                    desktop_capture::quit(app);
                }
            }
        })
        .invoke_handler(commands::handler())
        .run(context)
        .expect("Cannot start SubtitleVocabularyList");
}
