//! Complete session packets, independent from the entry synchronization cursor.
use crate::{
    assessment::{
        AssessmentAnswer, AssessmentCorrection, FrozenItem, Interaction, SamplingSnapshot,
        load_session,
    },
    assessment_bank::AssessmentConfig,
    assessment_reports::{AssessmentReport, PreparedReport, prepare_report, report_on},
    error::AppError,
    store::Store,
    vocabulary::digest,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

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
pub(crate) struct PreparedPacket {
    scope: String,
    id: String,
    epoch: i64,
    packet: AssessmentPacket,
    hash: String,
    inputs: ProjectionInputs,
}
#[derive(Default)]
struct ProjectionInputs {
    items: Vec<ItemInput>,
    reports: BTreeMap<i64, ReportInput>,
}
struct ItemInput {
    digest: String,
    media: Option<MediaReference>,
}
#[derive(Clone)]
struct ReportInput {
    digest: String,
    projected_digest: String,
    aliases: BTreeMap<String, Option<String>>,
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

fn preserved_copy(packet: &AssessmentPacket, archive: &str) -> Result<AssessmentPacket, AppError> {
    let mut copy = packet.clone();
    let parent = copy.session.id.clone();
    let new_session = Uuid::new_v4().to_string();
    let items = copy
        .items
        .iter()
        .map(|i| (i.id.clone(), Uuid::new_v4().to_string()))
        .collect::<BTreeMap<_, _>>();
    let answers = copy
        .answers
        .iter()
        .map(|a| (a.answer.id.clone(), Uuid::new_v4().to_string()))
        .collect::<BTreeMap<_, _>>();
    let corrections = copy
        .corrections
        .iter()
        .map(|a| (a.id.clone(), Uuid::new_v4().to_string()))
        .collect::<BTreeMap<_, _>>();
    let notice = format!(
        "保留副本：来自并发会话 {parent}，原始两份资料存档 {archive}；此副本用于保留记录，不代表一次新测试或独立复测。"
    );
    let answer = |a: &mut AssessmentAnswer| {
        a.id = answers[&a.id].clone();
        a.session_id = new_session.clone();
        a.session_item_id = items[&a.session_item_id].clone();
    };
    let correction = |c: &mut AssessmentCorrection| {
        c.id = corrections[&c.id].clone();
        c.answer_id = answers[&c.answer_id].clone();
    };
    copy.session.id = new_session.clone();
    copy.session.sampling.warnings.push(notice.clone());
    for i in &mut copy.items {
        i.id = items[&i.id].clone();
    }
    for a in &mut copy.answers {
        answer(&mut a.answer);
    }
    for c in &mut copy.corrections {
        correction(c);
    }
    for link in &mut copy.collection_links {
        link.session_item_id = items[&link.session_item_id].clone();
        if let Some(details) = link.details.as_object_mut() {
            details.insert("sessionId".into(), json!(new_session));
        }
    }
    for value in &mut copy.reports {
        let mut r: AssessmentReport = serde_json::from_str(&value.report_json)?;
        r.session_id = new_session.clone();
        r.warnings.push(notice.clone());
        for row in &mut r.items {
            row.id = items[&row.id].clone();
            if let Some(a) = &mut row.answer {
                answer(a);
            }
            if let Some(a) = &mut row.original_answer {
                answer(a);
            }
            for c in &mut row.corrections {
                correction(c);
            }
        }
        value.report_json = canonical(&r)?;
        value.digest = digest(value.report_json.as_bytes());
    }
    copy.reports.sort_by(|a, b| a.digest.cmp(&b.digest));
    copy.validate()?;
    Ok(copy)
}
fn invalid(message: &str) -> AppError {
    AppError::new("invalid_data", message)
}
fn uuid(s: &str) -> bool {
    Uuid::parse_str(s).is_ok()
}
fn sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn canonical<T: Serialize>(v: &T) -> Result<String, AppError> {
    Ok(serde_json::to_string(&serde_json::to_value(v)?)?)
}
pub(crate) fn packet_digest(v: &AssessmentPacket) -> Result<String, AppError> {
    let _timing = crate::local_profile::phase("assessment.digest");
    Ok(digest(canonical(v)?.as_bytes()))
}
// Physical cache addresses are local; the semantic and original recording descriptors are frozen.
fn without_paths(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        if o.contains_key("relativePath") && o.contains_key("sha256") && o.contains_key("voiceId") {
            o.insert("relativePath".into(), json!(""));
            o.insert("assetId".into(), Value::Null);
        }
        for v in o.values_mut() {
            without_paths(v);
        }
    } else if let Some(a) = v.as_array_mut() {
        for v in a {
            without_paths(v);
        }
    }
}
fn portable<T: Serialize + for<'a> Deserialize<'a>>(value: &T) -> Result<T, AppError> {
    let mut v = serde_json::to_value(value)?;
    without_paths(&mut v);
    Ok(serde_json::from_value(v)?)
}
fn entry_alias(
    c: &Connection,
    scope: &str,
    id: &str,
    outbound: bool,
) -> Result<Option<String>, AppError> {
    let (select, filter) = if outbound {
        ("remote_id", "local_id")
    } else {
        ("local_id", "remote_id")
    };
    Ok(c.query_row(&format!("SELECT {select} FROM sync_links WHERE account_scope=? AND entity_kind='entries' AND {filter}=? ORDER BY {select} LIMIT 1"),params![scope,id],|r|r.get(0)).optional()?)
}
fn rewrite_entry_references(
    c: &Connection,
    scope: &str,
    v: &mut Value,
    outbound: bool,
) -> Result<(), AppError> {
    rewrite_references(v, &mut |id| entry_alias(c, scope, id, outbound))
}

fn rewrite_references(
    v: &mut Value,
    alias: &mut impl FnMut(&str) -> Result<Option<String>, AppError>,
) -> Result<(), AppError> {
    if let Some(o) = v.as_object_mut() {
        for (k, v) in o.iter_mut() {
            if matches!(k.as_str(), "entryId" | "entry_id") {
                if let Some(id) = v.as_str() {
                    if let Some(mapped) = alias(id)? {
                        *v = json!(mapped);
                    }
                }
            } else if k == "collectedEntryIds" {
                if let Some(a) = v.as_array_mut() {
                    for id in a {
                        if let Some(text) = id.as_str() {
                            if let Some(mapped) = alias(text)? {
                                *id = json!(mapped);
                            }
                        }
                    }
                }
            } else {
                rewrite_references(v, alias)?;
            }
        }
    } else if let Some(a) = v.as_array_mut() {
        for v in a {
            rewrite_references(v, alias)?;
        }
    }
    Ok(())
}
impl AssessmentPacket {
    pub fn validate(&self) -> Result<(), AppError> {
        self.validate_projection(None, &HashSet::new())
    }

    // Only locally built projections can reuse previously validated immutable parts.
    // Incoming packets always use validate(), and all cross-record checks still run.
    fn validate_projection(
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

pub(crate) fn export_on(
    c: &Connection,
    _root: &std::path::Path,
    scope: &str,
    id: &str,
) -> Result<AssessmentPacket, AppError> {
    let base = load_base(c, scope, id)?;
    let mut settings = Vec::new();
    let (packet, _) = export_with_base(c, scope, id, base.as_ref(), None, None, &mut settings)?;
    write_settings(c, &settings)?;
    Ok(packet)
}

fn load_base(
    c: &Connection,
    scope: &str,
    id: &str,
) -> Result<Option<AssessmentDocument>, AppError> {
    let _timing = crate::local_profile::phase("assessment.base_decode");
    let raw: Option<String> = c
        .query_row(
            "SELECT base_json FROM assessment_sync_state WHERE account_key=? AND session_id=?",
            params![scope, id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(raw
        .as_deref()
        .map(serde_json::from_str::<AssessmentDocument>)
        .transpose()?)
}

fn write_settings(c: &Connection, settings: &[(String, String)]) -> Result<(), AppError> {
    let mut statement = c.prepare("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json WHERE settings.value_json IS NOT excluded.value_json")?;
    for (key, value) in settings {
        statement.execute(params![key, value])?;
    }
    Ok(())
}

fn export_with_base(
    c: &Connection,
    scope: &str,
    id: &str,
    base: Option<&AssessmentDocument>,
    prepared_report: Option<&PreparedReport>,
    previous: Option<&PreparedPacket>,
    settings: &mut Vec<(String, String)>,
) -> Result<(AssessmentPacket, ProjectionInputs), AppError> {
    let _timing = crate::local_profile::phase("assessment.export");
    let previous = previous.filter(|p| p.scope == scope && p.id == id);
    let old_reports = previous
        .map(|p| {
            p.packet
                .reports
                .iter()
                .map(|r| (r.digest.as_str(), r))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut inputs = ProjectionInputs::default();
    let mut reused_items = HashSet::new();
    let s = load_session(c, id)?;
    let origin = if let Some(base) = base {
        base.bundle.origin_device_id.clone()
    } else {
        let device: Option<String> = c
            .query_row(
                "SELECT value_json FROM settings WHERE key='assessment_device_v1'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let device = if let Some(device) = device {
            serde_json::from_str::<String>(&device)?
        } else {
            let v = Uuid::new_v4().to_string();
            settings.push(("assessment_device_v1".into(), canonical(&v)?));
            v
        };
        c.query_row(
            "SELECT value_json FROM settings WHERE key=?",
            [format!("assessment_origin:{}", s.id)],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .map(|v| serde_json::from_str::<String>(&v))
        .transpose()?
        .unwrap_or(device)
    };
    let mut q=c.prepare("SELECT id,position,snapshot_json,interaction_json FROM assessment_session_items WHERE session_id=? ORDER BY position")?;
    let rows = q
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, usize>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut items = Vec::new();
    let mut media = std::collections::BTreeSet::new();
    for (id, position, snapshot, interaction) in rows {
        let source_digest = digest(snapshot.as_bytes());
        let cached = previous
            .and_then(|p| {
                p.packet
                    .items
                    .get(position)
                    .zip(p.inputs.items.get(position))
            })
            .filter(|(item, input)| {
                item.id == id && item.position == position && input.digest == source_digest
            });
        let (snapshot, reference) = if let Some((item, input)) = cached {
            let _timing = crate::local_profile::phase("assessment.item_reuse");
            reused_items.insert(position);
            (item.snapshot.clone(), input.media.clone())
        } else {
            let _timing = crate::local_profile::phase("assessment.item_project");
            let snapshot: FrozenItem = serde_json::from_str(&snapshot)?;
            let reference = snapshot
                .content
                .audio
                .as_ref()
                .filter(|a| a.kind == "original")
                .map(|audio| MediaReference {
                    digest: audio.sha256.clone(),
                    format: std::path::Path::new(&audio.relative_path)
                        .extension()
                        .and_then(|v| v.to_str())
                        .unwrap_or("wav")
                        .to_owned(),
                });
            (portable(&snapshot)?, reference)
        };
        inputs.items.push(ItemInput {
            digest: source_digest,
            media: reference.clone(),
        });
        if let Some(reference) = reference {
            media.insert(reference);
        }
        items.push(ItemFacts {
            id,
            position,
            snapshot,
            interaction_json: interaction,
        });
    }
    let mut q = c.prepare(
        "SELECT answer_json,revision FROM assessment_answers WHERE session_id=? ORDER BY id",
    )?;
    let answers = q
        .query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .map(|r| {
            let (t, revision) = r?;
            Ok(AnswerFacts {
                answer: serde_json::from_str(&t)?,
                revision,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let mut q=c.prepare("SELECT correction_json FROM assessment_corrections WHERE answer_id IN (SELECT id FROM assessment_answers WHERE session_id=?) ORDER BY id")?;
    let corrections = q
        .query_map([id], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str::<AssessmentCorrection>(&r?)?))
        .collect::<Result<Vec<_>, AppError>>()?;
    let mut q=c.prepare("SELECT report_json,revision FROM assessment_report_versions WHERE session_id=? ORDER BY created_at,revision")?;
    let mut reports = BTreeMap::new();
    let mut retained_by_slot = BTreeMap::new();
    if let Some(base) = &base {
        for report in &base.bundle.reports {
            let slot: Option<String> = c
                .query_row(
                    "SELECT value_json FROM settings WHERE key=?",
                    [report_slot_key(id, &report.digest)],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(slot) = slot {
                retained_by_slot.entry(slot).or_insert(report);
            }
        }
    }
    let new_report = prepared_report.and_then(|report| {
        report
            .new_json
            .as_ref()
            .map(|raw| Ok((raw.clone(), report.report.revision)))
    });
    // Include the pending immutable report in the complete projection without writing it yet.
    for row in q
        .query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .chain(new_report)
    {
        let (raw, slot) = row?;
        if let Some(report) = retained_by_slot.get(&slot.to_string()) {
            let _timing = crate::local_profile::phase("assessment.report_reuse");
            reports.insert(report.digest.clone(), (*report).clone());
            continue;
        }
        let source_digest = digest(raw.as_bytes());
        if let Some(input) = previous.and_then(|p| p.inputs.reports.get(&slot)) {
            let mut unchanged = input.digest == source_digest;
            if unchanged {
                for (entry, mapped) in &input.aliases {
                    if entry_alias(c, scope, entry, true)? != *mapped {
                        unchanged = false;
                        break;
                    }
                }
            }
            if unchanged {
                if let Some(report) = old_reports.get(input.projected_digest.as_str()) {
                    let _timing = crate::local_profile::phase("assessment.report_reuse");
                    settings.push((report_slot_key(id, &report.digest), slot.to_string()));
                    reports.insert(report.digest.clone(), (*report).clone());
                    inputs.reports.insert(slot, input.clone());
                    continue;
                }
            }
        }
        let _timing = crate::local_profile::phase("assessment.report_project");
        let report: AssessmentReport = portable(&serde_json::from_str::<AssessmentReport>(&raw)?)?;
        let mut v = serde_json::to_value(report)?;
        let mut aliases: BTreeMap<String, Option<String>> = BTreeMap::new();
        rewrite_references(&mut v, &mut |entry| {
            if let Some(mapped) = aliases.get(entry) {
                return Ok(mapped.clone());
            }
            let mapped = entry_alias(c, scope, entry, true)?;
            aliases.insert(entry.to_owned(), mapped.clone());
            Ok(mapped)
        })?;
        let report: AssessmentReport = serde_json::from_value(v)?;
        let report_json = canonical(&report)?;
        let checksum = digest(report_json.as_bytes());
        inputs.reports.insert(
            slot,
            ReportInput {
                digest: source_digest,
                projected_digest: checksum.clone(),
                aliases,
            },
        );
        settings.push((report_slot_key(id, &checksum), slot.to_string()));
        reports.insert(
            checksum.clone(),
            ReportFacts {
                digest: checksum,
                report_json,
            },
        );
    }
    let mut q=c.prepare("SELECT session_item_id,operation_id,entry_id,link_json FROM assessment_collection_links WHERE session_item_id IN (SELECT id FROM assessment_session_items WHERE session_id=?) ORDER BY session_item_id,operation_id")?;
    let mut links = BTreeMap::new();
    for row in q.query_map([id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (item, operation, entry, details) = row?;
        let mut details: Value = serde_json::from_str(&details)?;
        rewrite_entry_references(c, scope, &mut details, true)?;
        links.insert(
            (item.clone(), operation.clone()),
            CollectionLink {
                session_item_id: item,
                operation_id: operation,
                entry_id: entry_alias(c, scope, &entry, true)?.unwrap_or(entry),
                details,
            },
        );
    }
    if let Some(base) = &base {
        for report in &base.bundle.reports {
            reports
                .entry(report.digest.clone())
                .or_insert_with(|| report.clone());
        }
        for link in &base.bundle.collection_links {
            links
                .entry((link.session_item_id.clone(), link.operation_id.clone()))
                .or_insert_with(|| link.clone());
        }
    }
    let packet = AssessmentPacket {
        protocol: "assessment-1".into(),
        schema_version: 1,
        origin_device_id: origin,
        session: SessionFacts {
            id: s.id,
            state: s.state,
            revision: s.revision,
            config: s.config,
            sampling: s.sampling,
            position: s.position,
            created_at: s.created_at,
            updated_at: s.updated_at,
        },
        items,
        answers,
        corrections,
        reports: reports.into_values().collect(),
        collection_links: links.into_values().collect(),
        media: media.into_iter().collect(),
    };
    packet.validate_projection(previous, &reused_items)?;
    Ok((packet, inputs))
}

// Cached digests describe complete exported facts, never a truncated history or an acknowledgement.
fn cache_received_packet(
    c: &Connection,
    scope: &str,
    remote: &AssessmentDocument,
    previous: Option<&PreparedPacket>,
) -> Result<PreparedPacket, AppError> {
    let id = &remote.session_id;
    c.execute(
        "INSERT OR IGNORE INTO assessment_sync_accounts(account_key,updated_at) VALUES (?,?)",
        params![scope, chrono::Utc::now().timestamp_millis()],
    )?;
    let mut settings = Vec::new();
    let (packet, inputs) =
        export_with_base(c, scope, id, Some(remote), None, previous, &mut settings)?;
    write_settings(c, &settings)?;
    let hash = packet_digest(&packet)?;
    let mut references = 0usize;
    for link in &packet.collection_links {
        if entry_alias(c, scope, &link.entry_id, false)?.is_none() {
            references += 1;
        }
    }
    let epoch: i64 = c.query_row(
        "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
        [id],
        |r| r.get(0),
    )?;
    c.execute("INSERT INTO assessment_sync_state(account_key,session_id,cached_epoch,cached_digest,cached_pending_references) VALUES (?,?,?,?,?) ON CONFLICT(account_key,session_id) DO UPDATE SET cached_epoch=excluded.cached_epoch,cached_digest=excluded.cached_digest,cached_pending_references=excluded.cached_pending_references",params![scope,id,epoch,hash,references])?;
    Ok(PreparedPacket {
        scope: scope.into(),
        id: id.clone(),
        epoch,
        packet,
        hash,
        inputs,
    })
}

fn equal<T: Serialize>(a: &T, b: &T) -> Result<bool, AppError> {
    Ok(canonical(a)? == canonical(b)?)
}
fn choose<T: Clone + Serialize>(base: &T, local: &T, remote: &T) -> Result<T, AppError> {
    if equal(local, remote)? || equal(base, remote)? {
        Ok(local.clone())
    } else if equal(base, local)? {
        Ok(remote.clone())
    } else {
        Err(AppError::new(
            "assessment_conflict",
            "同一测试状态或输入被两端修改，完整资料已保留。",
        ))
    }
}
fn union<T: Clone + Serialize>(
    local: &[T],
    remote: &[T],
    key: impl Fn(&T) -> String,
) -> Result<Vec<T>, AppError> {
    let mut out = BTreeMap::new();
    for row in local.iter().chain(remote) {
        let id = key(row);
        if let Some(old) = out.get(&id) {
            if !equal(old, row)? {
                return Err(AppError::new(
                    "assessment_conflict",
                    "同一测试事件的内容不同，保留两端完整版本。",
                ));
            }
        } else {
            out.insert(id, row.clone());
        }
    }
    Ok(out.into_values().collect())
}
pub(crate) fn merge(
    base: &AssessmentPacket,
    local: &AssessmentPacket,
    remote: &AssessmentPacket,
) -> Result<AssessmentPacket, AppError> {
    if !equal(&base.session.config, &local.session.config)?
        || !equal(&base.session.config, &remote.session.config)?
        || !equal(&base.session.sampling, &local.session.sampling)?
        || !equal(&base.session.sampling, &remote.session.sampling)?
        || base.origin_device_id != local.origin_device_id
        || base.origin_device_id != remote.origin_device_id
    {
        return Err(AppError::new(
            "assessment_conflict",
            "同一会话的冻结配置或抽样依据不一致。",
        ));
    }
    let mut merged = local.clone();
    for item in &mut merged.items {
        let b = base
            .items
            .iter()
            .find(|i| i.id == item.id)
            .ok_or_else(|| invalid("测试共同基准缺少题目。"))?;
        let r = remote
            .items
            .iter()
            .find(|i| i.id == item.id)
            .ok_or_else(|| invalid("远端测试缺少冻结题目。"))?;
        if item.position != r.position || !equal(&item.snapshot, &r.snapshot)? {
            return Err(AppError::new(
                "assessment_conflict",
                "已冻结的测试题目被修改。",
            ));
        }
        let b: Interaction = serde_json::from_str(&b.interaction_json)?;
        let r: Interaction = serde_json::from_str(&r.interaction_json)?;
        let mut interaction: Interaction = serde_json::from_str(&item.interaction_json)?;
        interaction.draft_text = choose(&b.draft_text, &interaction.draft_text, &r.draft_text)?;
        interaction.draft_option =
            choose(&b.draft_option, &interaction.draft_option, &r.draft_option)?;
        interaction.hinted |= r.hinted;
        interaction.hint_count = interaction.hint_count.max(r.hint_count);
        interaction.audio_played |= r.audio_played;
        interaction.audio_started |= r.audio_started;
        interaction.audio_seconds = interaction.audio_seconds.max(r.audio_seconds);
        item.interaction_json = canonical(&interaction)?;
    }
    // Revisions are derived from immutable correction events, not concurrent counters.
    let originals = union(
        &local
            .answers
            .iter()
            .map(|a| a.answer.clone())
            .collect::<Vec<_>>(),
        &remote
            .answers
            .iter()
            .map(|a| a.answer.clone())
            .collect::<Vec<_>>(),
        |a| a.id.clone(),
    )?;
    let corrections = union(&local.corrections, &remote.corrections, |c| c.id.clone())?;
    merged.answers = originals
        .into_iter()
        .map(|answer| {
            let revision = corrections
                .iter()
                .filter(|r| r.answer_id == answer.id)
                .map(|r| r.revision)
                .max()
                .unwrap_or(1);
            AnswerFacts { answer, revision }
        })
        .collect();
    merged.corrections = corrections;
    merged.reports = union(&local.reports, &remote.reports, |r| r.digest.clone())?;
    merged.collection_links = union(&local.collection_links, &remote.collection_links, |r| {
        format!("{}:{}", r.session_item_id, r.operation_id)
    })?;
    merged.media = local
        .media
        .iter()
        .chain(&remote.media)
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    merged.session.position = merged.answers.len();
    merged.session.state = if merged.session.position == merged.items.len() {
        "completed".into()
    } else {
        choose(
            &base.session.state,
            &local.session.state,
            &remote.session.state,
        )?
    };
    merged.session.revision = local.session.revision.max(remote.session.revision) + 1;
    merged.session.updated_at = local.session.updated_at.max(remote.session.updated_at);
    merged.validate()?;
    Ok(merged)
}

fn account_on(c: &Connection, scope: &str, capability: Option<&str>) -> Result<(), AppError> {
    c.execute("INSERT INTO assessment_sync_accounts(account_key,capability,updated_at) VALUES (?,?,?) ON CONFLICT(account_key) DO UPDATE SET capability=excluded.capability,updated_at=excluded.updated_at",params![scope,capability,chrono::Utc::now().timestamp_millis()])?;
    Ok(())
}
fn local_original_path(media: &MediaReference) -> String {
    format!("media/assessment/sync-{}.{}", media.digest, media.format)
}
fn report_slot_key(session: &str, checksum: &str) -> String {
    format!("assessment_report_slot:{session}:{checksum}")
}
fn save_report(c: &Connection, id: &str, report: &ReportFacts) -> Result<(), AppError> {
    let key = report_slot_key(id, &report.digest);
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [&key],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(());
    }
    let parsed: AssessmentReport = serde_json::from_str(&report.report_json)?;
    let mut slot = parsed.revision;
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM assessment_report_versions WHERE session_id=? AND revision=?)",
        params![id, slot],
        |r| r.get::<_, bool>(0),
    )? {
        slot=c.query_row("SELECT MIN(COALESCE(MIN(revision),0),0)-1 FROM assessment_report_versions WHERE session_id=?",[id],|r|r.get(0))?;
    }
    c.execute(
        "INSERT INTO assessment_report_versions VALUES (?,?,?,?)",
        params![id, slot, report.report_json, parsed.generated_at],
    )?;
    c.execute(
        "INSERT INTO settings(key,value_json) VALUES (?,?)",
        params![key, slot.to_string()],
    )?;
    Ok(())
}
// Callers validate incoming, merged or preserved packets before applying them.
fn apply_validated_on(
    c: &Connection,
    root: &std::path::Path,
    scope: &str,
    packet: &AssessmentPacket,
) -> Result<(), AppError> {
    let s = &packet.session;
    let prior: Option<i64> = c
        .query_row(
            "SELECT revision FROM assessment_sessions WHERE id=?",
            [&s.id],
            |r| r.get(0),
        )
        .optional()?;
    // Local report display slots are never overwritten by another device's fact counter.
    let revision = prior.map_or(s.revision, |r| r.max(s.revision) + 1);
    c.execute("INSERT INTO assessment_sessions VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET state=excluded.state,revision=excluded.revision,config_json=excluded.config_json,sampling_json=excluded.sampling_json,position=excluded.position,updated_at=excluded.updated_at",params![s.id,s.state,revision,canonical(&s.config)?,canonical(&s.sampling)?,s.position,s.created_at,s.updated_at])?;
    for row in &packet.items {
        let mut snapshot = row.snapshot.clone();
        if let Some(audio) = snapshot.content.audio.as_mut() {
            if audio.kind == "original" {
                let media = packet
                    .media
                    .iter()
                    .find(|m| m.digest == audio.sha256)
                    .ok_or_else(|| invalid("会话缺少原声清单。"))?;
                audio.relative_path = local_original_path(media);
                crate::corpus::validate_media_file(
                    root,
                    &audio.relative_path,
                    &audio.sha256,
                    "ready",
                )?;
            }
        }
        c.execute("INSERT INTO assessment_session_items VALUES (?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET snapshot_json=excluded.snapshot_json,interaction_json=excluded.interaction_json WHERE assessment_session_items.snapshot_json IS NOT excluded.snapshot_json OR assessment_session_items.interaction_json IS NOT excluded.interaction_json",params![row.id,s.id,row.position,canonical(&snapshot)?,row.interaction_json])?;
    }
    for row in &packet.answers {
        let a = &row.answer;
        c.execute("INSERT INTO assessment_answers VALUES (?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision WHERE assessment_answers.revision IS NOT excluded.revision",params![a.id,s.id,a.session_item_id,canonical(a)?,row.revision,a.created_at])?;
    }
    for row in &packet.corrections {
        c.execute(
            "INSERT OR IGNORE INTO assessment_corrections VALUES (?,?,?,?)",
            params![row.id, row.answer_id, canonical(row)?, row.created_at],
        )?;
    }
    for row in &packet.reports {
        save_report(c, &s.id, row)?;
    }
    for row in &packet.collection_links {
        let local = entry_alias(c, scope, &row.entry_id, false)?;
        let Some(local) = local else {
            continue;
        };
        if !c.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE id=?)",
            [&local],
            |r| r.get::<_, bool>(0),
        )? {
            continue;
        }
        let mut details = row.details.clone();
        rewrite_entry_references(c, scope, &mut details, false)?;
        c.execute(
            "INSERT OR IGNORE INTO assessment_collection_links VALUES (?,?,?,?)",
            params![
                row.session_item_id,
                row.operation_id,
                local,
                canonical(&details)?
            ],
        )?;
    }
    if matches!(s.state.as_str(), "completed" | "ended_early") {
        report_on(c, &s.id)?;
    }
    Ok(())
}
impl Store {
    fn previous_assessment(&self, scope: &str, id: &str) -> Option<Arc<PreparedPacket>> {
        self.assessment_packet.lock().ok().and_then(|cached| {
            cached
                .as_ref()
                .filter(|p| p.scope == scope && p.id == id)
                .cloned()
        })
    }

    fn prepared_assessment(&self, scope: &str, id: &str) -> Result<Arc<PreparedPacket>, AppError> {
        loop {
            let (packet, inputs, epoch, references, settings, report) = {
                let mut c = self.connection()?;
                let epoch: i64 = c.query_row(
                    "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                    [id],
                    |r| r.get(0),
                )?;
                let previous = self.previous_assessment(scope, id);
                if let Some(cached) = previous.as_ref().filter(|p| p.epoch == epoch) {
                    return Ok(Arc::clone(cached));
                }
                let _transaction = crate::local_profile::phase("assessment.read_tx");
                let changes = c.total_changes();
                let tx = c.transaction()?;
                let report = if matches!(
                    load_session(&tx, id)?.state.as_str(),
                    "completed" | "ended_early"
                ) {
                    Some(prepare_report(&tx, id)?)
                } else {
                    None
                };
                let base = load_base(&tx, scope, id)?;
                let mut settings = Vec::new();
                let (packet, inputs) = export_with_base(
                    &tx,
                    scope,
                    id,
                    base.as_ref(),
                    report.as_ref(),
                    previous.as_deref(),
                    &mut settings,
                )?;
                let epoch: i64 = tx.query_row(
                    "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                    [id],
                    |r| r.get(0),
                )?;
                let mut references = 0;
                for link in &packet.collection_links {
                    if entry_alias(&tx, scope, &link.entry_id, false)?.is_none() {
                        references += 1;
                    }
                }
                // Report preparation and export leave business rows untouched.
                let _commit = (tx.total_changes() != changes)
                    .then(|| crate::local_profile::phase("assessment.write_commit"));
                tx.commit()?;
                (packet, inputs, epoch, references, settings, report)
            };
            // Hashing the detached complete snapshot does not occupy the business connection.
            let hash = packet_digest(&packet)?;
            let published_epoch = {
                let mut c = self.connection()?;
                let _transaction = crate::local_profile::phase("assessment.publish_tx");
                let tx = c.transaction()?;
                let current: i64 = tx.query_row(
                    "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                    [id],
                    |r| r.get(0),
                )?;
                // The first export also depends on the as-yet-unpublished device id.
                // Another Store may have initialized it while hashing was detached.
                let device_changed = settings
                    .iter()
                    .any(|(key, _)| key == "assessment_device_v1")
                    && tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM settings WHERE key='assessment_device_v1')",
                        [],
                        |r| r.get::<_, bool>(0),
                    )?;
                if current != epoch || device_changed {
                    None
                } else {
                    tx.execute("INSERT OR IGNORE INTO assessment_sync_accounts(account_key,updated_at) VALUES (?,?)",params![scope,chrono::Utc::now().timestamp_millis()])?;
                    if let Some(report) = &report {
                        report.persist(&tx)?;
                    }
                    let published_epoch: i64 = tx.query_row(
                        "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                        [id],
                        |r| r.get(0),
                    )?;
                    write_settings(&tx, &settings)?;
                    tx.execute("INSERT INTO assessment_sync_state(account_key,session_id,cached_epoch,cached_digest,cached_pending_references) VALUES (?,?,?,?,?) ON CONFLICT(account_key,session_id) DO UPDATE SET cached_epoch=excluded.cached_epoch,cached_digest=excluded.cached_digest,cached_pending_references=excluded.cached_pending_references",params![scope,id,published_epoch,hash,references])?;
                    let _commit = crate::local_profile::phase("assessment.write_commit");
                    tx.commit()?;
                    Some(published_epoch)
                }
            };
            if let Some(epoch) = published_epoch {
                let prepared = Arc::new(PreparedPacket {
                    scope: scope.into(),
                    id: id.into(),
                    epoch,
                    packet,
                    hash,
                    inputs,
                });
                // Retain only the most recent complete packet, never one copy per historical session.
                if let Ok(mut cached) = self.assessment_packet.lock() {
                    *cached = Some(Arc::clone(&prepared));
                }
                return Ok(prepared);
            }
        }
    }
    pub fn assessment_sync_conflicts(&self, scope: &str) -> Result<Vec<Value>, AppError> {
        let c = self.connection()?;
        let mut q=c.prepare("SELECT session_id,conflict_json FROM assessment_sync_state WHERE account_key=? AND conflict_json IS NOT NULL ORDER BY session_id")?;
        q.query_map([scope],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.map(|r|{
            let(id,raw)=r?;let v:Value=serde_json::from_str(&raw)?;
            let side=|p:&Value|json!({"state":p["session"]["state"],"position":p["session"]["position"],"planned":p["session"]["config"]["questionCount"],"answers":p["answers"].as_array().map(Vec::len).unwrap_or(0),"corrections":p["corrections"].as_array().map(Vec::len).unwrap_or(0),"reports":p["reports"].as_array().map(Vec::len).unwrap_or(0)});
            let local=export_on(&c,self.root(),scope,&id)?;
            Ok(json!({"sessionId":id,"reason":v["reason"],"localRevision":local.session.revision,"remoteRevision":v["remote"]["revision"],"local":side(&serde_json::to_value(local)?),"remote":side(&v["remote"]["bundle"])}))
        }).collect()
    }
    pub fn assessment_sync_keep_both(
        &self,
        scope: &str,
        input: &KeepAssessmentBoth,
    ) -> Result<Value, AppError> {
        // Keep the existing original recording in its portable cache before the transaction.
        let packet = {
            let c = self.connection()?;
            export_on(&c, self.root(), scope, &input.session_id)?
        };
        for media in &packet.media {
            let bytes = self.assessment_sync_media(&media.digest, &media.format)?;
            self.assessment_save_remote_media(media, &bytes)?;
        }
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let result = crate::assessment_bank::operation(
            &tx,
            &input.operation_id,
            "assessment_keep_both",
            input,
            |tx| {
                let raw:Option<String>=tx.query_row("SELECT conflict_json FROM assessment_sync_state WHERE account_key=? AND session_id=?",params![scope,input.session_id],|r|r.get(0)).optional()?.flatten();
                let raw =
                    raw.ok_or_else(|| AppError::new("conflict", "这次冲突已处理，请刷新页面。"))?;
                let conflict: Value = serde_json::from_str(&raw)?;
                let remote: AssessmentDocument =
                    serde_json::from_value(conflict["remote"].clone())?;
                let local = export_on(tx, self.root(), scope, &input.session_id)?;
                if local.session.revision != input.expected_local_revision
                    || remote.revision != input.expected_remote_revision
                {
                    return Err(AppError::new(
                        "conflict",
                        "测试记录已变化，请刷新后重新选择。",
                    ));
                }
                remote.bundle.validate()?;
                let archive = Uuid::new_v4().to_string();
                let archived = json!({"local":local,"remote":remote,"reason":conflict["reason"]});
                tx.execute(
                    "INSERT INTO assessment_operations VALUES (?,?,?,?,?)",
                    params![
                        archive,
                        "assessment_conflict_archive",
                        digest(canonical(&archived)?.as_bytes()),
                        canonical(&archived)?,
                        chrono::Utc::now().timestamp_millis()
                    ],
                )?;
                let copy = preserved_copy(&local, &archive)?;
                apply_validated_on(tx, self.root(), scope, &copy)?;
                let copy_base = AssessmentDocument {
                    session_id: copy.session.id.clone(),
                    revision: 0,
                    bundle: copy.clone(),
                };
                tx.execute("INSERT INTO assessment_sync_state(account_key,session_id,base_json) VALUES (?,?,?)",params![scope,copy.session.id,canonical(&copy_base)?])?;
                // Exact old packets are archived above; replace only this session's local projection.
                tx.execute("DELETE FROM assessment_collection_links WHERE session_item_id IN (SELECT id FROM assessment_session_items WHERE session_id=?)",[&input.session_id])?;
                tx.execute("DELETE FROM assessment_corrections WHERE answer_id IN (SELECT id FROM assessment_answers WHERE session_id=?)",[&input.session_id])?;
                tx.execute(
                    "DELETE FROM assessment_answers WHERE session_id=?",
                    [&input.session_id],
                )?;
                tx.execute(
                    "DELETE FROM assessment_session_items WHERE session_id=?",
                    [&input.session_id],
                )?;
                tx.execute(
                    "DELETE FROM assessment_report_versions WHERE session_id=?",
                    [&input.session_id],
                )?;
                let prefix = format!("assessment_report_slot:{}:", input.session_id);
                tx.execute(
                    "DELETE FROM settings WHERE substr(key,1,?1)=?2",
                    params![prefix.len(), prefix],
                )?;
                apply_validated_on(tx, self.root(), scope, &remote.bundle)?;
                tx.execute("UPDATE assessment_sync_state SET base_json=?,pending_json=NULL,pending_change_id=NULL,pending_digest=NULL,conflict_json=NULL WHERE account_key=? AND session_id=?",params![canonical(&remote)?,scope,input.session_id])?;
                let acknowledged =
                    packet_digest(&export_on(tx, self.root(), scope, &input.session_id)?)?;
                tx.execute("UPDATE assessment_sync_state SET acknowledged_digest=? WHERE account_key=? AND session_id=?",params![acknowledged,scope,input.session_id])?;
                Ok(
                    json!({"sessionId":input.session_id,"copySessionId":copy.session.id,"archiveId":archive}),
                )
            },
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn assessment_sync_media(&self, hash: &str, format: &str) -> Result<Vec<u8>, AppError> {
        if !sha(hash) {
            return Err(invalid("测试原声摘要无效。"));
        }
        let c = self.connection()?;
        let path:Option<String>=c.query_row("SELECT json_extract(snapshot_json,'$.content.audio.relativePath') FROM assessment_session_items WHERE json_extract(snapshot_json,'$.content.audio.sha256')=? AND json_extract(snapshot_json,'$.content.audio.kind')='original' LIMIT 1",[hash],|r|r.get(0)).optional()?;
        let relative = path.unwrap_or_else(|| {
            local_original_path(&MediaReference {
                digest: hash.into(),
                format: format.into(),
            })
        });
        crate::corpus::validate_media_file(self.root(), &relative, hash, "ready")?;
        let bytes = std::fs::read(self.root().join(relative))?;
        if bytes.len() > 20 * 1024 * 1024 {
            return Err(AppError::new(
                "capacity_exceeded",
                "测试原声超过单文件同步上限，原资料保留。",
            ));
        }
        Ok(bytes)
    }
    pub fn assessment_save_remote_media(
        &self,
        media: &MediaReference,
        bytes: &[u8],
    ) -> Result<(), AppError> {
        if !sha(&media.digest) || bytes.len() > 20 * 1024 * 1024 || digest(bytes) != media.digest {
            return Err(invalid("测试原声内容与清单不一致。"));
        }
        let relative = local_original_path(media);
        let target = self.root().join(&relative);
        std::fs::create_dir_all(target.parent().unwrap())?;
        if target.is_file() && digest(&std::fs::read(&target)?) == media.digest {
            return Ok(());
        }
        let temporary = self
            .root()
            .join("jobs")
            .join(format!("assessment-media-{}", Uuid::new_v4()));
        std::fs::write(&temporary, bytes)?;
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(temporary, target)?;
        crate::corpus::validate_media_file(self.root(), &relative, &media.digest, "ready")?;
        Ok(())
    }
    pub fn assessment_sync_capability(
        &self,
        scope: &str,
        capability: Option<&str>,
    ) -> Result<(), AppError> {
        let c = self.connection()?;
        account_on(&c, scope, capability)
    }
    pub fn assessment_sync_status(&self, scope: &str) -> Result<AssessmentSyncStatus, AppError> {
        let _operation = crate::local_profile::operation("assessment.status");
        let ids = {
            let c = self.connection()?;
            let mut q = c.prepare("SELECT d.session_id FROM assessment_sync_dirty d LEFT JOIN assessment_sync_state s ON s.account_key=? AND s.session_id=d.session_id WHERE s.cached_digest IS NULL OR s.cached_epoch<>d.epoch ORDER BY d.session_id")?;
            q.query_map([scope], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        // Refresh only changed sessions; release the business connection between complete packets.
        for id in ids {
            self.prepared_assessment(scope, &id)?;
        }
        let c = self.connection()?;
        let row: Option<(Option<String>, Option<String>)> = c
            .query_row(
                "SELECT capability,cursor FROM assessment_sync_accounts WHERE account_key=?",
                [scope],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(AssessmentSyncStatus {
            capability: row.as_ref().and_then(|r| r.0.clone()),
            cursor: row.and_then(|r| r.1).and_then(|v| v.parse().ok()).unwrap_or(0),
            pending: c.query_row("SELECT COUNT(*) FROM assessment_sync_state WHERE account_key=? AND cached_digest IS NOT acknowledged_digest", [scope], |r| r.get(0))?,
            conflicts: c.query_row("SELECT COUNT(*) FROM assessment_sync_state WHERE account_key=? AND conflict_json IS NOT NULL", [scope], |r| r.get(0))?,
            pending_references: c.query_row("SELECT COALESCE(SUM(cached_pending_references),0) FROM assessment_sync_state WHERE account_key=?", [scope], |r| r.get(0))?,
        })
    }
    pub fn assessment_push_candidates(&self, scope: &str) -> Result<Vec<String>, AppError> {
        let c = self.connection()?;
        let mut q = c.prepare("SELECT d.session_id FROM assessment_sync_dirty d JOIN assessment_sessions a ON a.id=d.session_id LEFT JOIN assessment_sync_state s ON s.account_key=? AND s.session_id=d.session_id WHERE s.conflict_json IS NULL AND (s.pending_json IS NOT NULL OR s.cached_digest IS NULL OR s.cached_epoch<>d.epoch OR s.cached_digest IS NOT s.acknowledged_digest) ORDER BY a.created_at,a.id")?;
        Ok(q.query_map([scope], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn assessment_freeze_push(
        &self,
        scope: &str,
        id: &str,
    ) -> Result<Option<AssessmentPush>, AppError> {
        let _operation = crate::local_profile::operation("assessment.freeze");
        loop {
            let saved: Option<(i64, Option<String>, Option<String>, bool)> = {
                let c = self.connection()?;
                if c.query_row("SELECT EXISTS(SELECT 1 FROM assessment_sync_state s JOIN assessment_sync_dirty d ON d.session_id=s.session_id WHERE s.account_key=? AND s.session_id=? AND s.pending_json IS NULL AND s.conflict_json IS NULL AND s.cached_epoch=d.epoch AND s.cached_digest=s.acknowledged_digest)",params![scope,id],|r|r.get::<_,bool>(0))? {return Ok(None);}
                c.query_row(
                    "SELECT remote_revision,pending_json,acknowledged_digest,conflict_json IS NOT NULL FROM assessment_sync_state WHERE account_key=? AND session_id=?",
                    params![scope,id],
                    |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
                ).optional()?
            };
            if saved.as_ref().is_some_and(|r| r.3) {
                return Ok(None);
            }
            if let Some(pending) = saved.as_ref().and_then(|r| r.1.as_ref()) {
                return Ok(Some(serde_json::from_str(pending)?));
            }
            let prepared = self.prepared_assessment(scope, id)?;
            let acknowledged = saved.as_ref().and_then(|r| r.2.as_ref()) == Some(&prepared.hash);
            let _encoding = crate::local_profile::phase("assessment.freeze_encode");
            let push = (!acknowledged).then(|| AssessmentPush {
                change_id: Uuid::new_v4().to_string(),
                base_revision: saved.as_ref().map_or(0, |r| r.0),
                bundle: prepared.packet.clone(),
            });
            let payload = push.as_ref().map(canonical).transpose()?;
            drop(_encoding);
            if payload.as_ref().is_some_and(|v| v.len() > 8 * 1024 * 1024) {
                return Err(AppError::new(
                    "capacity_exceeded",
                    "完整水平测试资料超过单包能力，资料保持完整，未截断历史。",
                ));
            }
            let mut c = self.connection()?;
            let _transaction = crate::local_profile::phase("assessment.freeze_tx");
            let tx = c.transaction()?;
            let epoch: i64 = tx.query_row(
                "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                [id],
                |r| r.get(0),
            )?;
            let current:Option<(i64,Option<String>,Option<String>,bool)>=tx.query_row("SELECT remote_revision,pending_json,acknowledged_digest,conflict_json IS NOT NULL FROM assessment_sync_state WHERE account_key=? AND session_id=?",params![scope,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
            // A first snapshot publishes its empty sync-state row before reaching this commit.
            let expected = saved.unwrap_or((0, None, None, false));
            if epoch != prepared.epoch || current.as_ref() != Some(&expected) {
                continue;
            }
            if acknowledged {
                tx.commit()?;
                return Ok(None);
            }
            let push = push.unwrap();
            tx.execute("UPDATE assessment_sync_state SET pending_json=?,pending_change_id=?,pending_digest=? WHERE account_key=? AND session_id=?",params![payload,push.change_id,prepared.hash,scope,id])?;
            let _commit = crate::local_profile::phase("assessment.write_commit");
            tx.commit()?;
            return Ok(Some(push));
        }
    }
    pub fn assessment_freeze_pushes(&self, scope: &str) -> Result<Vec<AssessmentPush>, AppError> {
        let mut pushes = Vec::new();
        for id in self.assessment_push_candidates(scope)? {
            if let Some(push) = self.assessment_freeze_push(scope, &id)? {
                pushes.push(push);
            }
        }
        Ok(pushes)
    }
    pub fn assessment_revision_known(
        &self,
        scope: &str,
        id: &str,
        revision: i64,
    ) -> Result<bool, AppError> {
        Ok(self.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM assessment_sync_state WHERE account_key=? AND session_id=? AND remote_revision>=?)",params![scope,id,revision],|r|r.get(0))?)
    }
    pub fn assessment_sync_saved(
        &self,
        scope: &str,
        push: &AssessmentPush,
        revision: i64,
    ) -> Result<(), AppError> {
        if revision != push.base_revision + 1 {
            return Err(invalid("测试同步回执版本无效。"));
        }
        let _operation = crate::local_profile::operation("assessment.saved");
        let doc = AssessmentDocument {
            session_id: push.bundle.session.id.clone(),
            revision,
            bundle: push.bundle.clone(),
        };
        let _encoding = crate::local_profile::phase("assessment.saved_encode");
        let payload = canonical(&doc)?;
        drop(_encoding);
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        // Only advance an exact frozen projection. New local facts must remain dirty.
        let reusable: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM assessment_sync_state s JOIN assessment_sync_dirty d ON d.session_id=s.session_id WHERE s.account_key=? AND s.session_id=? AND s.pending_change_id=? AND s.cached_epoch=d.epoch AND s.cached_digest=s.pending_digest)",params![scope,push.bundle.session.id,push.change_id],|r|r.get(0))?;
        tx.execute("UPDATE assessment_sync_state SET base_json=?,acknowledged_digest=pending_digest,pending_json=NULL,pending_change_id=NULL,pending_digest=NULL WHERE account_key=? AND session_id=? AND pending_change_id=?",params![payload,scope,push.bundle.session.id,push.change_id])?;
        if reusable {
            tx.execute("UPDATE assessment_sync_state SET cached_epoch=(SELECT epoch FROM assessment_sync_dirty WHERE session_id=?2) WHERE account_key=?1 AND session_id=?2",params![scope,push.bundle.session.id])?;
        }
        let _commit = crate::local_profile::phase("assessment.write_commit");
        tx.commit()?;
        Ok(())
    }
    pub fn assessment_sync_receive(
        &self,
        scope: &str,
        remote: &AssessmentDocument,
    ) -> Result<bool, AppError> {
        let _operation = crate::local_profile::operation("assessment.receive");
        remote.bundle.validate()?;
        if remote.session_id != remote.bundle.session.id || remote.revision < 1 {
            return Err(invalid("测试变更关联无效。"));
        }
        let _encoding = crate::local_profile::phase("assessment.receive_encode");
        let remote_json = canonical(remote)?;
        drop(_encoding);
        let previous = self.previous_assessment(scope, &remote.session_id);
        let mut c = self.connection()?;
        let _transaction = crate::local_profile::phase("assessment.receive_tx");
        let tx = c.transaction()?;
        let saved:Option<(Option<String>,i64,bool)>=tx.query_row("SELECT s.acknowledged_digest,s.remote_revision,COALESCE(s.cached_epoch=d.epoch AND s.cached_digest=s.acknowledged_digest,0) FROM assessment_sync_state s JOIN assessment_sync_dirty d ON d.session_id=s.session_id WHERE s.account_key=? AND s.session_id=?",params![scope,remote.session_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if saved.as_ref().is_some_and(|s| s.1 >= remote.revision) {
            return Ok(true);
        }
        let unchanged = saved.as_ref().is_some_and(|s| s.2);
        let base = if unchanged {
            None
        } else {
            load_base(&tx, scope, &remote.session_id)?
        };
        let exists = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM assessment_sessions WHERE id=?)",
            [&remote.session_id],
            |r| r.get::<_, bool>(0),
        )?;
        let local = if exists && !unchanged {
            let mut settings = Vec::new();
            let (local, _) = export_with_base(
                &tx,
                scope,
                &remote.session_id,
                base.as_ref(),
                None,
                previous.as_deref(),
                &mut settings,
            )?;
            write_settings(&tx, &settings)?;
            Some(local)
        } else {
            None
        };
        let local_hash = local.as_ref().map(packet_digest).transpose()?;
        let apply = if let Some(local) = &local {
            if saved.as_ref().and_then(|r| r.0.as_ref()) == local_hash.as_ref() {
                Ok(remote.bundle.clone())
            } else if let Some(base) = &base {
                merge(&base.bundle, local, &remote.bundle)
            } else if equal(local, &remote.bundle)? {
                Ok(remote.bundle.clone())
            } else {
                Err(AppError::new(
                    "assessment_conflict",
                    "同一会话已有本地修改，缺少共同基准。",
                ))
            }
        } else {
            Ok(remote.bundle.clone())
        };
        match apply {
            Ok(packet) => {
                apply_validated_on(&tx, self.root(), scope, &packet)?;
                tx.execute("INSERT INTO assessment_sync_state(account_key,session_id,base_json) VALUES (?,?,?) ON CONFLICT(account_key,session_id) DO UPDATE SET base_json=excluded.base_json,pending_json=NULL,pending_change_id=NULL,pending_digest=NULL,conflict_json=NULL",params![scope,remote.session_id,remote_json])?;
                let prepared = cache_received_packet(&tx, scope, remote, previous.as_deref())?;
                // A merged local append must still be pushed; a plain receive is acknowledged locally.
                let merged = local.is_some()
                    && saved.as_ref().and_then(|r| r.0.as_ref()) != local_hash.as_ref()
                    && base.is_some();
                tx.execute("UPDATE assessment_sync_state SET acknowledged_digest=? WHERE account_key=? AND session_id=?",params![if merged{None}else{Some(&prepared.hash)},scope,remote.session_id])?;
                let _commit = crate::local_profile::phase("assessment.write_commit");
                tx.commit()?;
                if let Ok(mut cached) = self.assessment_packet.lock() {
                    *cached = Some(Arc::new(prepared));
                }
                Ok(true)
            }
            Err(error) => {
                tx.execute("INSERT INTO assessment_sync_state(account_key,session_id,conflict_json) VALUES (?,?,?) ON CONFLICT(account_key,session_id) DO UPDATE SET conflict_json=excluded.conflict_json",params![scope,remote.session_id,canonical(&json!({"local":local,"remote":remote,"reason":error.message}))?])?;
                let _commit = crate::local_profile::phase("assessment.write_commit");
                tx.commit()?;
                Ok(false)
            }
        }
    }
    pub fn assessment_sync_cursor(&self, scope: &str, cursor: i64) -> Result<(), AppError> {
        self.connection()?.execute(
            "UPDATE assessment_sync_accounts SET cursor=?,updated_at=? WHERE account_key=?",
            params![
                cursor.to_string(),
                chrono::Utc::now().timestamp_millis(),
                scope
            ],
        )?;
        Ok(())
    }
}

#[cfg(all(windows, feature = "desktop"))]
impl crate::application::Application {
    pub(crate) fn prepare_assessment_audio_cached(
        &self,
        session: &str,
        item: &str,
        context: &crate::tasks::TaskContext,
    ) -> Result<std::path::PathBuf, AppError> {
        let content = {
            let c = self.store.connection()?;
            let s = load_session(&c, session)?;
            let (position, frozen, _) = crate::assessment::load_item(&c, item, session)?;
            if !(matches!(s.state.as_str(), "completed" | "ended_early")
                || position < s.position
                || (s.state == "active" && position == s.position))
            {
                return Err(invalid("请先继续当前测试，或选择已作答的题目。"));
            }
            frozen.content
        };
        let audio = content.audio.as_ref().ok_or_else(|| {
            AppError::new("assessment_audio_unavailable", "此题没有预先准备的音频。")
        })?;
        if let Ok(path) = crate::assessment::validated_audio(&self.store, audio) {
            return Ok(path);
        }
        if audio.kind != "system" {
            return Err(AppError::new(
                "resource_missing",
                "对应原声尚未同步到本机，请先完成同步。",
            ));
        }
        let source_hash = audio.sha256.clone();
        let mut batch = vec![("listening".to_owned(), content)];
        self.assessment_prepare_audio(&mut batch, context)?;
        let restored = batch[0].1.audio.as_ref().unwrap();
        self.store.connection()?.execute("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",params![format!("assessment_audio_cache:{source_hash}"),canonical(restored)?])?;
        crate::assessment::validated_audio(&self.store, restored)
    }
    pub(crate) fn prepare_assessment_audio_cached_from_draft(
        &self,
        draft: &str,
        item: &str,
        context: &crate::tasks::TaskContext,
    ) -> Result<Option<std::path::PathBuf>, AppError> {
        let session = {
            let c = self.store.connection()?;
            let raw:String=c.query_row("SELECT result_json FROM assessment_operations WHERE operation_id=? AND kind='assessment_collection_prepare'",[draft],|r|r.get(0))?;
            let draft: crate::assessment_reports::AssessmentCollectionDraft =
                serde_json::from_str(&raw)?;
            let candidate = draft
                .items
                .iter()
                .find(|i| i.session_item_id == item)
                .ok_or_else(|| invalid("请选择已预览的测试题目。"))?;
            if candidate.audio_kind.is_none() {
                return Ok(None);
            }
            draft.session_id
        };
        self.prepare_assessment_audio_cached(&session, item, context)
            .map(Some)
    }
    pub fn assessment_audio_prepare(
        self: &std::sync::Arc<Self>,
        session: String,
        item: String,
        operation: String,
    ) -> Result<crate::tasks::TaskSnapshot, AppError> {
        let app = std::sync::Arc::clone(self);
        let hash = digest(format!("{session}:{item}").as_bytes());
        self.tasks
            .start("speech", &operation, &hash, move |context| {
                context.subject("水平测试英语录音");
                context.check_cancelled()?;
                let path = app.prepare_assessment_audio_cached(&session, &item, &context)?;
                Ok(json!({"path":path}))
            })
    }
}
