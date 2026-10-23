//! Shared speech contract and platform dispatch. Cache and review rules stay in Application.
use crate::{error::AppError, tasks::TaskContext};
use std::path::Path;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SystemVoice {
    pub id: String,
    pub name: String,
}

pub fn voices() -> Result<Vec<SystemVoice>, AppError> {
    #[cfg(all(windows, feature = "desktop"))]
    { return crate::windows_native::system_voices(); }
    #[cfg(all(target_os = "android", feature = "mobile"))]
    { return crate::mobile_platform::speech_voices(); }
    #[allow(unreachable_code)]
    Err(AppError::new("provider_unavailable", "当前平台没有本地英语声音接口。"))
}

pub(crate) fn synthesize(text: &str, voice: &str, path: &Path, context: &TaskContext) -> Result<(), AppError> {
    context.check_cancelled()?;
    #[cfg(all(windows, feature = "desktop"))]
    crate::windows_native::synthesize(text, voice, path, &context.cancelled)?;
    #[cfg(all(target_os = "android", feature = "mobile"))]
    crate::mobile_platform::synthesize(text, voice, path, &context.task_id)?;
    context.check_cancelled()
}
