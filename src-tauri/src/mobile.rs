//! Standalone Android host; it never connects to a desktop process.
use crate::{application::{Application, Settings}, store::Store, tasks::TaskManager};
use std::sync::Arc;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(crate::host::HostStartup::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(crate::mobile_platform::init())
        .setup(|app| {
            let root = app.path().app_data_dir()?.join("files");
            let store = Arc::new(Store::open(root)?);
            app.asset_protocol_scope().allow_directory(store.root().join("media"), true)?;
            app.asset_protocol_scope().allow_directory(store.root().join("jobs"), true)?;
            app.asset_protocol_scope().allow_directory(store.root().join("corpora"), true)?;
            app.asset_protocol_scope().allow_directory(store.root().join("cache/speech"), true)?;
            let handle = app.handle().clone();
            let tasks = TaskManager::new(Arc::clone(&store), Arc::new(move |snapshot| {
                let _ = handle.emit("task_updated", snapshot.update_event());
            }));
            tasks.recover_interrupted()?;
            let application = Arc::new(Application::new(store, tasks, Settings::default()));
            let handle = app.handle().clone();
            application.queries.observe(Arc::new(move |session| {
                let _ = handle.emit("query_updated", serde_json::json!({
                    "sessionId": session.session_id, "inputRevision": session.input_revision,
                    "queryId": session.query_id, "origin": session.input.origin.kind,
                }));
            }));
            // No desktop scheduler, clipboard watcher, tray or desktop model dependency.
            app.manage(application);
            app.state::<crate::host::HostStartup>().ready();
            Ok(())
        })
        .invoke_handler(crate::host::commands::handler())
        .run(tauri::generate_context!())
        .expect("Android application failed to start");
}
