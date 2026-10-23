//! Small platform playback contract; only app-owned audio can be opened here.
use crate::{error::AppError, store::Store};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ClipAudioState {
    pub session_id: String,
    pub active: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
    pub played: bool,
    pub ended: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ClipAudioAction { Play, Pause, Seek, Close }

pub(crate) fn open(store: &Store, path: &str) -> Result<ClipAudioState, AppError> {
    let path = std::path::Path::new(path).canonicalize()?;
    let root = store.root().canonicalize()?;
    if !path.is_file() || !(path.starts_with(root.join("media")) || path.starts_with(root.join("jobs"))) {
        return Err(AppError::new("invalid_input", "只能播放本机已保存的原声或生成语音。"));
    }
    invoke("audioOpen", serde_json::json!({ "path": path, "sessionId": uuid::Uuid::new_v4().to_string() }))
}

pub(crate) fn state(session_id: &str) -> Result<ClipAudioState, AppError> {
    invoke("audioState", serde_json::json!({"sessionId": session_id}))
}

pub(crate) fn control(session_id: &str, action: ClipAudioAction, position_ms: Option<u64>) -> Result<ClipAudioState, AppError> {
    invoke("audioControl", serde_json::json!({"sessionId": session_id, "action": action, "positionMs": position_ms}))
}

fn invoke(command: &str, args: serde_json::Value) -> Result<ClipAudioState, AppError> {
    #[cfg(all(target_os = "android", feature = "mobile"))]
    { return crate::mobile_platform::audio_invoke(command, args); }
    #[cfg(not(all(target_os = "android", feature = "mobile")))]
    { let _ = (command, args); Err(AppError::new("provider_unavailable", "此播放接口用于 Android 客户端。")) }
}
