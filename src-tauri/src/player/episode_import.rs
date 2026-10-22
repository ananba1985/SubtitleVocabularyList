//! Full episode preparation runs independently of the playing mpv file.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WatchImportStatus {
    pub task_id: Option<String>,
    pub state: String,
    pub message: String,
}

impl Actor {
    pub(super) fn import_current(&mut self) {
        let Some(media) = &self.media else { return };
        let path = self.state.path.clone();
        if let Some(status) = self.imports.get(&path) {
            self.state.import_status = Some(status.clone());
            return;
        }
        let status = WatchImportStatus {
            task_id: None,
            state: "checking".into(),
            message: String::new(),
        };
        self.imports.insert(path.clone(), status.clone());
        self.state.import_status = Some(status);
        let app = self.application.clone();
        let sender = self.sender.clone();
        let source = media.source_id.clone();
        std::thread::spawn(move || {
            let result = (|| {
                if app.store.watch_import_ready(source.as_deref())? {
                    return Ok(None);
                }
                let mut task =
                    app.import_start(vec![path.clone()], uuid::Uuid::new_v4().to_string())?;
                let _ = sender.send(Message::ImportStatus(
                    path.clone(),
                    WatchImportStatus {
                        task_id: Some(task.id.clone()),
                        state: "running".into(),
                        message: "正在后台导入字幕和剧集资料，可继续观看。".into(),
                    },
                ));
                while !task.terminal() {
                    std::thread::sleep(Duration::from_millis(250));
                    task = app.tasks.get(&task.id)?;
                }
                if task.state != "succeeded" {
                    return Err(task
                        .error
                        .unwrap_or_else(|| AppError::new("import_failed", task.message)));
                }
                let id = task
                    .result
                    .as_ref()
                    .and_then(|v| v["sources"][0]["id"].as_str())
                    .ok_or_else(|| AppError::new("import_failed", "导入任务未返回剧集资料。"))?;
                app.store.watch_media(Some(&path), Some(id)).map(Some)
            })();
            let _ = sender.send(Message::Imported(path, result));
        });
    }

    pub(super) fn import_finished(
        &mut self,
        path: &str,
        result: Result<Option<WatchMedia>, AppError>,
    ) {
        let status = self.imports.remove(path);
        let current = self.state.loaded && self.state.path == path;
        if current {
            self.state.import_status = None;
        }
        match result {
            Ok(Some(media)) => {
                for item in &mut self.state.playback.playlist {
                    if item.path == path {
                        item.source_id = media.source_id.clone();
                        item.title = media.title.clone();
                    }
                }
                if current
                    && self
                        .media
                        .as_ref()
                        .is_some_and(|m| m.media_sha256 == media.media_sha256)
                {
                    let refresh = self.user_pause
                        && self.engine.flag("pause")
                        && self
                            .state
                            .context
                            .as_ref()
                            .is_none_or(|c| c.translation.is_empty());
                    self.bind_media(media);
                    // Keep an already displayed sentence and its collection timer intact.
                    // Only retry a paused lookup which is still missing the imported text.
                    if refresh {
                        self.invalidate();
                        self.user_pause = true;
                        self.pending_context = Some(Instant::now() + Duration::from_millis(60));
                    }
                }
                let _ = self
                    .application
                    .store
                    .save_playback_state(&self.state.playback);
                let _ = self.handle.emit_to("main", "watch_sources_updated", ());
            }
            Ok(None) => {}
            Err(error) if current => {
                self.state.import_status = Some(WatchImportStatus {
                    task_id: status.and_then(|s| s.task_id),
                    state: "failed".into(),
                    message: format!("后台导入未完成：{} 可重新打开视频重试。", error.message),
                });
            }
            Err(_) => {} // The background task retains the error for a video no longer playing.
        }
    }
}
