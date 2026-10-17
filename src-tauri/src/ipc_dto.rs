//! Explicit views of legacy JSON projections at the desktop IPC boundary.
//! The core's stored receipts and sync packets retain their existing representation.
use crate::{
    assessment::AssessmentCorrection,
    assessment_bank::AssessmentConfig,
    assessment_reports::{Counts, ReportGroup},
    assessment_sampling::AssessmentCoverage,
    error::AppError,
    vocabulary::Entry,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn decode<T: DeserializeOwned>(value: Value) -> Result<T, AppError> {
    serde_json::from_value(value).map_err(|error| {
        AppError::new(
            "internal_error",
            format!("命令返回结构与接口契约不一致：{error}"),
        )
    })
}

pub(crate) fn project<T: DeserializeOwned + Serialize>(
    value: impl Serialize,
) -> Result<T, AppError> {
    let value = serde_json::to_value(value)?;
    let reply: T = decode(value.clone())?;
    if serde_json::to_value(&reply)? != value {
        return Err(AppError::new(
            "internal_error",
            "命令返回字段与接口契约不一致。",
        ));
    }
    Ok(reply)
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentSessionSummary {
    pub id: String,
    pub state: String,
    pub revision: i64,
    pub position: usize,
    pub config: AssessmentConfig,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentAudioReply {
    pub playback_id: String,
    pub played: bool,
    pub seconds: f64,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentCorrectionReply {
    pub answer_id: String,
    pub revision: i64,
    pub session_id: String,
    pub session_revision: i64,
    pub correction: AssessmentCorrection,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentReportSummary {
    pub session_id: String,
    pub revision: i64,
    pub state: String,
    pub status: String,
    pub created_at: i64,
    pub generated_at: i64,
    pub score_policy: String,
    pub config: AssessmentConfig,
    pub comparison_key: String,
    pub counts: Counts,
    pub by_content: Vec<ReportGroup>,
    pub by_band: Vec<ReportGroup>,
    pub by_type: Vec<ReportGroup>,
    pub by_dimension: Vec<ReportGroup>,
    pub details: Vec<ReportGroup>,
    pub coverage: Vec<AssessmentCoverage>,
    pub repeated_targets: usize,
    pub warnings: Vec<String>,
    pub item_total: usize,
    pub bank_versions: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum AssessmentReportReply {
    Finished {
        report: AssessmentReportSummary,
        revisions: Vec<i64>,
    },
    Pending(AssessmentPendingReport),
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentPendingReport {
    pub session_id: String,
    pub state: String,
    pub position: usize,
    pub planned: usize,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentHistoryItem {
    pub id: String,
    pub state: String,
    pub revision: i64,
    pub created_at: i64,
    pub config: AssessmentConfig,
    pub counts: Counts,
    pub status: String,
    pub comparison_key: String,
    pub repeated_targets: usize,
    pub preserved_copy: bool,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentComparison {
    pub first: AssessmentReportSummary,
    pub second: AssessmentReportSummary,
    pub comparable: bool,
    pub explanation: String,
    pub correct_rate_delta: Option<f64>,
    pub independent_rate_delta: Option<f64>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum BankReviewReply {
    Published {
        #[serde(rename = "bankId")]
        bank_id: String,
        state: String,
        revision: i64,
        #[serde(rename = "publishedItems")]
        published_items: usize,
    },
    Item {
        #[serde(rename = "bankId")]
        bank_id: String,
        #[serde(rename = "itemId")]
        item_id: String,
        state: String,
        revision: i64,
        #[serde(rename = "itemRevision")]
        item_revision: i64,
    },
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentConflictSide {
    pub state: String,
    pub position: usize,
    pub planned: usize,
    pub answers: usize,
    pub corrections: usize,
    pub reports: usize,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentSyncConflict {
    pub session_id: String,
    pub reason: String,
    pub local_revision: i64,
    pub remote_revision: i64,
    pub local: AssessmentConflictSide,
    pub remote: AssessmentConflictSide,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentKeepBothReply {
    pub session_id: String,
    pub copy_session_id: String,
    pub archive_id: String,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
pub struct SyncEntryView {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub revision: i64,
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SyncBundleView {
    pub schema_version: u32,
    pub deleted: bool,
    pub entry: SyncEntryView,
    pub tables: BTreeMap<String, Vec<BTreeMap<String, Value>>>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SyncConflictView {
    pub id: String,
    pub remote_id: String,
    pub local_id: Option<String>,
    pub text: String,
    pub reason: String,
    pub remote_revision: i64,
    pub local: Option<SyncBundleView>,
    pub remote: SyncBundleView,
    pub candidates: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ScreenSessionView {
    pub id: String,
    pub bounds: crate::ocr::ScreenBounds,
    pub image_path: std::path::PathBuf,
}
