//! Independent offline assessment sessions. No model, collection or regular-review calls.
use crate::{
    assessment_bank::{
        AssessmentAudio, AssessmentConfig, Choice, FrequencyCatalog, QuestionDraft, operation,
        revision_conflict, safe_audio_path,
    },
    assessment_sampling::{AssessmentCoverage, AssessmentPreview, AssessmentQuota, load_inventory},
    error::AppError,
    store::Store,
    vocabulary::{digest, normalize},
};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

pub const SCORE_POLICY: &str = "assessment-score-1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingSnapshot {
    pub seed: String,
    pub inventory_revision: String,
    pub policy_version: String,
    pub score_policy: String,
    pub quotas: Vec<AssessmentQuota>,
    pub coverage: Vec<AssessmentCoverage>,
    pub catalogs: Vec<FrequencyCatalog>,
    pub warnings: Vec<String>,
    pub repeated_targets: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FrozenItem {
    pub bank_id: String,
    pub item_id: String,
    pub catalog: FrequencyCatalog,
    pub target_rank: usize,
    pub lexeme_key: String,
    pub kind: String,
    pub question_type: String,
    pub from_rank: usize,
    pub to_rank: usize,
    pub content: QuestionDraft,
    pub repeat_recent: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Interaction {
    pub hinted: bool,
    pub hint_count: usize,
    pub draft_text: String,
    pub draft_option: Option<String>,
    pub playback_id: Option<String>,
    pub audio_started: bool,
    pub audio_played: bool,
    pub audio_seconds: f64,
    pub audio_error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentQuestion {
    pub id: String,
    pub position: usize,
    pub question_type: String,
    pub prompt: String,
    pub choices: Vec<Choice>,
    pub audio_kind: Option<String>,
    pub audio_played: bool,
    pub hinted: bool,
    pub hint: Option<String>,
    pub draft_text: String,
    pub draft_option: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentFeedback {
    pub answer_id: String,
    pub answer_revision: i64,
    pub session_item_id: String,
    pub question_type: String,
    pub target: String,
    pub sentence: String,
    pub meaning: String,
    pub translation: String,
    pub explanation: String,
    pub accepted_answers: Vec<String>,
    pub result: String,
    pub unscored_reason: Option<String>,
    pub uncertain: bool,
    pub hinted: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentSession {
    pub id: String,
    pub state: String,
    pub revision: i64,
    pub position: usize,
    pub planned: usize,
    pub processed: usize,
    pub config: AssessmentConfig,
    pub created_at: i64,
    pub updated_at: i64,
    pub question: Option<AssessmentQuestion>,
    pub last_feedback: Option<AssessmentFeedback>,
    pub warnings: Vec<String>,
    pub policy_version: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct StartAssessment {
    pub config: AssessmentConfig,
    pub expected_inventory_revision: String,
    pub operation_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentAnswerInput {
    pub session_id: String,
    pub item_id: String,
    pub expected_revision: i64,
    pub operation_id: String,
    pub action: String,
    pub option_id: Option<String>,
    pub text: String,
    pub uncertain: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentAnswer {
    pub id: String,
    pub session_id: String,
    pub session_item_id: String,
    pub original_action: String,
    pub option_id: Option<String>,
    pub text: String,
    pub result: String,
    pub unscored_reason: Option<String>,
    pub uncertain: bool,
    pub hinted: bool,
    pub audio_status: String,
    pub score_policy: String,
    pub created_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TransitionAssessment {
    pub session_id: String,
    pub expected_revision: i64,
    pub operation_id: String,
    pub action: String,
    pub item_id: Option<String>,
    #[serde(default)]
    pub draft_text: String,
    #[serde(default)]
    pub draft_option: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CorrectAssessment {
    pub answer_id: String,
    pub expected_revision: i64,
    pub operation_id: String,
    pub result: String,
    pub add_hint: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentCorrection {
    pub id: String,
    pub answer_id: String,
    pub revision: i64,
    pub result: String,
    pub add_hint: bool,
    pub reason: String,
    pub created_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentAudioStart {
    pub session_id: String,
    pub item_id: String,
    pub expected_revision: i64,
    pub operation_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentPlayback {
    pub playback_id: String,
    pub path: Option<String>,
    pub kind: String,
    pub error: Option<AppError>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentAudioEvent {
    pub session_id: String,
    pub item_id: String,
    pub playback_id: String,
    pub operation_id: String,
    pub event: String,
    pub seconds: f64,
    #[serde(default)]
    pub error: String,
}
pub(crate) struct SessionRecord {
    pub id: String,
    pub state: String,
    pub revision: i64,
    pub config: AssessmentConfig,
    pub sampling: SamplingSnapshot,
    pub position: usize,
    pub created_at: i64,
    pub updated_at: i64,
}
pub(crate) fn load_session(c: &Connection, id: &str) -> Result<SessionRecord, AppError> {
    let row=c.query_row("SELECT id,state,revision,config_json,sampling_json,position,created_at,updated_at FROM assessment_sessions WHERE id=?",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,usize>(5)?,r.get::<_,i64>(6)?,r.get::<_,i64>(7)?)))?;
    Ok(SessionRecord {
        id: row.0,
        state: row.1,
        revision: row.2,
        config: serde_json::from_str(&row.3)?,
        sampling: serde_json::from_str(&row.4)?,
        position: row.5,
        created_at: row.6,
        updated_at: row.7,
    })
}
pub(crate) fn load_item(
    c: &Connection,
    id: &str,
    session_id: &str,
) -> Result<(usize, FrozenItem, Interaction), AppError> {
    let (position,snapshot,interaction):(usize,String,String)=c.query_row("SELECT position,snapshot_json,interaction_json FROM assessment_session_items WHERE id=? AND session_id=?",params![id,session_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    Ok((
        position,
        serde_json::from_str(&snapshot)?,
        serde_json::from_str(&interaction)?,
    ))
}
fn active(record: &SessionRecord, revision: Option<i64>) -> Result<(), AppError> {
    if revision.is_some_and(|r| r != record.revision) {
        return Err(revision_conflict());
    }
    if record.state != "active" {
        return Err(AppError::new(
            "assessment_not_active",
            "请先继续当前测试，或查看已经结束的报告。",
        ));
    }
    Ok(())
}
fn current(
    c: &Connection,
    record: &SessionRecord,
    id: &str,
) -> Result<(FrozenItem, Interaction), AppError> {
    let (position, item, interaction) = load_item(c, id, &record.id)?;
    if position != record.position {
        return Err(revision_conflict());
    }
    Ok((item, interaction))
}
fn save_interaction(c: &Connection, id: &str, value: &Interaction) -> Result<(), AppError> {
    c.execute(
        "UPDATE assessment_session_items SET interaction_json=? WHERE id=?",
        params![serde_json::to_string(value)?, id],
    )?;
    Ok(())
}
fn bump(c: &Connection, id: &str) -> Result<(), AppError> {
    c.execute(
        "UPDATE assessment_sessions SET revision=revision+1,updated_at=? WHERE id=?",
        params![Utc::now().timestamp_millis(), id],
    )?;
    Ok(())
}

pub(crate) fn effective_answer(
    c: &Connection,
    id: &str,
) -> Result<(AssessmentAnswer, i64, Vec<AssessmentCorrection>), AppError> {
    let (raw, revision): (String, i64) = c.query_row(
        "SELECT answer_json,revision FROM assessment_answers WHERE id=?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut answer: AssessmentAnswer = serde_json::from_str(&raw)?;
    let mut q=c.prepare("SELECT correction_json FROM assessment_corrections WHERE answer_id=? ORDER BY json_extract(correction_json,'$.revision'),id")?;
    let corrections = q
        .query_map([id], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str::<AssessmentCorrection>(&r?)?))
        .collect::<Result<Vec<_>, AppError>>()?;
    for correction in &corrections {
        answer.hinted |= correction.add_hint;
        if correction.result == "invalid" {
            answer.unscored_reason = Some("invalid_item".into());
        } else {
            answer.result = correction.result.clone();
            answer.unscored_reason = None;
        }
    }
    Ok((answer, revision, corrections))
}
fn feedback(
    c: &Connection,
    answer_id: &str,
    item: &FrozenItem,
) -> Result<AssessmentFeedback, AppError> {
    let (answer, revision, _) = effective_answer(c, answer_id)?;
    Ok(AssessmentFeedback {
        answer_id: answer.id,
        answer_revision: revision,
        session_item_id: answer.session_item_id,
        question_type: item.question_type.clone(),
        target: item.content.target.clone(),
        sentence: item.content.sentence.clone(),
        meaning: item.content.meaning.clone(),
        translation: item.content.translation.clone(),
        explanation: item.content.explanation.clone(),
        accepted_answers: item.content.accepted_answers.clone(),
        result: answer.result,
        unscored_reason: answer.unscored_reason,
        uncertain: answer.uncertain,
        hinted: answer.hinted,
    })
}
pub(crate) fn session_view(c: &Connection, id: &str) -> Result<AssessmentSession, AppError> {
    let record = load_session(c, id)?;
    let planned: usize = c.query_row(
        "SELECT COUNT(*) FROM assessment_session_items WHERE session_id=?",
        [id],
        |r| r.get(0),
    )?;
    let processed: usize = c.query_row(
        "SELECT COUNT(*) FROM assessment_answers WHERE session_id=?",
        [id],
        |r| r.get(0),
    )?;
    let question = if record.state == "active" {
        let row:Option<(String,String,String)>=c.query_row("SELECT id,snapshot_json,interaction_json FROM assessment_session_items WHERE session_id=? AND position=?",params![id,record.position],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        row.map(|(id, snapshot, raw)| {
            let item: FrozenItem = serde_json::from_str(&snapshot)?;
            let interaction: Interaction = serde_json::from_str(&raw)?;
            let prompt = if item.question_type == "listening" {
                "请播放英语词语及例句，选择所考查表达的含义。".into()
            } else {
                item.content.prompt
            };
            Ok::<_, AppError>(AssessmentQuestion {
                id,
                position: record.position,
                question_type: item.question_type,
                prompt,
                choices: item.content.choices,
                audio_kind: item.content.audio.as_ref().map(|a| a.kind.clone()),
                audio_played: interaction.audio_played,
                hint: interaction
                    .hinted
                    .then(|| format!("语境提示：{}", item.content.translation)),
                hinted: interaction.hinted,
                draft_text: interaction.draft_text,
                draft_option: interaction.draft_option,
            })
        })
        .transpose()?
    } else {
        None
    };
    let last:Option<(String,String)>=c.query_row("SELECT a.id,i.snapshot_json FROM assessment_answers a JOIN assessment_session_items i ON i.id=a.session_item_id WHERE a.session_id=? ORDER BY i.position DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let last_feedback = last
        .map(|(answer_id, snapshot)| feedback(c, &answer_id, &serde_json::from_str(&snapshot)?))
        .transpose()?;
    Ok(AssessmentSession {
        id: record.id,
        state: record.state,
        revision: record.revision,
        position: record.position,
        planned,
        processed,
        config: record.config,
        created_at: record.created_at,
        updated_at: record.updated_at,
        question,
        last_feedback,
        warnings: record.sampling.warnings,
        policy_version: record.sampling.policy_version,
    })
}
fn answer_key(value: &str) -> String {
    normalize(value).replace(['’', '‘', 'ʼ'], "'")
}

impl Store {
    pub fn assessment_preview(
        &self,
        config: &AssessmentConfig,
    ) -> Result<AssessmentPreview, AppError> {
        let c = self.connection()?;
        Ok(load_inventory(self, &c, config)?.preview)
    }
    pub fn assessment_start(&self, input: &StartAssessment) -> Result<AssessmentSession, AppError> {
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let result = operation(&tx, &input.operation_id, "session_start", input, |tx| {
            let inventory = load_inventory(self, tx, &input.config)?;
            if inventory.preview.inventory_revision != input.expected_inventory_revision {
                return Err(AppError::new(
                    "assessment_inventory_changed",
                    "题库或近期记录已变化，请重新预览。",
                ));
            }
            let id = Uuid::new_v4().to_string();
            let seed = Uuid::new_v4().to_string();
            let (sampling, items) = inventory.freeze(&input.config, &seed)?;
            let now = Utc::now().timestamp_millis();
            tx.execute("INSERT INTO assessment_sessions(id,state,revision,config_json,sampling_json,created_at,updated_at) VALUES (?,'active',1,?,?,?,?)",params![id,serde_json::to_string(&input.config)?,serde_json::to_string(&sampling)?,now,now])?;
            for (position, item) in items.into_iter().enumerate() {
                tx.execute("INSERT INTO assessment_session_items(id,session_id,position,snapshot_json) VALUES (?,?,?,?)",params![Uuid::new_v4().to_string(),id,position,serde_json::to_string(&item)?])?;
            }
            session_view(tx, &id)
        })?;
        tx.commit()?;
        Ok(result)
    }
    pub fn assessment_session(&self, id: &str) -> Result<AssessmentSession, AppError> {
        let connection = self.connection()?;
        session_view(&connection, id)
    }
    pub fn assessment_session_list(
        &self,
        unfinished_only: bool,
        offset: usize,
        limit: usize,
    ) -> Result<Value, AppError> {
        let c = self.connection()?;
        let total: usize = c.query_row(
            "SELECT COUNT(*) FROM assessment_sessions WHERE NOT ? OR state IN ('active','paused')",
            [unfinished_only],
            |r| r.get(0),
        )?;
        let mut q=c.prepare("SELECT id FROM assessment_sessions WHERE NOT ?1 OR state IN ('active','paused') ORDER BY created_at DESC,id LIMIT ?2 OFFSET ?3")?;
        let ids = q
            .query_map(params![unfinished_only, limit.clamp(1, 100), offset], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let items=ids.iter().map(|id|{let s=load_session(&c,id)?;Ok(json!({"id":s.id,"state":s.state,"revision":s.revision,"position":s.position,"config":s.config,"createdAt":s.created_at,"updatedAt":s.updated_at}))}).collect::<Result<Vec<_>,AppError>>()?;
        Ok(json!({"items":items,"total":total}))
    }
    pub fn assessment_answer_submit(
        &self,
        input: &AssessmentAnswerInput,
    ) -> Result<AssessmentSession, AppError> {
        if !matches!(input.action.as_str(), "answer" | "unknown" | "skip")
            || input.text.chars().count() > 4000
        {
            return Err(AppError::new(
                "invalid_input",
                "请提供有效作答，或选择不知道。",
            ));
        }
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let result = operation(&tx, &input.operation_id, "answer_submit", input, |tx| {
            let record = load_session(tx, &input.session_id)?;
            active(&record, Some(input.expected_revision))?;
            if record.sampling.score_policy != SCORE_POLICY {
                return Err(AppError::new(
                    "unsupported_schema",
                    "当前程序不支持此会话的评分版本。",
                ));
            }
            let (item, interaction) = current(tx, &record, &input.item_id)?;
            let mut outcome = "unknown";
            if input.action == "answer" {
                if item.question_type == "cloze" {
                    if input.text.trim().is_empty() {
                        return Err(AppError::new(
                            "invalid_input",
                            "请输入答案，或明确选择不知道。",
                        ));
                    }
                    outcome = if item
                        .content
                        .accepted_answers
                        .iter()
                        .any(|a| answer_key(a) == answer_key(&input.text))
                    {
                        "correct"
                    } else {
                        "needs_confirmation"
                    };
                } else {
                    let option = input
                        .option_id
                        .as_deref()
                        .ok_or_else(|| AppError::new("invalid_input", "请选择一个答案。"))?;
                    if !item.content.choices.iter().any(|c| c.id == option) {
                        return Err(AppError::new("invalid_input", "此选项不属于当前题目。"));
                    }
                    outcome = if item.content.correct_option_id.as_deref() == Some(option) {
                        "correct"
                    } else {
                        "incorrect"
                    };
                }
            }
            let unscored_reason = (item.question_type == "listening" && !interaction.audio_played)
                .then(|| {
                    if interaction.audio_error.is_some() {
                        "audio_error".into()
                    } else {
                        "audio_unplayed".into()
                    }
                });
            let answer = AssessmentAnswer {
                id: Uuid::new_v4().to_string(),
                session_id: record.id.clone(),
                session_item_id: input.item_id.clone(),
                original_action: input.action.clone(),
                option_id: input.option_id.clone(),
                text: input.text.clone(),
                result: outcome.into(),
                unscored_reason,
                uncertain: input.uncertain,
                hinted: interaction.hinted,
                audio_status: if interaction.audio_played {
                    "played"
                } else if interaction.audio_error.is_some() {
                    "error"
                } else {
                    "unplayed"
                }
                .into(),
                score_policy: record.sampling.score_policy.clone(),
                created_at: Utc::now().timestamp_millis(),
            };
            tx.execute("INSERT INTO assessment_answers(id,session_id,session_item_id,answer_json,created_at) VALUES (?,?,?,?,?)",params![answer.id,record.id,input.item_id,serde_json::to_string(&answer)?,answer.created_at])?;
            let total: usize = tx.query_row(
                "SELECT COUNT(*) FROM assessment_session_items WHERE session_id=?",
                [&record.id],
                |r| r.get(0),
            )?;
            tx.execute("UPDATE assessment_sessions SET position=position+1,state=?,revision=revision+1,updated_at=? WHERE id=?",params![if record.position+1==total{"completed"}else{"active"},answer.created_at,record.id])?;
            session_view(tx, &record.id)
        })?;
        tx.commit()?;
        Ok(result)
    }
    pub fn assessment_transition(
        &self,
        input: &TransitionAssessment,
    ) -> Result<AssessmentSession, AppError> {
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let result = operation(
            &tx,
            &input.operation_id,
            "session_transition",
            input,
            |tx| {
                let record = load_session(tx, &input.session_id)?;
                if record.revision != input.expected_revision {
                    return Err(revision_conflict());
                }
                match input.action.as_str() {
                    "pause" if record.state == "active" => {
                        tx.execute(
                            "UPDATE assessment_sessions SET state='paused' WHERE id=?",
                            [&record.id],
                        )?;
                    }
                    "resume" if record.state == "paused" => {
                        tx.execute(
                            "UPDATE assessment_sessions SET state='active' WHERE id=?",
                            [&record.id],
                        )?;
                    }
                    "end_early" if matches!(record.state.as_str(), "active" | "paused") => {
                        tx.execute(
                            "UPDATE assessment_sessions SET state='ended_early' WHERE id=?",
                            [&record.id],
                        )?;
                    }
                    "request_hint" | "save_draft" if record.state == "active" => {
                        let id = input
                            .item_id
                            .as_deref()
                            .ok_or_else(|| AppError::new("invalid_input", "请选择当前题目。"))?;
                        let (item, mut interaction) = current(tx, &record, id)?;
                        if input.action == "request_hint" {
                            interaction.hinted = true;
                            interaction.hint_count += 1;
                        } else {
                            if input.draft_text.chars().count() > 4000
                                || input.draft_option.as_ref().is_some_and(|id| {
                                    !item.content.choices.iter().any(|c| &c.id == id)
                                })
                            {
                                return Err(AppError::new("invalid_input", "当前题目的草稿无效。"));
                            }
                            interaction.draft_text = input.draft_text.clone();
                            interaction.draft_option = input.draft_option.clone();
                        }
                        save_interaction(tx, id, &interaction)?;
                    }
                    _ => return Err(AppError::new("invalid_input", "当前测试状态不支持此操作。")),
                }
                bump(tx, &record.id)?;
                session_view(tx, &record.id)
            },
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn assessment_correct(&self, input: &CorrectAssessment) -> Result<Value, AppError> {
        if !matches!(
            input.result.as_str(),
            "correct" | "incorrect" | "unknown" | "invalid"
        ) || input.reason.trim().is_empty()
            || input.reason.chars().count() > 4000
        {
            return Err(AppError::new("invalid_input", "请提供修正结论和依据。"));
        }
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let result = operation(&tx, &input.operation_id, "result_correct", input, |tx| {
            let (answer, revision, _) = effective_answer(tx, &input.answer_id)?;
            if revision != input.expected_revision {
                return Err(revision_conflict());
            }
            // An unheard recording cannot become a scored listening success through a correction.
            let (_, item, _) = load_item(tx, &answer.session_item_id, &answer.session_id)?;
            if item.question_type == "listening"
                && answer.audio_status != "played"
                && input.result != "invalid"
            {
                return Err(AppError::new(
                    "assessment_audio_unavailable",
                    "该题没有有效播放事实，只可标记题目无效，不能改为计分题。",
                ));
            }
            let correction = AssessmentCorrection {
                id: Uuid::new_v4().to_string(),
                answer_id: input.answer_id.clone(),
                revision: revision + 1,
                result: input.result.clone(),
                add_hint: input.add_hint,
                reason: input.reason.clone(),
                created_at: Utc::now().timestamp_millis(),
            };
            tx.execute(
                "INSERT INTO assessment_corrections VALUES (?,?,?,?)",
                params![
                    correction.id,
                    correction.answer_id,
                    serde_json::to_string(&correction)?,
                    correction.created_at
                ],
            )?;
            tx.execute(
                "UPDATE assessment_answers SET revision=revision+1 WHERE id=?",
                [&input.answer_id],
            )?;
            bump(tx, &answer.session_id)?;
            Ok(
                json!({"answerId":answer.id,"revision":revision+1,"sessionId":answer.session_id,"sessionRevision":load_session(tx,&answer.session_id)?.revision,"correction":correction}),
            )
        })?;
        tx.commit()?;
        Ok(result)
    }
    pub fn assessment_audio_start(
        &self,
        input: &AssessmentAudioStart,
    ) -> Result<AssessmentPlayback, AppError> {
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let result = operation(&tx, &input.operation_id, "audio_start", input, |tx| {
            let record = load_session(tx, &input.session_id)?;
            active(&record, Some(input.expected_revision))?;
            let (item, mut interaction) = current(tx, &record, &input.item_id)?;
            if item.question_type != "listening" {
                return Err(AppError::new("invalid_input", "当前不是听音题。"));
            }
            let audio = item.content.audio.as_ref().ok_or_else(|| {
                AppError::new("assessment_audio_unavailable", "该题没有已准备音频。")
            })?;
            let path = resolved_audio_on(self.root(), tx, audio)
                .and_then(|a| safe_audio_path(self.root(), &a));
            let playback_id = Uuid::new_v4().to_string();
            interaction.playback_id = Some(playback_id.clone());
            interaction.audio_started = false;
            interaction.audio_seconds = 0.0;
            interaction.audio_error = path.as_ref().err().map(ToString::to_string);
            save_interaction(tx, &input.item_id, &interaction)?;
            Ok(AssessmentPlayback {
                playback_id,
                path: path.as_ref().ok().map(|p| p.to_string_lossy().into_owned()),
                kind: audio.kind.clone(),
                error: path.err(),
            })
        })?;
        tx.commit()?;
        Ok(result)
    }
    pub fn assessment_audio_event(&self, input: &AssessmentAudioEvent) -> Result<Value, AppError> {
        if !input.seconds.is_finite()
            || input.seconds < 0.0
            || input.seconds > 86400.0
            || !matches!(
                input.event.as_str(),
                "started" | "progress" | "ended" | "error"
            )
        {
            return Err(AppError::new("invalid_input", "音频事件无效。"));
        }
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let result = operation(&tx, &input.operation_id, "audio_event", input, |tx| {
            let record = load_session(tx, &input.session_id)?;
            active(&record, None)?;
            let (item, mut interaction) = current(tx, &record, &input.item_id)?;
            if item.question_type != "listening"
                || interaction.playback_id.as_deref() != Some(&input.playback_id)
            {
                return Err(revision_conflict());
            }
            match input.event.as_str() {
                "started" => {
                    interaction.audio_started = true;
                    interaction.audio_error = None;
                }
                "ended" if interaction.audio_started && input.seconds > 0.0 => {
                    interaction.audio_played = true;
                    interaction.audio_error = None;
                }
                "progress" if interaction.audio_started => {}
                "error" => {
                    interaction.audio_error = Some(input.error.chars().take(500).collect());
                }
                _ => {
                    return Err(AppError::new(
                        "invalid_input",
                        "播放事件缺少开始或有效进度。",
                    ));
                }
            }
            interaction.audio_seconds = interaction.audio_seconds.max(input.seconds);
            save_interaction(tx, &input.item_id, &interaction)?;
            // Media acknowledgements don't invalidate an in-progress answer's expected session revision.
            tx.execute(
                "UPDATE assessment_sessions SET updated_at=? WHERE id=?",
                params![Utc::now().timestamp_millis(), record.id],
            )?;
            Ok(
                json!({"playbackId":input.playback_id,"played":interaction.audio_played,"seconds":interaction.audio_seconds,"error":interaction.audio_error}),
            )
        })?;
        tx.commit()?;
        Ok(result)
    }
}
pub(crate) fn resolved_audio_on(
    root: &std::path::Path,
    c: &Connection,
    audio: &AssessmentAudio,
) -> Result<AssessmentAudio, AppError> {
    if let Ok(path) = safe_audio_path(root, audio) {
        if std::fs::read(path).is_ok_and(|bytes| digest(&bytes) == audio.sha256) {
            return Ok(audio.clone());
        }
    }
    if audio.kind == "system" {
        let saved: Option<String> = c
            .query_row(
                "SELECT value_json FROM settings WHERE key=?",
                [format!("assessment_audio_cache:{}", audio.sha256)],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(saved) = saved {
            let cached: AssessmentAudio = serde_json::from_str(&saved)?;
            if cached.kind == "system" && cached.text == audio.text {
                let path = safe_audio_path(root, &cached)?;
                if std::fs::read(path).is_ok_and(|bytes| digest(&bytes) == cached.sha256) {
                    return Ok(cached);
                }
            }
        }
    }
    Err(AppError::new(
        "assessment_audio_unavailable",
        "本地听音资源尚未准备，可重新准备或跳过并标为未计分。",
    ))
}
pub(crate) fn validated_audio(
    store: &Store,
    audio: &AssessmentAudio,
) -> Result<std::path::PathBuf, AppError> {
    let c = store.connection()?;
    let resolved = resolved_audio_on(store.root(), &c, audio)?;
    safe_audio_path(store.root(), &resolved)
}
