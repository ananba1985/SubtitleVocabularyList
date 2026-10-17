//! Shared typed progress payload; no task or learning history is discarded.
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SourceProgress {
    pub(super) source_id: String,
    pub(super) title: String,
    pub(super) current: usize,
    pub(super) total: usize,
}

#[derive(Default, Serialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PreparationProgress {
    pub(super) unit: &'static str,
    pub(super) words_total: usize,
    pub(super) words_current: usize,
    pub(super) sentences_total: usize,
    pub(super) sentences_current: usize,
    pub(super) generated: usize,
    pub(super) reused: usize,
    pub(super) skipped: usize,
    pub(super) failed: usize,
    pub(super) sources: Vec<SourceProgress>,
    pub(super) failures: Vec<PreparationFailure>,
    pub(super) concurrency: usize,
    pub(super) in_flight: usize,
    pub(super) waiting: usize,
    pub(super) batches_in_flight: usize,
    pub(super) batch_size: usize,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
pub struct PreparationFailure {
    pub(super) source: String,
    pub(super) text: String,
    pub(super) message: String,
}
