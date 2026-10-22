//! Runtime inbox: startup/second-instance arguments and both native/WebView drops.
use crate::{error::AppError, playback::file_arguments};
use std::{collections::VecDeque, path::Path, sync::Mutex};
use tauri::{Emitter, Manager};

pub struct FileRequest {
    pub paths: Vec<String>,
    pub activate: bool,
}
#[derive(Default)]
pub struct FileInbox(Mutex<VecDeque<FileRequest>>);
impl FileInbox {
    pub fn push(&self, paths: Vec<String>, activate: bool) {
        if !paths.is_empty()
            && let Ok(mut queue) = self.0.lock()
        {
            queue.push_back(FileRequest { paths, activate });
        }
    }
    pub fn pending(&self) -> bool {
        self.0.lock().is_ok_and(|queue| !queue.is_empty())
    }
    pub fn take(&self) -> Result<Option<FileRequest>, AppError> {
        self.0
            .lock()
            .map(|mut queue| queue.pop_front())
            .map_err(|_| AppError::new("internal_error", "文件打开队列不可用。"))
    }
}
pub fn receive(app: &tauri::AppHandle, paths: Vec<String>, activate: bool) {
    if paths.is_empty() {
        return;
    }
    app.state::<FileInbox>().push(paths, activate);
    crate::desktop_capture::show_main(app);
    let _ = app.emit_to("main", "watch_files_pending", ());
}
pub fn receive_arguments(app: &tauri::AppHandle, args: Vec<String>, cwd: &str) {
    receive(
        app,
        file_arguments(args.into_iter().skip(1), Path::new(cwd)),
        true,
    );
}
