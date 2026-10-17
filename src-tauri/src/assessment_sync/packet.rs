//! Complete packet facts and validation of trusted projections or incoming data.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionFacts {
    pub id: String,
    pub state: String,
    pub revision: i64,
    pub config: AssessmentConfig,
    pub sampling: SamplingSnapshot,
    pub position: usize,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemFacts {
    pub id: String,
    pub position: usize,
    pub snapshot: FrozenItem,
    pub interaction_json: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnswerFacts {
    pub answer: AssessmentAnswer,
    pub revision: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportFacts {
    pub digest: String,
    pub report_json: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionLink {
    pub session_item_id: String,
    pub operation_id: String,
    pub entry_id: String,
    pub details: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaReference {
    pub digest: String,
    pub format: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssessmentPacket {
    pub protocol: String,
    pub schema_version: u32,
    pub origin_device_id: String,
    pub session: SessionFacts,
    pub items: Vec<ItemFacts>,
    pub answers: Vec<AnswerFacts>,
    pub corrections: Vec<AssessmentCorrection>,
    pub reports: Vec<ReportFacts>,
    pub collection_links: Vec<CollectionLink>,
    pub media: Vec<MediaReference>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssessmentPush {
    pub change_id: String,
    pub base_revision: i64,
    pub bundle: AssessmentPacket,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentDocument {
    pub session_id: String,
    pub revision: i64,
    pub bundle: AssessmentPacket,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentSyncStatus {
    pub capability: Option<String>,
    pub pending: usize,
    pub conflicts: usize,
    pub pending_references: usize,
    pub cursor: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct KeepAssessmentBoth {
    pub session_id: String,
    pub expected_local_revision: i64,
    pub expected_remote_revision: i64,
    pub operation_id: String,
}

pub(super) fn invalid(message: &str) -> AppError {
    AppError::new("invalid_data", message)
}
pub(super) fn uuid(s: &str) -> bool {
    Uuid::parse_str(s).is_ok()
}
pub(super) fn sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
impl AssessmentPacket {
    pub fn validate(&self) -> Result<(), AppError> {
        self.validate_projection(None, &HashSet::new())
    }

    // Only locally built projections can reuse previously validated immutable parts.
    // Incoming packets always use validate(), and all cross-record checks still run.
    pub(super) fn validate_projection(
        &self,
        previous: Option<&PreparedPacket>,
        reused_items: &HashSet<usize>,
    ) -> Result<(), AppError> {
        let s = &self.session;
        s.config.validate()?;
        if self.protocol != "assessment-1"
            || self.schema_version != 1
            || !uuid(&s.id)
            || !uuid(&self.origin_device_id)
            || s.revision < 1
            || !matches!(
                s.state.as_str(),
                "active" | "paused" | "completed" | "ended_early"
            )
            || self.items.len() != s.config.question_count
            || s.position > self.items.len()
        {
            return Err(invalid("水平测试会话或题量无效。"));
        }
        let mut items = HashSet::new();
        let mut positions = HashSet::new();
        let mut targets = HashSet::new();
        for i in &self.items {
            let interaction: Interaction = serde_json::from_str(&i.interaction_json)?;
            if !interaction.audio_seconds.is_finite() || interaction.audio_seconds < 0.0 {
                return Err(invalid("测试播放状态无效。"));
            }
            if !reused_items.contains(&i.position) {
                let _timing = crate::local_profile::phase("assessment.item_validate");
                i.snapshot.content.validate(
                    &i.snapshot.question_type,
                    false,
                    std::path::Path::new(""),
                )?;
            }
            if !uuid(&i.id)
                || !items.insert(&i.id)
                || !positions.insert(i.position)
                || i.position >= self.items.len()
                || !targets.insert(format!("{}:{}", i.snapshot.kind, i.snapshot.lexeme_key))
            {
                return Err(invalid("测试位置、题目或词元重复。"));
            }
            if let Some(a) = &i.snapshot.content.audio {
                if !a.relative_path.is_empty() || a.asset_id.is_some() || !sha(&a.sha256) {
                    return Err(invalid("测试音频描述包含本机路径或无效摘要。"));
                }
            }
        }
        let mut answers = HashSet::new();
        let mut answered_items = HashSet::new();
        for a in &self.answers {
            if !uuid(&a.answer.id)
                || !answers.insert(&a.answer.id)
                || !answered_items.insert(&a.answer.session_item_id)
                || !items.contains(&a.answer.session_item_id)
                || a.answer.session_id != s.id
                || a.revision < 1
                || !matches!(
                    a.answer.result.as_str(),
                    "correct" | "incorrect" | "unknown" | "needs_confirmation"
                )
            {
                return Err(invalid("测试原作答或题目关联无效。"));
            }
        }
        if self.answers.len() != s.position
            || (s.state == "completed" && s.position != self.items.len())
            || self
                .items
                .iter()
                .any(|i| (i.position < s.position) != answered_items.contains(&i.id))
        {
            return Err(invalid("测试进度与实际作答不一致。"));
        }
        let mut corrections = HashSet::new();
        let mut corrected_revisions = HashSet::new();
        for a in &self.corrections {
            if !uuid(&a.id)
                || !answers.contains(&a.answer_id)
                || !corrections.insert(&a.id)
                || !corrected_revisions.insert((&a.answer_id, a.revision))
                || a.revision < 2
                || !matches!(
                    a.result.as_str(),
                    "correct" | "incorrect" | "unknown" | "invalid"
                )
                || a.reason.trim().is_empty()
            {
                return Err(invalid("测试修正或关联无效。"));
            }
        }
        for a in &self.answers {
            if a.revision
                != self
                    .corrections
                    .iter()
                    .filter(|r| r.answer_id == a.answer.id)
                    .map(|r| r.revision)
                    .max()
                    .unwrap_or(1)
            {
                return Err(invalid("测试答案修订与修正事件不一致。"));
            }
        }
        let mut reports = HashSet::new();
        let old_reports = previous
            .filter(|p| p.id == s.id)
            .map(|p| {
                p.packet
                    .reports
                    .iter()
                    .map(|r| (r.digest.as_str(), r.report_json.as_str()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        for r in &self.reports {
            if !reports.insert(&r.digest) {
                return Err(invalid("测试报告摘要或会话关联无效。"));
            }
            if old_reports
                .get(r.digest.as_str())
                .is_some_and(|raw| *raw == r.report_json)
            {
                let _timing = crate::local_profile::phase("assessment.report_validate_reuse");
                continue;
            }
            let _timing = crate::local_profile::phase("assessment.report_validate");
            let report: AssessmentReport = serde_json::from_str(&r.report_json)?;
            if report.session_id != s.id || digest(r.report_json.as_bytes()) != r.digest {
                return Err(invalid("测试报告摘要或会话关联无效。"));
            }
        }
        let mut links = HashSet::new();
        for l in &self.collection_links {
            if !items.contains(&l.session_item_id)
                || !uuid(&l.operation_id)
                || !uuid(&l.entry_id)
                || !links.insert((&l.session_item_id, &l.operation_id))
            {
                return Err(invalid("测试收录关联无效。"));
            }
        }
        if self.media.iter().any(|m| {
            !sha(&m.digest)
                || !["wav", "m4a", "mp3", "ogg", "flac", "webm"].contains(&m.format.as_str())
        }) {
            return Err(invalid("测试原声清单无效。"));
        }
        Ok(())
    }
}
