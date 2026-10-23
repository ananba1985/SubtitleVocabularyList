//! Foreground playback orchestration; timing, collection and review facts remain shared.
use crate::{application::Application, error::AppError, mobile_video, tasks::TaskSnapshot, watch::{DisplayClock, WatchContext}};
use std::{sync::{Arc, Mutex, OnceLock}, time::Instant};

#[derive(Clone, Debug, serde::Serialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct VideoLearning {
    pub session_id: String,
    pub context: Option<WatchContext>,
    pub eligible: bool,
    pub remaining_seconds: Option<f64>,
    pub collected: bool,
    pub skipped: bool,
    pub task: Option<TaskSnapshot>,
}

#[derive(Default)]
struct PauseSession {
    key: String,
    revision: u64,
    context: Option<WatchContext>,
    eligible: bool,
    skipped: bool,
    clock: DisplayClock,
    task_id: Option<String>,
}
fn session() -> &'static Mutex<PauseSession> {
    static SESSION: OnceLock<Mutex<PauseSession>> = OnceLock::new();
    SESSION.get_or_init(||Mutex::new(PauseSession::default()))
}

impl Application {
    pub(crate) fn video_learning(&self) -> Result<VideoLearning, AppError> {
        let state = mobile_video::state(&self.store)?;
        let verified: Option<String> = mobile_video::setting(&self.store,&mobile_video::key("verified",&state.uri))?;
        let key = format!("{}:{}:{}:{}:{}",state.session_id,state.revision,state.paused,state.foreground,verified.as_deref().unwrap_or(""));
        let settings = self.settings()?;
        let mut session = session().lock().map_err(|_|AppError::new("internal_error","暂停学习状态暂不可用。"))?;
        if session.key != key {
            let context = if state.paused && state.foreground && verified.as_deref() == Some(&state.session_id) { mobile_video::context(&self.store,&state)? } else { None };
            session.key = key; session.revision += 1; session.clock.cancel(); session.skipped = false; session.task_id = None;
            session.context = context.map(|mut c| { c.revision = session.revision; c });
        }
        session.eligible = state.user_paused && state.paused && state.foreground && state.window_focused && !state.fullscreen && state.error.is_none()
            && settings.watch_auto_collect && session.context.as_ref().is_some_and(|c|!c.translation.trim().is_empty());
        if !session.eligible || session.skipped {
            if (!state.window_focused || state.fullscreen) && session.clock.remaining(session.revision,Instant::now(),settings.watch_collect_delay_seconds).is_some() {session.skipped=true;}
            session.clock.cancel();
        }
        let collected = session.context.as_ref().map(|c|self.store.watch_receipt(c)).transpose()?.flatten().is_some();
        if collected { session.clock.cancel(); }
        let remaining = session.clock.remaining(session.revision,Instant::now(),settings.watch_collect_delay_seconds);
        if remaining.is_some_and(|n|n <= 0.0) && session.eligible && !session.skipped && !collected && session.task_id.is_none() {
            let context = session.context.clone().unwrap();
            let state_uri=state.uri.clone();
            let store = Arc::clone(&self.store); let tools = settings.tools;
            let hash = crate::vocabulary::digest(serde_json::to_string(&context)?.as_bytes());
            let task = self.tasks.start_typed("watch_collect",&uuid::Uuid::new_v4().to_string(),&hash,move |task| {
                task.subject(&context.text); task.progress("audio",0,1,"正在保存暂停对白与独立原声"); task.check_cancelled()?;
                let title=store.playback_state()?.playlist.iter().find(|i|i.path==state_uri).map(|i|i.title.clone()).unwrap_or_else(||"本地视频".into());
                let context=store.bind_watch_context_verified(&state_uri,&title,&context,&context.media_sha256)?;
                let asset = if context.precise_timing && context.audio_stream.is_some() {
                    Some(store.ensure_clip(&tools,&context.source_id,&context.example_id,&task.cancelled)?)
                } else { None };
                task.check_cancelled()?;
                store.watch_collect(&context,asset.as_ref().map(|a|a.id.as_str()))
            })?;
            session.task_id = Some(task.id.clone()); session.clock.cancel();
        }
        Ok(VideoLearning { session_id:state.session_id,context:session.context.clone(),eligible:session.eligible,
            remaining_seconds:session.clock.remaining(session.revision,Instant::now(),settings.watch_collect_delay_seconds),
            collected,skipped:session.skipped,task:session.task_id.as_deref().map(|id|self.tasks.get(id)).transpose()? })
    }

    pub(crate) fn video_learning_visible(&self, revision: u64, visible: bool) -> Result<(), AppError> {
        if !visible {
            let mut session = session().lock().map_err(|_|AppError::new("internal_error","暂停学习状态暂不可用。"))?;
            if session.context.as_ref().is_some_and(|c|c.revision == revision) {
                if session.clock.remaining(session.revision,Instant::now(),self.settings()?.watch_collect_delay_seconds).is_some() {session.skipped=true;}
                session.clock.cancel();
            }
            return Ok(());
        }
        if self.video_learning()?.collected { return Ok(()); }
        let mut session = session().lock().map_err(|_|AppError::new("internal_error","暂停学习状态暂不可用。"))?;
        if session.context.as_ref().is_some_and(|c|c.revision == revision) {
            if visible && session.eligible && !session.skipped && session.task_id.is_none() { session.clock.arm(revision,Instant::now()); }
            else { session.clock.cancel(); }
        }
        Ok(())
    }

    pub(crate) fn video_learning_skip(&self, revision: u64) -> Result<(), AppError> {
        self.video_learning()?;
        let mut session = session().lock().map_err(|_|AppError::new("internal_error","暂停学习状态暂不可用。"))?;
        if session.context.as_ref().is_some_and(|c|c.revision == revision) {
            session.skipped = true; session.clock.cancel();
            if let Some(id) = session.task_id.as_deref() { let _ = self.tasks.cancel(id); }
        }
        Ok(())
    }
}
