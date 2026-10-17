//! Versioned projections and explicit collection of frozen assessment content.
mod cache;
use crate::{
    assessment::{
        AssessmentAnswer, AssessmentCorrection, FrozenItem, effective_answer, load_item,
        load_session,
    },
    assessment_bank::{AssessmentConfig, operation, safe_audio_path},
    error::AppError,
    store::Store,
    vocabulary::{self, CollectionInput, CollectionResult, Entry, ExampleInput, digest},
};
use cache::{Content, ReportCache};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub planned: usize,
    pub processed: usize,
    pub unscored: usize,
    pub valid: usize,
    pub unanswered: usize,
    pub correct: usize,
    pub independent_correct: usize,
    pub incorrect: usize,
    pub unknown: usize,
    pub needs_confirmation: usize,
    pub uncertain: usize,
    pub hinted: usize,
    pub correct_rate: Option<f64>,
    pub independent_rate: Option<f64>,
    pub reasons: BTreeMap<String, usize>,
}
impl Counts {
    fn add(&mut self, answer: Option<&AssessmentAnswer>) {
        self.planned += 1;
        if let Some(a) = answer {
            self.processed += 1;
            self.uncertain += usize::from(a.uncertain);
            self.hinted += usize::from(a.hinted);
            let reason = a
                .unscored_reason
                .as_deref()
                .or((a.result == "needs_confirmation").then_some("needs_confirmation"));
            if a.result == "needs_confirmation" {
                self.needs_confirmation += 1;
            }
            if let Some(reason) = reason {
                self.unscored += 1;
                *self.reasons.entry(reason.into()).or_default() += 1;
            } else {
                self.valid += 1;
                match a.result.as_str() {
                    "correct" => {
                        self.correct += 1;
                        self.independent_correct += usize::from(!a.hinted && !a.uncertain);
                    }
                    "incorrect" => self.incorrect += 1,
                    "unknown" => self.unknown += 1,
                    _ => return,
                }
            }
        } else {
            self.unanswered += 1;
        }
        self.correct_rate = (self.valid > 0).then(|| self.correct as f64 / self.valid as f64);
        self.independent_rate =
            (self.valid > 0).then(|| self.independent_correct as f64 / self.valid as f64);
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ReportGroup {
    pub key: String,
    pub label: String,
    pub counts: Counts,
    pub insufficient_sample: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ReportItem {
    pub id: String,
    pub position: usize,
    pub item: FrozenItem,
    pub answer: Option<AssessmentAnswer>,
    #[serde(default)]
    pub original_answer: Option<AssessmentAnswer>,
    pub answer_revision: Option<i64>,
    pub corrections: Vec<AssessmentCorrection>,
    pub collected_entry_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentReport {
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
    pub coverage: Value,
    pub repeated_targets: usize,
    pub warnings: Vec<String>,
    pub items: Vec<ReportItem>,
}
fn groups(map: BTreeMap<String, (String, Counts)>, minimum: usize) -> Vec<ReportGroup> {
    map.into_iter()
        .map(|(key, (label, counts))| ReportGroup {
            key,
            label,
            insufficient_sample: counts.valid < minimum,
            counts,
        })
        .collect()
}
fn add(
    map: &mut BTreeMap<String, (String, Counts)>,
    key: String,
    label: String,
    a: Option<&AssessmentAnswer>,
) {
    map.entry(key)
        .or_insert_with(|| (label, Counts::default()))
        .1
        .add(a);
}
fn dimension(t: &str) -> (&str, &str) {
    match t {
        "cloze" => ("recall", "语境主动回忆"),
        "listening" => ("listening", "听音理解"),
        _ => ("recognition", "词义辨认"),
    }
}
fn type_label(t: &str) -> &str {
    match t {
        "cloze" => "语境填空",
        "listening" => "听音识义",
        _ => "词义辨识",
    }
}
fn summary(report: &AssessmentReport) -> Result<Value, AppError> {
    let mut value = serde_json::to_value(report)?;
    value.as_object_mut().unwrap().remove("items");
    value["itemTotal"] = json!(report.items.len());
    value["bankVersions"] = json!(
        report
            .items
            .iter()
            .map(|i| i.item.bank_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
    );
    Ok(value)
}

fn remember_report(c: &Connection, id: &str, slot: i64) -> Result<(), AppError> {
    // Use the existing counter for every changed fact, including linked entries.
    c.execute("INSERT INTO settings(key,value_json) SELECT 'assessment_current_report:'||?1,json_array(epoch,?2) FROM assessment_sync_dirty WHERE session_id=?1 ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",params![id,slot])?;
    Ok(())
}

pub(crate) struct PreparedReport {
    pub report: AssessmentReport,
    pub new_json: Option<String>,
    refresh_index: bool,
    business_cache: Option<String>,
}

impl PreparedReport {
    pub(crate) fn persist(&self, c: &Connection) -> Result<(), AppError> {
        if let Some(raw) = &self.new_json {
            c.execute(
                "INSERT INTO assessment_report_versions VALUES (?,?,?,?)",
                params![
                    self.report.session_id,
                    self.report.revision,
                    raw,
                    self.report.generated_at
                ],
            )?;
        }
        if self.refresh_index {
            // A newly inserted report has already advanced the fact counter.
            remember_report(c, &self.report.session_id, self.report.revision)?;
        }
        if let Some(raw) = &self.business_cache {
            ReportCache::persist(c, &self.report.session_id, raw)?;
        }
        Ok(())
    }
}

pub(crate) fn report_on(c: &Connection, id: &str) -> Result<AssessmentReport, AppError> {
    let prepared = prepare_report(c, id)?;
    prepared.persist(c)?;
    Ok(prepared.report)
}

// Read-only preparation lets synchronization publish the report and digest atomically.
pub(crate) fn prepare_report(c: &Connection, id: &str) -> Result<PreparedReport, AppError> {
    let _timing = crate::local_profile::phase("assessment.report");
    let s = load_session(c, id)?;
    if !matches!(s.state.as_str(), "completed" | "ended_early") {
        return Err(AppError::new(
            "assessment_not_finished",
            "测试尚未结束，请先继续作答或明确提前结束。",
        ));
    }
    let saved: Option<(i64, String)> = c
        .query_row(
            "SELECT r.revision,r.report_json FROM settings m JOIN assessment_sync_dirty d ON d.session_id=?1 AND d.epoch=json_extract(m.value_json,'$[0]') JOIN assessment_report_versions r ON r.session_id=?1 AND r.revision=json_extract(m.value_json,'$[1]') WHERE m.key='assessment_current_report:'||?1",
            [id],
            |r| Ok((r.get(0)?,r.get(1)?)),
        )
        .optional()?;
    if let Some((slot, saved)) = saved {
        let mut report: AssessmentReport = serde_json::from_str(&saved)?;
        report.revision = slot;
        return Ok(PreparedReport {
            report,
            new_json: None,
            refresh_index: false,
            business_cache: None,
        });
    }
    let mut cache = ReportCache::load(c, id)?;
    let inputs = ReportCache::inputs(c, id)?;
    if let Some((slot, raw)) = cache.current(c, id, &inputs)? {
        let mut report: AssessmentReport = serde_json::from_str(&raw)?;
        report.revision = slot;
        return Ok(PreparedReport {
            report,
            new_json: None,
            refresh_index: true,
            business_cache: None,
        });
    }
    let _build = crate::local_profile::phase("assessment.report_build");
    let mut q = c.prepare(
        "SELECT id,position FROM assessment_session_items WHERE session_id=? ORDER BY position",
    )?;
    let rows = q
        .query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, usize>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut counts = Counts::default();
    let (mut contents, mut bands, mut types, mut dims, mut details) = (
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut items = Vec::new();
    for (item_id, position) in rows {
        let (_, item, _) = load_item(c, &item_id, id)?;
        let answer_id: Option<String> = c
            .query_row(
                "SELECT id FROM assessment_answers WHERE session_item_id=?",
                [&item_id],
                |r| r.get(0),
            )
            .optional()?;
        let (answer, answer_revision, corrections) = if let Some(answer_id) = answer_id {
            let (a, r, cs) = effective_answer(c, &answer_id)?;
            (Some(a), Some(r), cs)
        } else {
            (None, None, vec![])
        };
        let original_answer: Option<AssessmentAnswer> = answer
            .as_ref()
            .map(|a| {
                let raw: String = c.query_row(
                    "SELECT answer_json FROM assessment_answers WHERE id=?",
                    [&a.id],
                    |r| r.get(0),
                )?;
                Ok::<_, AppError>(serde_json::from_str(&raw)?)
            })
            .transpose()?;
        let a = answer.as_ref();
        counts.add(a);
        let kind = if item.kind == "phrase" {
            "短语与组合"
        } else {
            "单词"
        };
        let band_key = format!(
            "{}:{}:{:010}:{:010}",
            item.catalog.id, item.catalog.source_sha256, item.from_rank, item.to_rank
        );
        let band_label = format!("{kind} · {}–{}", item.from_rank, item.to_rank);
        add(&mut contents, item.kind.clone(), kind.into(), a);
        add(&mut bands, band_key.clone(), band_label.clone(), a);
        add(
            &mut types,
            item.question_type.clone(),
            type_label(&item.question_type).into(),
            a,
        );
        let (dim, label) = dimension(&item.question_type);
        add(&mut dims, dim.into(), label.into(), a);
        add(
            &mut details,
            format!("{band_key}:{}", item.question_type),
            format!("{band_label} · {}", type_label(&item.question_type)),
            a,
        );
        let mut links=c.prepare("SELECT DISTINCT entry_id FROM assessment_collection_links WHERE session_item_id=? ORDER BY entry_id")?;
        let collected_entry_ids = links
            .query_map([&item_id], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        items.push(ReportItem {
            id: item_id,
            position,
            item,
            answer,
            original_answer,
            answer_revision,
            corrections,
            collected_entry_ids,
        });
    }
    let mut warnings = s.sampling.warnings.clone();
    if counts.valid < s.config.minimum_sample {
        warnings.push(format!(
            "有效题数 {} 少于 {}，样本不足，仅展示本次实际表现。",
            counts.valid, s.config.minimum_sample
        ));
    }
    if s.state == "ended_early" {
        warnings.push(format!(
            "本次提前结束，{} 题未作答，不纳入正确率分母。",
            counts.unanswered
        ));
    }
    if counts.needs_confirmation > 0 {
        warnings.push(format!(
            "{} 题待人工核对判分，当前报告为暂定。",
            counts.needs_confirmation
        ));
    }
    if s.sampling.repeated_targets > 0 {
        warnings.push(format!(
            "包含 {} 个近期已测目标，请结合重复情况解读。",
            s.sampling.repeated_targets
        ));
    }
    warnings.push("结果仅描述所选范围内已准备题目的抽样表现，不换算精确词汇量或英语等级。".into());
    let mut catalogs = s
        .sampling
        .catalogs
        .iter()
        .map(|c| json!([c.id, c.source_sha256, c.rank_unit]))
        .collect::<Vec<_>>();
    catalogs.sort_by_key(Value::to_string);
    let mut question_types = s.config.question_types.clone();
    question_types.sort();
    let comparison_key = digest(&serde_json::to_vec(
        &json!({"catalogs":catalogs,"content":s.config.content,
        "wordTopN":if s.config.content!="phrase"{Some(s.config.word_top_n)}else{None},
        "phraseTopN":if s.config.content!="word"{Some(s.config.phrase_top_n)}else{None},
        "questionCount":s.config.question_count,"questionTypes":question_types,"bandSize":s.config.band_size,
        "samplingPolicy":s.sampling.policy_version,"scorePolicy":s.sampling.score_policy}),
    )?);
    let minimum = s.config.minimum_sample;
    let mut report = AssessmentReport {
        session_id: id.into(),
        revision: s.revision,
        state: s.state,
        status: if counts.needs_confirmation > 0 {
            "provisional"
        } else {
            "settled"
        }
        .into(),
        created_at: s.created_at,
        generated_at: Utc::now().timestamp_millis(),
        score_policy: s.sampling.score_policy,
        config: s.config,
        comparison_key,
        counts,
        by_content: groups(contents, minimum),
        by_band: groups(bands, minimum),
        by_type: groups(types, minimum),
        by_dimension: groups(dims, minimum),
        details: groups(details, minimum),
        coverage: serde_json::to_value(s.sampling.coverage)?,
        repeated_targets: s.sampling.repeated_targets,
        warnings,
        items,
    };
    let content = Content::build(c, &report)?;
    let mut q = c.prepare("SELECT revision,report_json FROM assessment_report_versions WHERE session_id=? ORDER BY revision DESC")?;
    for saved in q.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (slot, raw) = saved?;
        if let Some(mut prior) = cache.matching(c, slot, &raw, &content)? {
            prior.revision = slot;
            let business_cache = cache.publish(c, id, inputs, slot, None)?;
            return Ok(PreparedReport {
                report: prior,
                new_json: None,
                refresh_index: true,
                business_cache: Some(business_cache),
            });
        }
    }
    drop(q);
    report.revision = c.query_row("SELECT MAX(?1,COALESCE(MAX(revision)+1,?1)) FROM assessment_report_versions WHERE session_id=?2",params![s.revision,id],|r|r.get(0))?;
    let raw = serde_json::to_string(&report)?;
    let business_cache = cache.publish(c, id, inputs, report.revision, Some((&raw, content)))?;
    Ok(PreparedReport {
        new_json: Some(raw),
        report,
        refresh_index: true,
        business_cache: Some(business_cache),
    })
}
impl Store {
    pub fn assessment_report(&self, id: &str, revision: Option<i64>) -> Result<Value, AppError> {
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let s = load_session(&tx, id)?;
        if !matches!(s.state.as_str(), "completed" | "ended_early") {
            return Ok(
                json!({"sessionId":id,"state":s.state,"position":s.position,"planned":s.config.question_count}),
            );
        }
        let report = if let Some(revision) = revision {
            let text:String=tx.query_row("SELECT report_json FROM assessment_report_versions WHERE session_id=? AND revision=?",params![id,revision],|r|r.get(0))?;
            serde_json::from_str::<AssessmentReport>(&text)?
        } else {
            report_on(&tx, id)?
        };
        let mut q=tx.prepare("SELECT revision FROM assessment_report_versions WHERE session_id=? ORDER BY revision DESC")?;
        let revisions = q
            .query_map([id], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(q);
        tx.commit()?;
        let mut view = summary(&report)?;
        // Received reports retain their immutable source JSON; this number is a local lookup slot.
        if let Some(slot) = revision {
            view["revision"] = json!(slot);
        }
        Ok(json!({"report":view,"revisions":revisions}))
    }
    pub fn assessment_report_items(
        &self,
        id: &str,
        revision: i64,
        offset: usize,
        limit: usize,
    ) -> Result<Value, AppError> {
        let c = self.connection()?;
        let text: String = c.query_row(
            "SELECT report_json FROM assessment_report_versions WHERE session_id=? AND revision=?",
            params![id, revision],
            |r| r.get(0),
        )?;
        let report: AssessmentReport = serde_json::from_str(&text)?;
        Ok(
            json!({"total":report.items.len(),"items":report.items.into_iter().skip(offset).take(limit.clamp(1,100)).collect::<Vec<_>>()}),
        )
    }
    pub fn assessment_history(
        &self,
        offset: usize,
        limit: usize,
        comparison_key: Option<&str>,
    ) -> Result<Value, AppError> {
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let mut q=tx.prepare("SELECT id FROM assessment_sessions s WHERE state IN ('completed','ended_early') AND NOT EXISTS(SELECT 1 FROM settings m JOIN assessment_sync_dirty d ON d.session_id=s.id AND d.epoch=json_extract(m.value_json,'$[0]') JOIN assessment_report_versions r ON r.session_id=s.id AND r.revision=json_extract(m.value_json,'$[1]') WHERE m.key='assessment_current_report:'||s.id)")?;
        let ids = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(q);
        for id in ids {
            report_on(&tx, &id)?;
        }
        let total:usize=tx.query_row("SELECT COUNT(*) FROM assessment_sessions s JOIN settings m ON m.key='assessment_current_report:'||s.id JOIN assessment_report_versions r ON r.session_id=s.id AND r.revision=json_extract(m.value_json,'$[1]') WHERE s.state IN ('completed','ended_early') AND (?1 IS NULL OR json_extract(r.report_json,'$.comparisonKey')=?1)",[comparison_key],|r|r.get(0))?;
        let mut q=tx.prepare("SELECT json_object('id',s.id,'state',s.state,'revision',r.revision,'createdAt',s.created_at,'config',json(s.config_json),'counts',json_extract(r.report_json,'$.counts'),'status',json_extract(r.report_json,'$.status'),'comparisonKey',json_extract(r.report_json,'$.comparisonKey'),'repeatedTargets',json_extract(r.report_json,'$.repeatedTargets'),'preservedCopy',json(CASE WHEN EXISTS(SELECT 1 FROM json_each(s.sampling_json,'$.warnings') WHERE substr(value,1,5)='保留副本：') THEN 'true' ELSE 'false' END)) FROM assessment_sessions s JOIN settings m ON m.key='assessment_current_report:'||s.id JOIN assessment_report_versions r ON r.session_id=s.id AND r.revision=json_extract(m.value_json,'$[1]') WHERE s.state IN ('completed','ended_early') AND (?1 IS NULL OR json_extract(r.report_json,'$.comparisonKey')=?1) ORDER BY s.created_at DESC,s.id DESC LIMIT ?2 OFFSET ?3")?;
        let items = q
            .query_map(params![comparison_key, limit.clamp(1, 100), offset], |r| {
                r.get::<_, String>(0)
            })?
            .map(|r| Ok(serde_json::from_str::<Value>(&r?)?))
            .collect::<Result<Vec<_>, AppError>>()?;
        drop(q);
        tx.commit()?;
        Ok(json!({"items":items,"total":total}))
    }
    pub fn assessment_compare(&self, first: &str, second: &str) -> Result<Value, AppError> {
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let a = report_on(&tx, first)?;
        let b = report_on(&tx, second)?;
        let banks = |r: &AssessmentReport| {
            r.items
                .iter()
                .map(|i| i.item.bank_id.clone())
                .collect::<std::collections::BTreeSet<_>>()
        };
        let copied = |r: &AssessmentReport| r.warnings.iter().any(|w| w.starts_with("保留副本："));
        let comparable = first != second
            && !copied(&a)
            && !copied(&b)
            && a.comparison_key == b.comparison_key
            && banks(&a) == banks(&b);
        let result = json!({"first":summary(&a)?,"second":summary(&b)?,"comparable":comparable,"explanation":if comparable{"配置及评分依据一致；仍需结合各频段实际样本、覆盖和近期重复解读，不代表已校准能力提升。"}else{"所选记录不是两次独立测试，或目录／题库版本、范围、频段、题量／题型构成或评分依据不同，仅作并列展示，不作同尺度提升结论。"},
            "correctRateDelta":if comparable{a.counts.correct_rate.zip(b.counts.correct_rate).map(|(x,y)|y-x)}else{None},
            "independentRateDelta":if comparable{a.counts.independent_rate.zip(b.counts.independent_rate).map(|(x,y)|y-x)}else{None}});
        tx.commit()?;
        Ok(result)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PrepareAssessmentCollection {
    pub session_id: String,
    pub item_ids: Vec<String>,
    pub operation_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CollectionCandidate {
    pub session_item_id: String,
    pub operation_id: String,
    pub kind: String,
    pub text: String,
    pub meaning: String,
    pub sentence: String,
    pub translation: String,
    pub audio_kind: Option<String>,
    pub matches: Vec<Entry>,
    pub collected_entry_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentCollectionDraft {
    pub draft_id: String,
    pub session_id: String,
    pub report_revision: i64,
    pub items: Vec<CollectionCandidate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CollectionSelection {
    pub session_item_id: String,
    pub target_entry_id: Option<String>,
    pub expected_revision: Option<i64>,
    pub meaning: String,
    pub translation: String,
    pub save_audio: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CommitAssessmentCollection {
    pub draft_id: String,
    pub items: Vec<CollectionSelection>,
    pub operation_id: String,
}

impl Store {
    pub fn assessment_collection_prepare(
        &self,
        input: &PrepareAssessmentCollection,
    ) -> Result<AssessmentCollectionDraft, AppError> {
        if input.item_ids.is_empty() || input.item_ids.len() > 300 {
            return Err(AppError::new("invalid_input", "请选择要收录的测试内容。"));
        }
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let result = operation(
            &tx,
            &input.operation_id,
            "assessment_collection_prepare",
            input,
            |tx| {
                let report = report_on(tx, &input.session_id)?;
                let mut items = Vec::new();
                for id in &input.item_ids {
                    let row = report.items.iter().find(|r| &r.id == id).ok_or_else(|| {
                        AppError::new("invalid_input", "所选题目不在当前报告中。")
                    })?;
                    if items
                        .iter()
                        .any(|i: &CollectionCandidate| i.session_item_id == *id)
                    {
                        continue;
                    }
                    let mut q=tx.prepare("SELECT id FROM entries WHERE kind=? AND match_key=? AND archived=0 ORDER BY created_at,id")?;
                    let ids = q
                        .query_map(
                            params![
                                row.item.kind,
                                vocabulary::normalize(&row.item.content.target)
                            ],
                            |r| r.get::<_, String>(0),
                        )?
                        .collect::<Result<Vec<_>, _>>()?;
                    let matches = ids
                        .iter()
                        .map(|id| vocabulary::get_entry_on(tx, id))
                        .collect::<Result<Vec<_>, _>>()?;
                    items.push(CollectionCandidate {
                        session_item_id: id.clone(),
                        operation_id: Uuid::new_v4().to_string(),
                        kind: row.item.kind.clone(),
                        text: row.item.content.target.clone(),
                        meaning: row.item.content.meaning.clone(),
                        sentence: row.item.content.sentence.clone(),
                        translation: row.item.content.translation.clone(),
                        audio_kind: row.item.content.audio.as_ref().map(|a| a.kind.clone()),
                        matches,
                        collected_entry_ids: row.collected_entry_ids.clone(),
                    });
                }
                Ok(AssessmentCollectionDraft {
                    draft_id: input.operation_id.clone(),
                    session_id: input.session_id.clone(),
                    report_revision: report.revision,
                    items,
                })
            },
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn assessment_collect_item(
        &self,
        draft_id: &str,
        selection: &CollectionSelection,
    ) -> Result<CollectionResult, AppError> {
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let text:String=tx.query_row("SELECT result_json FROM assessment_operations WHERE operation_id=? AND kind='assessment_collection_prepare'",[draft_id],|r|r.get(0))?;
        let draft: AssessmentCollectionDraft = serde_json::from_str(&text)?;
        let candidate = draft
            .items
            .iter()
            .find(|r| r.session_item_id == selection.session_item_id)
            .ok_or_else(|| AppError::new("invalid_input", "请选择已预览的收录内容。"))?;
        let result = operation(
            &tx,
            &candidate.operation_id,
            "assessment_collect_item",
            selection,
            |tx| {
                let (_, item, _) = load_item(tx, &selection.session_item_id, &draft.session_id)?;
                if let Some(target) = &selection.target_entry_id {
                    if !candidate.matches.iter().any(|e| &e.id == target) {
                        return Err(AppError::new(
                            "invalid_input",
                            "合并目标不在本次匹配结果中，请重新预览。",
                        ));
                    }
                }
                let now = Utc::now().timestamp_millis();
                let source_fingerprint = format!(
                    "assessment:{}:{}",
                    draft.session_id, selection.session_item_id
                );
                let source_id: Option<String> = tx
                    .query_row(
                        "SELECT id FROM sources WHERE kind='assessment' AND fingerprint=?",
                        [&source_fingerprint],
                        |r| r.get(0),
                    )
                    .optional()?;
                let source_id = if let Some(id) = source_id {
                    id
                } else {
                    let id = Uuid::new_v4().to_string();
                    tx.execute("INSERT INTO sources(id,kind,title,fingerprint,created_at) VALUES (?,'assessment',?,?,?)",params![id,format!("高频水平测试 · {}",item.content.target),source_fingerprint,now])?;
                    id
                };
                let mut assets = Vec::new();
                if selection.save_audio {
                    if let Some(frozen) = &item.content.audio {
                        let audio = crate::assessment::resolved_audio_on(self.root(), tx, frozen)?;
                        let path = safe_audio_path(self.root(), &audio)?;
                        let bytes = std::fs::read(&path)?;
                        if digest(&bytes) != audio.sha256 {
                            return Err(AppError::new(
                                "resource_missing",
                                "测试录音已变化，请重新核对或选择只保存文字。",
                            ));
                        }
                        let asset = if let Some(id) = &audio.asset_id {
                            id.clone()
                        } else {
                            let recipe = digest(
                                format!("assessment:{}:{}", audio.kind, audio.sha256).as_bytes(),
                            );
                            let existing: Option<String> = tx
                                .query_row(
                                    "SELECT id FROM media_assets WHERE recipe_key=?",
                                    [&recipe],
                                    |r| r.get(0),
                                )
                                .optional()?;
                            if let Some(id) = existing {
                                id
                            } else {
                                let id = Uuid::new_v4().to_string();
                                let format =
                                    path.extension().and_then(|e| e.to_str()).unwrap_or("wav");
                                tx.execute("INSERT INTO media_assets(id,recipe_key,digest,relative_path,kind,format,duration_ms,state,created_at) VALUES (?,?,?,?,?,?,0,'ready',?)",params![id,recipe,audio.sha256,audio.relative_path,audio.kind,format,now])?;
                                id
                            }
                        };
                        assets.push(asset);
                    }
                }
                let input = CollectionInput {
                    operation_id: candidate.operation_id.clone(),
                    kind: item.kind.clone(),
                    text: item.content.target.clone(),
                    meaning: selection.meaning.clone(),
                    examples: vec![ExampleInput {
                        text: item.content.sentence.clone(),
                        context_meaning: selection.translation.clone(),
                        source_id: Some(source_id),
                        location_key: format!("assessment:{}", selection.session_item_id),
                        media_asset_ids: assets,
                        ..Default::default()
                    }],
                    target_entry_id: selection.target_entry_id.clone(),
                    expected_revision: selection.expected_revision,
                };
                let result = vocabulary::collect_on(self.root(), tx, &input)?;
                tx.execute("INSERT INTO assessment_collection_links VALUES (?,?,?,?)",params![selection.session_item_id,candidate.operation_id,result.entry_id,serde_json::to_string(&json!({"sessionId":draft.session_id,"reportRevision":draft.report_revision,"catalog":item.catalog,"rank":item.target_rank,"audioKind":item.content.audio.as_ref().map(|a|&a.kind),"savedAudio":selection.save_audio,"collection":result}))?])?;
                tx.execute(
                    "UPDATE assessment_sessions SET revision=revision+1,updated_at=? WHERE id=?",
                    params![now, draft.session_id],
                )?;
                Ok(result)
            },
        )?;
        tx.commit()?;
        Ok(result)
    }
}

impl crate::application::Application {
    pub fn assessment_collection_commit(
        self: &std::sync::Arc<Self>,
        input: CommitAssessmentCollection,
    ) -> Result<crate::tasks::TaskSnapshot, AppError> {
        if input.items.is_empty() || input.items.len() > 300 {
            return Err(AppError::new("invalid_input", "请选择并确认要保存的项目。"));
        }
        let store = std::sync::Arc::clone(&self.store);
        let app = std::sync::Arc::clone(self);
        let hash = digest(&serde_json::to_vec(&input)?);
        self.tasks.start("collection",&input.operation_id,&hash,move|context|{
            context.subject("水平测试确认收录");
            let mut receipts=Vec::new();let total=input.items.len();
            for (index,selection) in input.items.iter().enumerate() {
                context.check_cancelled()?;
                #[cfg(all(windows,feature="desktop"))]
                let audio=if selection.save_audio{app.prepare_assessment_audio_cached_from_draft(&input.draft_id,&selection.session_item_id,&context).map(|_|())}else{Ok(())};
                #[cfg(not(all(windows,feature="desktop")))]
                let audio:Result<(),AppError>={let _=&app;Ok(())};
                let result=audio.and_then(|()|store.assessment_collect_item(&input.draft_id,selection));
                receipts.push(match result{Ok(result)=>json!({"sessionItemId":selection.session_item_id,"saved":true,"result":result}),Err(error)=>json!({"sessionItemId":selection.session_item_id,"saved":false,"error":error})});
                context.partial_result(json!({"items":receipts}));
                context.progress("collection",index+1,total,"正在保存已确认的测试内容");
            }
            Ok(json!({"items":receipts}))
        })
    }
}
