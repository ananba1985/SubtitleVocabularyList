//! Fixed task completion payloads. Progress and old task history remain open JSON.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
pub struct AudioPath {
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
pub struct PreviewAudio {
    pub path: PathBuf,
    pub asset: crate::vocabulary::AudioAsset,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
pub struct ReviewAudio {
    pub path: PathBuf,
    pub kind: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
pub struct SpeechAudio {
    pub path: PathBuf,
    pub kind: String,
    pub voice: crate::speech::SystemVoice,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
pub struct SyncResolution {
    pub resolved: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
pub struct SyncTaskResult {
    #[serde(flatten)]
    pub entries: crate::synchronization::SyncResult,
    pub assessment: crate::assessment_sync::AssessmentSyncStatus,
}
