use crate::{
    error::AppError,
    review_policy::{Outcome, POLICY_VERSION, ReviewState},
    store::Store,
    vocabulary::{digest, normalize},
};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewUnit {
    pub id: String,
    pub entry_id: String,
    pub text: String,
    pub kind: String,
    pub scope: String,
    pub dimension: String,
    pub state: ReviewState,
    pub revision: i64,
    pub reasons: Vec<String>,
    pub available: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::{CollectionInput, EntryUpdate, ExampleInput, MeaningUpdate};
    const DAY: i64 = 86_400_000;

    #[test]
    fn comparison_does_not_accept_different_word_boundaries_or_negation() {
        assert_eq!(
            grade("not able", &["notable".into()]),
            Outcome::NeedsConfirmation
        );
        assert_eq!(
            grade("不愿意", &["愿意".into()]),
            Outcome::NeedsConfirmation
        );
        assert_eq!(grade(" HELLO! ", &["hello".into()]), Outcome::Correct);
    }

    fn fixture() -> (tempfile::TempDir, Store, String) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let result = store
            .collect(&CollectionInput {
                operation_id: Uuid::new_v4().to_string(),
                kind: "word".into(),
                text: "reluctant".into(),
                meaning: "不情愿的；勉强的".into(),
                examples: vec![ExampleInput {
                    text: "She was reluctant to ask for help.".into(),
                    context_meaning: "不愿求助".into(),
                    ..Default::default()
                }],
                target_entry_id: None,
                expected_revision: None,
            })
            .unwrap();
        (directory, store, result.entry_id)
    }
    fn unit(store: &Store, dimension: &str) -> ReviewUnit {
        store
            .review_units("all", dimension, 0, 100)
            .unwrap()
            .remove(0)
    }
    fn answer(store: &Store, dimension: &str, text: &str, unable: bool) -> Attempt {
        let u = unit(store, dimension);
        let q = store.review_question(&u.id, u.revision).unwrap();
        if dimension == "listening" {
            store.review_audio_prepared(&q.id).unwrap();
        }
        store
            .review_submit(&AnswerInput {
                operation_id: Uuid::new_v4().to_string(),
                question_id: q.id,
                answer: text.into(),
                unable,
            })
            .unwrap()
    }
    fn answer_at(store: &Store, dimension: &str, text: &str, unable: bool, time: i64) -> Attempt {
        let a = answer(store, dimension, text, unable);
        let c = store.connection().unwrap();
        c.execute(
            "UPDATE review_attempts SET created_at=? WHERE id=?",
            params![time, a.id],
        )
        .unwrap();
        recompute(&c, &a.unit_id).unwrap();
        attempt_on(&c, &a.id).unwrap()
    }
    fn correction(a: &Attempt, outcome: Outcome) -> CorrectionInput {
        CorrectionInput {
            operation_id: Uuid::new_v4().to_string(),
            attempt_id: a.id.clone(),
            expected_revision: a.revision,
            outcome,
            reason: "核对语境后的人工确认".into(),
        }
    }

    #[test]
    fn unanswered_question_has_no_reference_answer_or_history() {
        let (_d, store, _) = fixture();
        let meaning = unit(&store, "meaning");
        let q = store
            .review_question(&meaning.id, meaning.revision)
            .unwrap();
        let json = serde_json::to_string(&q).unwrap();
        assert!(!json.contains("不情愿"));
        assert!(!json.contains("expected"));
        let listening = unit(&store, "listening");
        let q = store
            .review_question(&listening.id, listening.revision)
            .unwrap();
        assert!(!serde_json::to_string(&q).unwrap().contains("reluctant"));
        assert!(store.review_history(None, 0, 50).unwrap().is_empty());
        assert_eq!(unit(&store, "meaning").state.streak, 0);
    }
    #[test]
    fn ambiguous_answer_is_durable_and_retries_never_count_twice() {
        let (d, store, _) = fixture();
        let u = unit(&store, "meaning");
        let q = store.review_question(&u.id, u.revision).unwrap();
        let input = AnswerInput {
            operation_id: Uuid::new_v4().to_string(),
            question_id: q.id,
            answer: "不太愿意".into(),
            unable: false,
        };
        let a = store.review_submit(&input).unwrap();
        assert_eq!(a.outcome, Outcome::NeedsConfirmation);
        assert_eq!(a.state.due_at, u.state.due_at);
        assert_eq!(store.review_submit(&input).unwrap().id, a.id);
        let mut changed = input.clone();
        changed.answer = "另一答案".into();
        assert_eq!(store.review_submit(&changed).unwrap_err().code, "conflict");
        drop(store);
        let store = Store::open(d.path()).unwrap();
        assert_eq!(store.review_history(None, 0, 50).unwrap().len(), 1);
        let fixed = store
            .review_correct(&correction(&a, Outcome::Correct))
            .unwrap();
        assert_eq!(fixed.original_outcome, Outcome::NeedsConfirmation);
        assert_eq!(fixed.outcome, Outcome::Correct);
        assert_eq!(fixed.answer, "不太愿意");
        assert_eq!(fixed.state.streak, 1);
    }
    #[test]
    fn hint_and_manually_declared_hint_cannot_be_erased_by_correction() {
        let (_d, store, _) = fixture();
        let u = unit(&store, "meaning");
        let q = store.review_question(&u.id, u.revision).unwrap();
        store.review_hint(&q.id).unwrap();
        let a = store
            .review_submit(&AnswerInput {
                operation_id: Uuid::new_v4().to_string(),
                question_id: q.id,
                answer: "勉强的".into(),
                unable: false,
            })
            .unwrap();
        assert_eq!(a.outcome, Outcome::CorrectWithHint);
        assert_eq!(a.state.interval_ms, 600_000);
        let a = store
            .review_correct(&correction(&a, Outcome::Correct))
            .unwrap();
        assert!(a.hinted);
        assert_eq!(a.outcome, Outcome::CorrectWithHint);
        let b = answer(&store, "meaning", "不情愿的", false);
        let b = store
            .review_correct(&correction(&b, Outcome::CorrectWithHint))
            .unwrap();
        let b = store
            .review_correct(&correction(&b, Outcome::Correct))
            .unwrap();
        assert!(b.hinted);
        assert_eq!(b.outcome, Outcome::CorrectWithHint);
    }
    #[test]
    fn dimensions_remain_independent_and_unavailable_audio_does_not_fail_learning() {
        let (_d, store, _) = fixture();
        let a = answer(&store, "meaning", "不情愿的", false);
        let listening = unit(&store, "listening");
        let q = store
            .review_question(&listening.id, listening.revision)
            .unwrap();
        assert_eq!(
            store
                .review_submit(&AnswerInput {
                    operation_id: Uuid::new_v4().to_string(),
                    question_id: q.id,
                    answer: String::new(),
                    unable: true
                })
                .unwrap_err()
                .code,
            "resource_missing"
        );
        assert_eq!(store.review_history(None, 0, 50).unwrap().len(), 1);
        let b = answer(&store, "listening", "", true);
        assert_eq!(b.state.lapses, 1);
        assert_eq!(b.state.interval_ms, 300_000);
        assert_eq!(unit(&store, "meaning").state.due_at, a.state.due_at);
        assert_eq!(unit(&store, "meaning").state.lapses, 0);
    }
    #[test]
    fn correcting_older_result_replays_later_results_and_is_idempotent() {
        let (_d, store, _) = fixture();
        let base = unit(&store, "meaning").state.due_at + 1000;
        let first = answer_at(&store, "meaning", "", true, base);
        let second = answer_at(&store, "meaning", "不情愿的", false, base + 300_000);
        assert_eq!(second.state.due_at, base + 300_000 + DAY);
        let listening_before = unit(&store, "listening");
        let request = correction(&first, Outcome::Correct);
        let fixed = store.review_correct(&request).unwrap();
        assert_eq!(fixed.state.lapses, 0);
        assert_eq!(fixed.state.streak, 1);
        assert_eq!(fixed.state.due_at, base + DAY);
        assert_eq!(store.review_correct(&request).unwrap().revision, 2);
        assert_eq!(
            unit(&store, "listening").revision,
            listening_before.revision
        );
        assert_eq!(
            store
                .review_history(Some(&first.unit_id), 0, 10)
                .unwrap()
                .len(),
            2
        );
        let mut changed = request;
        changed.outcome = Outcome::Incorrect;
        assert_eq!(store.review_correct(&changed).unwrap_err().code, "conflict");
    }
    #[test]
    fn mastery_recollection_preserves_history_and_resets_all_scopes() {
        let (_d, store, id) = fixture();
        for _ in 0..5 {
            let due = unit(&store, "meaning").state.due_at.max(1);
            answer_at(&store, "meaning", "不情愿的", false, due);
        }
        assert_eq!(unit(&store, "meaning").state.level, "mastered");
        let entry = store.get_entry(&id).unwrap();
        store
            .collect(&CollectionInput {
                operation_id: Uuid::new_v4().to_string(),
                kind: "word".into(),
                text: entry.text,
                meaning: "犹豫的".into(),
                examples: vec![],
                target_entry_id: Some(id.clone()),
                expected_revision: Some(entry.revision),
            })
            .unwrap();
        let units = store.review_units("all", "meaning", 0, 50).unwrap();
        assert_eq!(units.len(), 2);
        assert!(
            units
                .iter()
                .all(|u| u.state.streak == 0 && u.state.level != "mastered")
        );
        assert_eq!(store.review_history(None, 0, 50).unwrap().len(), 5);
        assert_eq!(store.get_entry(&id).unwrap().examples.len(), 1);
    }
    #[test]
    fn special_requires_three_due_passes_and_old_collection_count_stays_cleared() {
        let (_d, store, id) = fixture();
        let base = unit(&store, "meaning").state.due_at + 1000;
        for n in 0..3 {
            answer_at(&store, "meaning", "", true, base + n);
        }
        assert_eq!(
            store.review_units("leech", "meaning", 0, 50).unwrap().len(),
            1
        );
        for n in 3..6 {
            answer_at(&store, "meaning", "不情愿的", false, base + n);
        }
        assert!(unit(&store, "meaning").state.leech_active);
        for _ in 0..3 {
            let due = unit(&store, "meaning").state.due_at;
            answer_at(&store, "meaning", "不情愿的", false, due);
        }
        assert!(
            store
                .review_units("leech", "meaning", 0, 50)
                .unwrap()
                .is_empty()
        );
        let cleared = unit(&store, "meaning").state.leech_cleared_at;
        assert!(cleared > 0);
        let e = store.get_entry(&id).unwrap();
        store
            .collect(&CollectionInput {
                operation_id: Uuid::new_v4().to_string(),
                kind: e.kind,
                text: e.text,
                meaning: e.meanings[0].text.clone(),
                examples: vec![],
                target_entry_id: Some(id),
                expected_revision: Some(e.revision),
            })
            .unwrap();
        assert!(
            store
                .review_units("leech", "meaning", 0, 50)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn collection_retry_does_not_trigger_special_and_new_actions_do() {
        let (_d, store, id) = fixture();
        for _ in 0..2 {
            let e = store.get_entry(&id).unwrap();
            let input = CollectionInput {
                operation_id: Uuid::new_v4().to_string(),
                kind: e.kind,
                text: e.text,
                meaning: e.meanings[0].text.clone(),
                examples: vec![],
                target_entry_id: Some(id.clone()),
                expected_revision: Some(e.revision),
            };
            store.collect(&input).unwrap();
            store.collect(&input).unwrap();
        }
        let units = store.review_units("leech", "", 0, 50).unwrap();
        assert_eq!(units.len(), 2);
        assert!(units.iter().all(|u| !u.reasons.is_empty()));
        assert_eq!(store.get_entry(&id).unwrap().collection_count, 3);
    }
    #[test]
    fn editing_entry_invalidates_open_question_but_retains_old_attempt_snapshot() {
        let (_d, store, id) = fixture();
        let a = answer(&store, "meaning", "不情愿的", false);
        let u = unit(&store, "meaning");
        let q = store.review_question(&u.id, u.revision).unwrap();
        let e = store.get_entry(&id).unwrap();
        store
            .update_entry(&EntryUpdate {
                id,
                expected_revision: e.revision,
                text: "Reluctant".into(),
                meanings: vec![MeaningUpdate {
                    id: Some(e.meanings[0].id.clone()),
                    text: "犹豫的".into(),
                }],
            })
            .unwrap();
        assert_eq!(
            store
                .review_submit(&AnswerInput {
                    operation_id: Uuid::new_v4().to_string(),
                    question_id: q.id,
                    answer: "不情愿的".into(),
                    unable: false
                })
                .unwrap_err()
                .code,
            "conflict"
        );
        assert_eq!(
            store.review_history(None, 0, 50).unwrap()[0]
                .question
                .expected,
            a.question.expected
        );
    }
    #[test]
    fn raw_original_listening_masks_target_and_retains_exact_association() {
        let (_d, store, id) = fixture();
        let e = store.get_entry(&id).unwrap();
        let c = store.connection().unwrap();
        c.execute("INSERT INTO media_assets(id,recipe_key,digest,relative_path,format,duration_ms,state,created_at) VALUES ('fixture-audio','fixture','abc','media/fixture.wav','wav',1000,'ready',0)",[]).unwrap();
        c.execute(
            "INSERT INTO example_media(example_id,asset_id) VALUES (?,'fixture-audio')",
            [&e.examples[0].id],
        )
        .unwrap();
        drop(c);
        let u = unit(&store, "listening");
        let q = store.review_question(&u.id, u.revision).unwrap();
        assert_eq!(q.context, "She was ____ to ask for help.");
        assert_eq!(q.audio_kind, "original");
        assert!(!serde_json::to_string(&q).unwrap().contains("reluctant"));
        assert_eq!(
            store.review_audio(&q.id).unwrap().asset_id.as_deref(),
            Some("fixture-audio")
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSnapshot {
    pub entry_id: String,
    pub target: String,
    pub dimension: String,
    pub expected: Vec<String>,
    pub context: String,
    pub asset_id: Option<String>,
    pub audio_kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub unit_id: String,
    pub dimension: String,
    pub prompt: String,
    pub context: String,
    pub audio_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerInput {
    pub operation_id: String,
    pub question_id: String,
    pub answer: String,
    pub unable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionInput {
    pub operation_id: String,
    pub attempt_id: String,
    pub expected_revision: i64,
    pub outcome: Outcome,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub id: String,
    pub unit_id: String,
    pub question: QuestionSnapshot,
    pub answer: String,
    pub hinted: bool,
    pub original_outcome: Outcome,
    pub outcome: Outcome,
    pub grader: String,
    pub revision: i64,
    pub created_at: i64,
    pub state: ReviewState,
    pub corrections: Vec<Correction>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Correction {
    pub outcome: Outcome,
    pub reason: String,
    pub created_at: i64,
}

fn invalid(message: &str) -> AppError {
    AppError::new("invalid_input", message)
}
fn conflict() -> AppError {
    AppError::new("conflict", "复习资料或安排已变化，请重新打开题目。")
}
fn outcome_text(outcome: Outcome) -> Result<String, AppError> {
    Ok(serde_json::to_value(outcome)?.as_str().unwrap().to_owned())
}
fn parse_outcome(text: &str) -> Result<Outcome, AppError> {
    Ok(serde_json::from_value(serde_json::Value::String(
        text.into(),
    ))?)
}
fn hinted_outcome(outcome: Outcome, hinted: bool) -> Outcome {
    if hinted && outcome == Outcome::Correct {
        Outcome::CorrectWithHint
    } else {
        outcome
    }
}
pub(crate) fn event_time(connection: &Connection) -> Result<i64, AppError> {
    let last: i64 = connection.query_row("SELECT MAX(t) FROM (SELECT COALESCE(MAX(created_at),0) t FROM collection_actions UNION ALL SELECT COALESCE(MAX(created_at),0) FROM review_attempts)", [], |r| r.get(0))?;
    Ok(Utc::now().timestamp_millis().max(last.saturating_add(1)))
}

/// Called inside the vocabulary transaction: all meanings share the same unit rules.
pub(crate) fn ensure_units(
    connection: &Connection,
    entry_id: &str,
    now: i64,
) -> Result<(), AppError> {
    let mut query =
        connection.prepare("SELECT id FROM meanings WHERE entry_id=? ORDER BY created_at,id")?;
    let mut scopes = query
        .query_map([entry_id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if scopes.is_empty() {
        scopes.push("entry".into());
    }
    for scope in scopes {
        for dimension in ["meaning", "listening"] {
            connection.execute("INSERT OR IGNORE INTO learning_units(id,entry_id,scope_key,dimension,created_at) VALUES (?,?,?,?,?)",
                params![Uuid::new_v4().to_string(), entry_id, scope, dimension, now])?;
        }
    }
    let mut query =
        connection.prepare("SELECT id,created_at FROM learning_units WHERE entry_id=?")?;
    let units = query
        .query_map([entry_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, created_at) in units {
        let state = ReviewState {
            due_at: created_at,
            ..Default::default()
        };
        connection.execute(
            "INSERT OR IGNORE INTO review_states(unit_id,state_json,due_at) VALUES (?,?,?)",
            params![id, serde_json::to_string(&state)?, created_at],
        )?;
    }
    Ok(())
}

pub(crate) fn recompute_entry(connection: &Connection, entry_id: &str) -> Result<(), AppError> {
    let mut query = connection.prepare("SELECT id FROM learning_units WHERE entry_id=?")?;
    let ids = query
        .query_map([entry_id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        recompute(connection, &id)?;
    }
    Ok(())
}

/// Replays original collection events and effective attempts in order, retaining each policy version.
fn recompute(connection: &Connection, unit_id: &str) -> Result<ReviewState, AppError> {
    let (entry_id, created_at): (String, i64) = connection.query_row(
        "SELECT entry_id,created_at FROM learning_units WHERE id=?",
        [unit_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut state = ReviewState {
        due_at: created_at,
        ..Default::default()
    };
    let mut query = connection.prepare("SELECT created_at,0,operation_id,'' FROM collection_actions WHERE entry_id=? AND created_at>=?
        UNION ALL SELECT created_at,1,id,policy_version FROM review_attempts WHERE unit_id=? ORDER BY 1,2,3")?;
    let events = query
        .query_map(params![entry_id, created_at, unit_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut relearn_at = 0;
    for (time, kind, id, policy) in events {
        let count: u32 = connection.query_row("SELECT COUNT(*) FROM collection_actions WHERE entry_id=? AND created_at>? AND created_at<=?",
            params![entry_id,state.leech_cleared_at,time],|r|r.get(0))?;
        if kind == 0 {
            relearn_at = time;
            state.streak = 0;
            state.interval_ms = 0;
            state.level = "learning".into();
            state.due_at = time;
            if state.is_leech(count) && !state.leech_active {
                state.leech_active = true;
                state.leech_started_at = time;
            }
        } else {
            if policy != POLICY_VERSION {
                return Err(AppError::new(
                    "unsupported_policy",
                    "缺少历史复习策略，不能静默重算。",
                ));
            }
            let (original, hinted): (String, bool) = connection.query_row(
                "SELECT outcome,hinted FROM review_attempts WHERE id=?",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let corrected:Option<String> = connection.query_row("SELECT outcome FROM review_corrections WHERE attempt_id=? ORDER BY expected_revision DESC LIMIT 1",[&id],|r|r.get(0)).optional()?;
            let outcome = hinted_outcome(
                parse_outcome(corrected.as_deref().unwrap_or(&original))?,
                hinted,
            );
            let before = serde_json::to_string(&state)?;
            if outcome != Outcome::NeedsConfirmation {
                state = state.apply(outcome, time, 0, count)?;
            }
            connection.execute(
                "UPDATE review_attempts SET state_before_json=?,state_after_json=? WHERE id=?",
                params![before, serde_json::to_string(&state)?, id],
            )?;
        }
    }
    connection.execute("UPDATE review_states SET state_json=?,due_at=?,relearn_at=?,revision=revision+1 WHERE unit_id=?",
        params![serde_json::to_string(&state)?,state.due_at,relearn_at,unit_id])?;
    Ok(state)
}

fn snapshot(connection: &Connection, unit_id: &str) -> Result<QuestionSnapshot, AppError> {
    let (entry_id,target,dimension,scope):(String,String,String,String) = connection.query_row(
        "SELECT u.entry_id,e.text,u.dimension,u.scope_key FROM learning_units u JOIN entries e ON e.id=u.entry_id WHERE u.id=?",
        [unit_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?.ok_or_else(||AppError::new("not_found","学习单元不存在。"))?;
    let expected = if dimension == "listening" {
        vec![target.clone()]
    } else {
        let mut query=connection.prepare("SELECT text FROM meanings WHERE entry_id=? AND (?='entry' OR id=?) ORDER BY created_at,id")?;
        query
            .query_map(params![entry_id, scope, scope], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let example:Option<(String,Option<String>)> = connection.query_row(
        "SELECT e.text,(SELECT a.id FROM example_media em JOIN media_assets a ON a.id=em.asset_id WHERE em.example_id=e.id AND a.kind='original' ORDER BY a.created_at,a.id LIMIT 1)
         FROM entry_examples ee JOIN examples e ON e.id=ee.example_id WHERE ee.entry_id=? AND (?='entry' OR ee.meaning_id=?)
         ORDER BY (EXISTS(SELECT 1 FROM example_media em WHERE em.example_id=e.id)) DESC,e.created_at,e.id LIMIT 1",
        params![entry_id,scope,scope],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (context, mut asset_id) = example.unwrap_or_default();
    if dimension == "listening" && mask_target(&context, &target) == context {
        asset_id = None;
    }
    Ok(QuestionSnapshot {
        entry_id,
        target,
        dimension,
        expected,
        context,
        audio_kind: if asset_id.is_some() {
            "original"
        } else {
            "system"
        }
        .into(),
        asset_id,
    })
}

fn attempt_on(connection: &Connection, id: &str) -> Result<Attempt, AppError> {
    let (unit_id,snapshot,answer,hinted,original,grader,revision,created_at):(String,String,String,bool,String,String,i64,i64)=connection.query_row(
        "SELECT unit_id,question_json,answer,hinted,outcome,grader,revision,created_at FROM review_attempts WHERE id=?",[id],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional()?.ok_or_else(||AppError::new("not_found","作答记录不存在。"))?;
    let mut query=connection.prepare("SELECT outcome,reason,created_at FROM review_corrections WHERE attempt_id=? ORDER BY expected_revision")?;
    let rows = query
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let corrections = rows
        .into_iter()
        .map(|(outcome, reason, created_at)| {
            Ok(Correction {
                outcome: parse_outcome(&outcome)?,
                reason,
                created_at,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let original_outcome = parse_outcome(&original)?;
    let outcome = hinted_outcome(
        corrections
            .last()
            .map(|c| c.outcome)
            .unwrap_or(original_outcome),
        hinted,
    );
    let state: String = connection.query_row(
        "SELECT state_json FROM review_states WHERE unit_id=?",
        [&unit_id],
        |r| r.get(0),
    )?;
    Ok(Attempt {
        id: id.into(),
        unit_id,
        question: serde_json::from_str(&snapshot)?,
        answer,
        hinted,
        original_outcome,
        outcome,
        grader,
        revision,
        created_at,
        state: serde_json::from_str(&state)?,
        corrections,
    })
}

fn comparison(text: &str) -> String {
    normalize(text)
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() || c == '\'' {
                c
            } else if c == '’' {
                '\''
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn mask_target(context: &str, target: &str) -> String {
    let prefix = if target.chars().next().is_some_and(|c| c.is_alphanumeric()) {
        "\\b"
    } else {
        ""
    };
    let suffix = if target.chars().last().is_some_and(|c| c.is_alphanumeric()) {
        "\\b"
    } else {
        ""
    };
    regex::Regex::new(&format!("(?i){prefix}{}{suffix}", regex::escape(target)))
        .map(|r| r.replace_all(context, "____").into_owned())
        .unwrap_or_default()
}
fn grade(answer: &str, expected: &[String]) -> Outcome {
    let answer = comparison(answer);
    if !answer.is_empty()
        && expected.iter().any(|text| {
            comparison(text) == answer
                || text
                    .split(['；', ';', '|'])
                    .any(|part| comparison(part) == answer)
        })
    {
        Outcome::Correct
    } else {
        Outcome::NeedsConfirmation
    }
}

impl Store {
    pub fn review_units(
        &self,
        mode: &str,
        dimension: &str,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<ReviewUnit>, AppError> {
        if !matches!(mode, "due" | "all" | "leech")
            || !matches!(dimension, "" | "meaning" | "listening")
        {
            return Err(invalid("复习筛选无效。"));
        }
        let connection = self.connection()?;
        let now = Utc::now().timestamp_millis();
        let mut query=connection.prepare("SELECT u.id,u.entry_id,e.text,e.kind,u.scope_key,u.dimension,r.state_json,r.revision,
            (SELECT COUNT(*) FROM collection_actions c WHERE c.entry_id=u.entry_id AND c.created_at>COALESCE(json_extract(r.state_json,'$.leechClearedAt'),0))
            FROM learning_units u JOIN entries e ON e.id=u.entry_id JOIN review_states r ON r.unit_id=u.id
            WHERE e.archived=0 AND (?='' OR u.dimension=?) AND (u.scope_key<>'entry' OR NOT EXISTS(SELECT 1 FROM meanings m WHERE m.entry_id=u.entry_id))
            AND (?<>'due' OR r.due_at<=?) AND (?<>'leech' OR COALESCE(json_extract(r.state_json,'$.leechActive'),0)=1 OR
            ((SELECT COUNT(*) FROM collection_actions c WHERE c.entry_id=u.entry_id AND c.created_at>COALESCE(json_extract(r.state_json,'$.leechClearedAt'),0))>=3 AND COALESCE(json_extract(r.state_json,'$.streak'),0)<3))
            ORDER BY r.due_at,u.id LIMIT ? OFFSET ?")?;
        let rows = query
            .query_map(
                params![
                    dimension,
                    dimension,
                    mode,
                    now,
                    mode,
                    limit.clamp(1, 100),
                    offset
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, i64>(7)?,
                        r.get::<_, u32>(8)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(
                |(id, entry_id, text, kind, scope, dimension, json, revision, count)| {
                    let mut state: ReviewState = serde_json::from_str(&json)?;
                    let (due_at, relearn_at): (i64, i64) = connection.query_row(
                        "SELECT due_at,relearn_at FROM review_states WHERE unit_id=?",
                        [&id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?;
                    state.due_at = due_at;
                    if relearn_at > state.last_reviewed_at {
                        state.streak = 0;
                        state.level = "learning".into();
                    }
                    let mut reasons = vec![];
                    if state.is_leech(count) {
                        if count >= 3 {
                            reasons.push("多次主动收录，尚需巩固".into());
                        }
                        if state
                            .recent
                            .iter()
                            .filter(|o| **o == Outcome::Incorrect)
                            .count()
                            >= 3
                        {
                            reasons.push(
                                if dimension == "listening" {
                                    "近期听力识别多次失败"
                                } else if kind == "phrase" {
                                    "近期短语含义或用法多次失败"
                                } else {
                                    "近期词义回忆多次失败"
                                }
                                .into(),
                            );
                        }
                        if reasons.is_empty() {
                            reasons.push("专项尚未达到三次到期独立回忆通过".into());
                        }
                    }
                    let available = !snapshot(&connection, &id)?.expected.is_empty();
                    Ok(ReviewUnit {
                        id,
                        entry_id,
                        text,
                        kind,
                        scope,
                        dimension,
                        state,
                        revision,
                        reasons,
                        available,
                    })
                },
            )
            .collect()
    }

    pub fn review_question(
        &self,
        unit_id: &str,
        expected_revision: i64,
    ) -> Result<Question, AppError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let revision: Option<i64> = transaction
            .query_row(
                "SELECT revision FROM review_states WHERE unit_id=?",
                [unit_id],
                |r| r.get(0),
            )
            .optional()?;
        if revision != Some(expected_revision) {
            return Err(conflict());
        }
        let snapshot = snapshot(&transaction, unit_id)?;
        if snapshot.expected.is_empty() {
            return Err(AppError::new(
                "resource_missing",
                "请先补充词义，已有听力单元仍可练习。",
            ));
        }
        let id = Uuid::new_v4().to_string();
        transaction.execute("INSERT INTO review_questions(id,unit_id,state_revision,snapshot_json,created_at) VALUES (?,?,?,?,?)",
            params![id,unit_id,expected_revision,serde_json::to_string(&snapshot)?,Utc::now().timestamp_millis()])?;
        transaction.commit()?;
        Ok(Question {
            id,
            unit_id: unit_id.into(),
            dimension: snapshot.dimension.clone(),
            prompt: if snapshot.dimension == "meaning" {
                snapshot.target.clone()
            } else if snapshot.asset_id.is_some() {
                "听对白，补全例句中的空缺".into()
            } else {
                "听系统语音，写出听到的词、短语或整句".into()
            },
            context: if snapshot.dimension == "meaning" {
                snapshot.context
            } else if snapshot.asset_id.is_some() {
                mask_target(&snapshot.context, &snapshot.target)
            } else {
                String::new()
            },
            audio_kind: snapshot.audio_kind,
        })
    }

    pub fn review_hint(&self, question_id: &str) -> Result<String, AppError> {
        let connection = self.connection()?;
        let json: Option<String> = connection
            .query_row(
                "SELECT snapshot_json FROM review_questions WHERE id=? AND attempt_id IS NULL",
                [question_id],
                |r| r.get(0),
            )
            .optional()?;
        let snapshot: QuestionSnapshot =
            serde_json::from_str(&json.ok_or_else(|| AppError::new("not_found", "题目已结束。"))?)?;
        connection.execute(
            "UPDATE review_questions SET hinted=1 WHERE id=?",
            [question_id],
        )?;
        let text = if snapshot.dimension == "meaning" {
            &snapshot.expected[0]
        } else {
            &snapshot.target
        };
        Ok(format!(
            "首字提示：{}…（{} 个字符）",
            text.chars().next().unwrap_or('?'),
            text.chars().count()
        ))
    }

    pub fn review_audio(&self, question_id: &str) -> Result<QuestionSnapshot, AppError> {
        let connection = self.connection()?;
        let json: Option<String> = connection
            .query_row(
                "SELECT snapshot_json FROM review_questions WHERE id=?",
                [question_id],
                |r| r.get(0),
            )
            .optional()?;
        let snapshot: QuestionSnapshot =
            serde_json::from_str(&json.ok_or_else(|| AppError::new("not_found", "题目不存在。"))?)?;
        if snapshot.dimension != "listening" {
            return Err(invalid("此题不是听力题。"));
        }
        Ok(snapshot)
    }

    #[cfg(any(test, all(windows, feature = "desktop")))]
    pub(crate) fn review_audio_prepared(&self, question_id: &str) -> Result<(), AppError> {
        self.connection()?.execute(
            "UPDATE review_questions SET audio_prepared=1 WHERE id=?",
            [question_id],
        )?;
        Ok(())
    }

    pub fn review_submit(&self, input: &AnswerInput) -> Result<Attempt, AppError> {
        if Uuid::parse_str(&input.operation_id).is_err()
            || input.answer.chars().count() > 20000
            || (!input.unable && input.answer.trim().is_empty())
        {
            return Err(invalid("请输入回答，或选择暂时想不起来。"));
        }
        let hash = digest(&serde_json::to_vec(input)?);
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let receipt: Option<(String, String)> = transaction
            .query_row(
                "SELECT id,request_hash FROM review_attempts WHERE operation_id=?",
                [&input.operation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, saved)) = receipt {
            if saved != hash {
                return Err(conflict());
            }
            return attempt_on(&transaction, &id);
        }
        let (unit_id,revision,json,hinted,closed):(String,i64,String,bool,Option<String>)=transaction.query_row(
            "SELECT unit_id,state_revision,snapshot_json,hinted,attempt_id FROM review_questions WHERE id=?",[&input.question_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?.ok_or_else(||AppError::new("not_found","题目不存在。"))?;
        let current: i64 = transaction.query_row(
            "SELECT revision FROM review_states WHERE unit_id=?",
            [&unit_id],
            |r| r.get(0),
        )?;
        if closed.is_some() || current != revision {
            return Err(conflict());
        }
        let snapshot: QuestionSnapshot = serde_json::from_str(&json)?;
        if snapshot.dimension == "listening" {
            let prepared: bool = transaction.query_row(
                "SELECT audio_prepared FROM review_questions WHERE id=?",
                [&input.question_id],
                |r| r.get(0),
            )?;
            if !prepared {
                return Err(AppError::new(
                    "resource_missing",
                    "请先准备听力音频；资料不可用时跳过，不记录学习失败。",
                ));
            }
        }
        let outcome = hinted_outcome(
            if input.unable {
                Outcome::Incorrect
            } else {
                grade(&input.answer, &snapshot.expected)
            },
            hinted,
        );
        let now = event_time(&transaction)?;
        let id = Uuid::new_v4().to_string();
        transaction.execute("INSERT INTO review_attempts(id,operation_id,request_hash,unit_id,question_json,answer,hinted,outcome,grader,policy_version,state_before_json,created_at) VALUES (?,?,?,?,?,?,?,?,?,?,'{}',?)",
            params![id,input.operation_id,hash,unit_id,json,input.answer,hinted,outcome_text(outcome)?,if input.unable {"self_unable"} else {"exact_or_confirm"},POLICY_VERSION,now])?;
        transaction.execute(
            "UPDATE review_questions SET attempt_id=? WHERE id=?",
            params![id, input.question_id],
        )?;
        // Pending confirmation preserves the schedule, while closing this exact question.
        if outcome == Outcome::NeedsConfirmation {
            let state: String = transaction.query_row(
                "SELECT state_json FROM review_states WHERE unit_id=?",
                [&unit_id],
                |r| r.get(0),
            )?;
            transaction.execute(
                "UPDATE review_attempts SET state_before_json=?,state_after_json=? WHERE id=?",
                params![state, state, id],
            )?;
        } else {
            recompute(&transaction, &unit_id)?;
        }
        let result = attempt_on(&transaction, &id)?;
        crate::synchronization::mark_dirty(&transaction, &snapshot.entry_id)?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn review_correct(&self, input: &CorrectionInput) -> Result<Attempt, AppError> {
        if Uuid::parse_str(&input.operation_id).is_err()
            || input.reason.trim().is_empty()
            || input.reason.chars().count() > 4000
            || input.outcome == Outcome::NeedsConfirmation
        {
            return Err(invalid("请选择有效判分并说明修正理由。"));
        }
        let hash = digest(&serde_json::to_vec(input)?);
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let receipt: Option<(String, String)> = transaction
            .query_row(
                "SELECT attempt_id,request_hash FROM review_corrections WHERE operation_id=?",
                [&input.operation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, saved)) = receipt {
            if saved != hash {
                return Err(conflict());
            }
            return attempt_on(&transaction, &id);
        }
        let attempt = attempt_on(&transaction, &input.attempt_id)?;
        if attempt.revision != input.expected_revision {
            return Err(conflict());
        }
        transaction.execute("INSERT INTO review_corrections(id,operation_id,attempt_id,expected_revision,outcome,reason,created_at,request_hash) VALUES (?,?,?,?,?,?,?,?)",
            params![Uuid::new_v4().to_string(),input.operation_id,input.attempt_id,input.expected_revision,outcome_text(hinted_outcome(input.outcome,attempt.hinted))?,input.reason.trim(),Utc::now().timestamp_millis(),hash])?;
        transaction.execute(
            "UPDATE review_attempts SET revision=revision+1,hinted=MAX(hinted,?) WHERE id=?",
            params![input.outcome == Outcome::CorrectWithHint, input.attempt_id],
        )?;
        recompute(&transaction, &attempt.unit_id)?;
        let result = attempt_on(&transaction, &input.attempt_id)?;
        crate::synchronization::mark_dirty(&transaction, &attempt.question.entry_id)?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn review_history(
        &self,
        unit_id: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Attempt>, AppError> {
        let connection = self.connection()?;
        let mut query=connection.prepare("SELECT id FROM review_attempts WHERE (? IS NULL OR unit_id=?) ORDER BY created_at DESC,id DESC LIMIT ? OFFSET ?")?;
        let ids = query
            .query_map(
                params![unit_id, unit_id, limit.clamp(1, 100), offset],
                |r| r.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        ids.iter().map(|id| attempt_on(&connection, id)).collect()
    }
}
