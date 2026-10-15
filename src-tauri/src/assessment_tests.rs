use crate::{
    assessment::*,
    assessment_bank::*,
    store::Store,
    vocabulary::{CollectionInput, ExampleInput, digest},
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::collections::HashSet;
use uuid::Uuid;

fn op() -> String {
    Uuid::new_v4().to_string()
}
fn fixture() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("dictionaries")).unwrap();
    let c = Connection::open(dir.path().join("dictionaries/base.sqlite3")).unwrap();
    c.execute_batch("CREATE TABLE dictionary_entries(word TEXT,meaning TEXT);CREATE TABLE dictionary_aliases(form TEXT,lemma TEXT);CREATE TABLE metadata(key TEXT,value TEXT);INSERT INTO metadata VALUES ('source','TC-54');
        CREATE TABLE frequency_catalogs(id TEXT,kind TEXT,title TEXT,source_url TEXT,source_version TEXT,source_sha256 TEXT,rank_unit TEXT,raw_rows INTEGER,max_rank INTEGER,notice TEXT);
        INSERT INTO frequency_catalogs VALUES ('words','word','合成固定频段','https://example.invalid','v1','fixture','original_rank',6,2000,'TC-54 synthetic');
        CREATE TABLE frequency_targets(catalog_id TEXT,rank INTEGER,text TEXT,target_key TEXT,lexeme_key TEXT,mapping_basis TEXT,frequency INTEGER,eligible INTEGER,seed_json TEXT);").unwrap();
    for (rank, text) in [
        (5, "bright"),
        (70, "reluctant"),
        (900, "river"),
        (1005, "garden"),
        (1500, "gentle"),
        (1900, "apprentice"),
    ] {
        c.execute(
            "INSERT INTO frequency_targets VALUES ('words',?,?,?,?,'identity',NULL,1,NULL)",
            params![rank, text, text, text],
        )
        .unwrap();
    }
    drop(c);
    let store = Store::open(dir.path()).unwrap();
    store
        .collect(&CollectionInput {
            operation_id: op(),
            kind: "word".into(),
            text: "bright".into(),
            meaning: "原人工释义".into(),
            examples: vec![ExampleInput {
                text: "The light is bright.".into(),
                ..Default::default()
            }],
            target_entry_id: None,
            expected_revision: None,
        })
        .unwrap();
    let bank = op();
    let sources = store.assessment_catalogs().unwrap();
    store.connection().unwrap().execute("INSERT INTO assessment_banks(id,state,sources_json,policy_version,created_at) VALUES (?,'draft',?,?,1)",params![bank,serde_json::to_string(&sources).unwrap(),POLICY]).unwrap();
    let mut wave = b"RIFF".to_vec();
    wave.extend_from_slice(&836u32.to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&[1, 0, 1, 0]);
    wave.extend_from_slice(&8000u32.to_le_bytes());
    wave.extend_from_slice(&16000u32.to_le_bytes());
    wave.extend_from_slice(&[2, 0, 16, 0]);
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&800u32.to_le_bytes());
    wave.resize(844, 0);
    std::fs::write(store.root().join("media/fixture.wav"), &wave).unwrap();
    for target in store.assessment_targets("words", 2000).unwrap() {
        store.set_known_target("word", &target.text, true).unwrap();
        let sentence = format!("This example uses {}.", target.text);
        let items = TYPES
            .iter()
            .map(|kind| {
                (
                    (*kind).to_owned(),
                    QuestionDraft {
                        target: target.text.clone(),
                        meaning: "本题固定中文义项".into(),
                        sentence: sentence.clone(),
                        translation: "这句话使用本题词语。".into(),
                        prompt: if *kind == "cloze" {
                            "This example uses ___.".into()
                        } else if *kind == "listening" {
                            "听音选择含义。".into()
                        } else {
                            format!("{} 在句中是什么意思？", target.text)
                        },
                        choices: if *kind == "cloze" {
                            vec![]
                        } else {
                            vec![
                                Choice {
                                    id: "original-right".into(),
                                    text: "本题固定中文义项".into(),
                                },
                                Choice {
                                    id: "wrong-a".into(),
                                    text: "另一类中文含义".into(),
                                },
                                Choice {
                                    id: "wrong-b".into(),
                                    text: "不同的中文义项".into(),
                                },
                            ]
                        },
                        correct_option_id: (*kind != "cloze").then(|| "original-right".into()),
                        accepted_answers: if *kind == "cloze" {
                            vec![target.text.clone()]
                        } else {
                            vec![]
                        },
                        explanation: "这是一道事先核对的合成核心样本。".into(),
                        origin: "TC-54 synthetic".into(),
                        audio: (*kind == "listening").then(|| AssessmentAudio {
                            kind: "system".into(),
                            relative_path: "media/fixture.wav".into(),
                            sha256: digest(&wave),
                            text: format!("{}. {}", target.text, sentence),
                            voice_id: "synthetic-test-only".into(),
                            asset_id: None,
                        }),
                    },
                )
            })
            .collect::<Vec<_>>();
        store
            .assessment_save_candidates(&bank, &target, &items)
            .unwrap();
    }
    // The normal fixture is pre-reviewed; interactive human review was covered once by TC-53.
    store.connection().unwrap().execute("UPDATE assessment_items SET state='approved',review_note='TC-54 fixed fixture' WHERE bank_id=?",[&bank]).unwrap();
    let summary = store.assessment_bank_summary(&bank).unwrap();
    store
        .assessment_bank_review(&ReviewBank {
            operation_id: op(),
            bank_id: bank,
            expected_revision: summary.revision,
            action: "publish".into(),
            item_id: None,
            expected_item_revision: None,
            content: None,
            note: "TC-54 合成题库，沿用已核对固定题面。".into(),
        })
        .unwrap();
    (dir, store)
}
fn personal_snapshot(store: &Store) -> Value {
    let c = store.connection().unwrap();
    let mut q = c
        .prepare("SELECT state_json FROM review_states ORDER BY unit_id")
        .unwrap();
    let reviews = q
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    json!({"entries":c.query_row("SELECT COUNT(*) FROM entries",[],|r|r.get::<_,usize>(0)).unwrap(),"meaning":c.query_row("SELECT text FROM meanings LIMIT 1",[],|r|r.get::<_,String>(0)).unwrap(),"reviews":reviews,"attempts":c.query_row("SELECT COUNT(*) FROM review_attempts",[],|r|r.get::<_,usize>(0)).unwrap(),"known":c.query_row("SELECT COUNT(*) FROM settings WHERE key LIKE 'known_target:%'",[],|r|r.get::<_,usize>(0)).unwrap()})
}
#[test]
fn tc54_stratified_offline_session_and_resume() {
    let (dir, store) = fixture();
    let personal = personal_snapshot(&store);
    let config = AssessmentConfig {
        word_top_n: 2000,
        question_count: 6,
        content: "word".into(),
        ..Default::default()
    };
    let preview = store.assessment_preview(&config).unwrap();
    assert!(preview.can_start);
    assert_eq!(preview.quotas.len(), 6);
    assert!(
        preview
            .quotas
            .iter()
            .all(|q| q.planned == 1 && q.available == 3 && q.unfilled == 0)
    );
    assert_eq!(preview.coverage[0].prepared_targets, 6);
    let request = StartAssessment {
        config,
        expected_inventory_revision: preview.inventory_revision,
        operation_id: op(),
    };
    let mut session = store.assessment_start(&request).unwrap();
    assert_eq!(store.assessment_start(&request).unwrap().id, session.id);
    let initial = {
        let c = store.connection().unwrap();
        let mut q=c.prepare("SELECT id,snapshot_json FROM assessment_session_items WHERE session_id=? ORDER BY position").unwrap();
        q.query_map([&session.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };
    let frozen = initial
        .iter()
        .map(|(_, s)| serde_json::from_str::<FrozenItem>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        frozen
            .iter()
            .map(|i| &i.lexeme_key)
            .collect::<HashSet<_>>()
            .len(),
        6
    );
    assert_eq!(frozen.iter().filter(|i| i.from_rank == 1).count(), 3);
    assert_eq!(frozen.iter().filter(|i| i.from_rank == 1001).count(), 3);
    // Save one ordinary draft, pause, reopen and resume exactly the same frozen sequence.
    let first = session.question.as_ref().unwrap();
    let first_id = first.id.clone();
    session = store
        .assessment_transition(&TransitionAssessment {
            session_id: session.id.clone(),
            expected_revision: session.revision,
            operation_id: op(),
            action: "save_draft".into(),
            item_id: Some(first_id.clone()),
            draft_text: "my draft".into(),
            draft_option: None,
        })
        .unwrap();
    session = store
        .assessment_transition(&TransitionAssessment {
            session_id: session.id.clone(),
            expected_revision: session.revision,
            operation_id: op(),
            action: "pause".into(),
            item_id: None,
            draft_text: String::new(),
            draft_option: None,
        })
        .unwrap();
    let id = session.id.clone();
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    let paused = store.assessment_session(&id).unwrap();
    assert_eq!(paused.state, "paused");
    session = store
        .assessment_transition(&TransitionAssessment {
            session_id: id.clone(),
            expected_revision: paused.revision,
            operation_id: op(),
            action: "resume".into(),
            item_id: None,
            draft_text: String::new(),
            draft_option: None,
        })
        .unwrap();
    assert_eq!(session.question.as_ref().unwrap().id, first_id);
    assert_eq!(session.question.as_ref().unwrap().draft_text, "my draft");
    let mut count = 0;
    let mut corrected = None;
    while session.state == "active" {
        let question = session.question.clone().unwrap();
        let (_, item, _) = load_item(&store.connection().unwrap(), &question.id, &id).unwrap();
        let public = serde_json::to_value(&question).unwrap();
        assert!(public.get("acceptedAnswers").is_none());
        assert!(public.get("correctOptionId").is_none());
        assert!(!question.choices.iter().any(|c| c.id == "original-right"));
        if question.question_type == "listening" {
            assert!(
                !serde_json::to_string(&question)
                    .unwrap()
                    .contains(&item.content.target)
            );
            let playback = store
                .assessment_audio_start(&AssessmentAudioStart {
                    session_id: id.clone(),
                    item_id: question.id.clone(),
                    expected_revision: session.revision,
                    operation_id: op(),
                })
                .unwrap();
            assert!(playback.path.is_some());
            for (event, seconds) in [("started", 0.0), ("ended", 0.05)] {
                store
                    .assessment_audio_event(&AssessmentAudioEvent {
                        session_id: id.clone(),
                        item_id: question.id.clone(),
                        playback_id: playback.playback_id.clone(),
                        operation_id: op(),
                        event: event.into(),
                        seconds,
                        error: String::new(),
                    })
                    .unwrap();
            }
        }
        if question.question_type == "cloze" && corrected.is_none() {
            session = store
                .assessment_transition(&TransitionAssessment {
                    session_id: id.clone(),
                    expected_revision: session.revision,
                    operation_id: op(),
                    action: "request_hint".into(),
                    item_id: Some(question.id.clone()),
                    draft_text: String::new(),
                    draft_option: None,
                })
                .unwrap();
        }
        let input = AssessmentAnswerInput {
            session_id: id.clone(),
            item_id: question.id.clone(),
            expected_revision: session.revision,
            operation_id: op(),
            action: if count == 5 { "unknown" } else { "answer" }.into(),
            option_id: item.content.correct_option_id.clone(),
            text: if question.question_type == "cloze" && corrected.is_none() {
                "a reasonable alternative for manual review".into()
            } else {
                item.content.target.clone()
            },
            uncertain: count == 1,
        };
        session = store.assessment_answer_submit(&input).unwrap();
        let saved = store.assessment_answer_submit(&input).unwrap();
        assert_eq!(saved.revision, session.revision);
        let feedback = session.last_feedback.as_ref().unwrap();
        if feedback.result == "needs_confirmation" {
            assert!(feedback.hinted);
            let before = store
                .connection()
                .unwrap()
                .query_row(
                    "SELECT answer_json FROM assessment_answers WHERE id=?",
                    [&feedback.answer_id],
                    |r| r.get::<_, String>(0),
                )
                .unwrap();
            store
                .assessment_correct(&CorrectAssessment {
                    answer_id: feedback.answer_id.clone(),
                    expected_revision: feedback.answer_revision,
                    operation_id: op(),
                    result: "correct".into(),
                    add_hint: false,
                    reason: "此合成测试答案经人工核对可接受。".into(),
                })
                .unwrap();
            let (effective, _, corrections) =
                effective_answer(&store.connection().unwrap(), &feedback.answer_id).unwrap();
            assert_eq!(effective.result, "correct");
            assert!(effective.hinted);
            assert_eq!(corrections.len(), 1);
            assert_eq!(
                before,
                store
                    .connection()
                    .unwrap()
                    .query_row(
                        "SELECT answer_json FROM assessment_answers WHERE id=?",
                        [&feedback.answer_id],
                        |r| r.get::<_, String>(0)
                    )
                    .unwrap()
            );
            corrected = Some(feedback.answer_id.clone());
            session = store.assessment_session(&id).unwrap();
        }
        count += 1;
    }
    assert_eq!(count, 6);
    assert!(corrected.is_some());
    assert_eq!(session.state, "completed");
    assert_eq!(session.processed, 6);
    let after = {
        let c = store.connection().unwrap();
        let mut q=c.prepare("SELECT id,snapshot_json FROM assessment_session_items WHERE session_id=? ORDER BY position").unwrap();
        q.query_map([&id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };
    assert_eq!(initial, after);
    assert_eq!(personal_snapshot(&store), personal);
    assert_eq!(
        store.assessment_session_list(false, 0, 20).unwrap()["total"],
        json!(1)
    );
    println!(
        "TC-54 OK: six unique known targets, two frequency bands x three types, frozen private snapshots/choices, saved draft and reopen/resume, audio acknowledgements, one answer receipt per position, append-only correction and hint retention, regular vocabulary/review unchanged. No model calls."
    );
}
