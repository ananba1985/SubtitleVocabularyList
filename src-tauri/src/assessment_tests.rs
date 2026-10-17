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

#[test]
fn ipc_dto_normal_responses() {
    use crate::ipc_dto::{self, AssessmentAudioReply, AssessmentHistoryItem,
        AssessmentReportReply, AssessmentSessionSummary, Page};
    use crate::assessment_reports::ReportItem;
    let (_dir, store) = fixture();
    let entry = store.list_entries("", 0, 1).unwrap().remove(0);
    let bundle: ipc_dto::SyncBundleView = ipc_dto::project(store.sync_export(&entry.id).unwrap()).unwrap();
    assert_eq!(bundle.entry.id, entry.id);
    let config = AssessmentConfig {
        word_top_n: 2000, question_count: 1, minimum_sample: 1,
        question_types: vec!["recognition".into()], ..Default::default()
    };
    let preview = store.assessment_preview(&config).unwrap();
    let session = store.assessment_start(&StartAssessment {
        config, expected_inventory_revision: preview.inventory_revision, operation_id: op(),
    }).unwrap();
    let page: Page<AssessmentSessionSummary> = ipc_dto::project(store.assessment_session_list(true, 0, 10).unwrap()).unwrap();
    assert_eq!(page.items[0].config.question_count, 1);
    let pending: AssessmentReportReply = ipc_dto::project(store.assessment_report(&session.id, None).unwrap()).unwrap();
    assert!(matches!(pending, AssessmentReportReply::Pending(_)));
    let question = session.question.unwrap();
    let completed = store.assessment_answer_submit(&AssessmentAnswerInput {
        session_id: session.id.clone(), item_id: question.id, expected_revision: session.revision,
        operation_id: op(), action: "unknown".into(), option_id: None,
        text: String::new(), uncertain: false,
    }).unwrap();
    let reply: AssessmentReportReply = ipc_dto::project(store.assessment_report(&session.id, None).unwrap()).unwrap();
    let AssessmentReportReply::Finished { report, .. } = reply else { panic!("Expected finished report") };
    assert_eq!(report.counts.processed, 1);
    assert_eq!(report.coverage[0].top_n, 2000);
    let items: Page<ReportItem> = ipc_dto::project(store.assessment_report_items(&session.id, completed.revision, 0, 10).unwrap()).unwrap();
    assert_eq!(items.items[0].answer.as_ref().unwrap().result, "unknown");
    let history: Page<AssessmentHistoryItem> = ipc_dto::project(store.assessment_history(0, 10, None).unwrap()).unwrap();
    assert_eq!(history.items[0].id, session.id);
    let audio: AssessmentAudioReply = ipc_dto::project(json!({
        "playbackId":"synthetic-playback", "played":true, "seconds":1.0, "error":null,
    })).unwrap();
    assert!(audio.error.is_none());
}

#[test]
fn sync_growth_assessment_caches_changed_sessions_and_keeps_reports() {
    let (_dir, store) = fixture();
    let scope = "growth-account";
    let config = AssessmentConfig {
        word_top_n: 2000,
        question_count: 1,
        question_types: vec!["recognition".into()],
        ..Default::default()
    };
    let preview = store.assessment_preview(&config).unwrap();
    let session = store
        .assessment_start(&StartAssessment {
            config,
            expected_inventory_revision: preview.inventory_revision,
            operation_id: op(),
        })
        .unwrap();
    assert_eq!(store.assessment_sync_status(scope).unwrap().pending, 1);
    let initial = store.assessment_freeze_pushes(scope).unwrap().remove(0);
    store.assessment_sync_saved(scope, &initial, 1).unwrap();
    assert!(
        store
            .assessment_revision_known(scope, &session.id, 1)
            .unwrap()
    );
    assert_eq!(store.assessment_sync_status(scope).unwrap().pending, 0);
    store
        .assessment_answer_submit(&AssessmentAnswerInput {
            session_id: session.id.clone(),
            item_id: session.question.unwrap().id,
            expected_revision: session.revision,
            operation_id: op(),
            action: "unknown".into(),
            option_id: None,
            text: String::new(),
            uncertain: false,
        })
        .unwrap();
    let complete = store.assessment_freeze_pushes(scope).unwrap().remove(0);
    assert_eq!(
        serde_json::to_value(&complete.bundle.items).unwrap(),
        serde_json::to_value(&initial.bundle.items).unwrap()
    );
    assert_eq!(complete.bundle.answers.len(), 1);
    assert_eq!(complete.bundle.reports.len(), 1);
    store.assessment_sync_saved(scope, &complete, 2).unwrap();
    assert_eq!(store.assessment_sync_status(scope).unwrap().pending, 0);
    let writes: i64 = store
        .connection()
        .unwrap()
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(store.assessment_sync_status(scope).unwrap().pending, 0);
    assert!(store.assessment_freeze_pushes(scope).unwrap().is_empty());
    assert_eq!(
        store
            .connection()
            .unwrap()
            .query_row("SELECT total_changes()", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        writes
    );
    let answer = &complete.bundle.answers[0];
    store
        .assessment_correct(&CorrectAssessment {
            answer_id: answer.answer.id.clone(),
            expected_revision: answer.revision,
            operation_id: op(),
            result: "incorrect".into(),
            add_hint: false,
            reason: "合成核对记录".into(),
        })
        .unwrap();
    let corrected = store.assessment_freeze_pushes(scope).unwrap().remove(0);
    assert_eq!(corrected.bundle.answers.len(), 1);
    assert_eq!(corrected.bundle.corrections.len(), 1);
    assert_eq!(corrected.bundle.reports.len(), 2);
    assert!(
        corrected
            .bundle
            .reports
            .iter()
            .any(|r| r.digest == complete.bundle.reports[0].digest
                && r.report_json == complete.bundle.reports[0].report_json)
    );
    store.assessment_sync_saved(scope, &corrected, 3).unwrap();
    assert_eq!(store.assessment_sync_status(scope).unwrap().pending, 0);
}

#[test]
fn reports_reuse_business_content_across_sync_and_keep_history() {
    crate::local_profile::capture();
    use crate::assessment_sync::AssessmentDocument;
    let (_dir, source) = fixture_audio("original");
    let scope = "report-content-normal";
    source
        .assessment_sync_capability(scope, Some("assessment-1"))
        .unwrap();
    let config = AssessmentConfig {
        word_top_n: 2000,
        question_count: 1,
        minimum_sample: 1,
        question_types: vec!["listening".into()],
        ..Default::default()
    };
    let preview = source.assessment_preview(&config).unwrap();
    let session = source
        .assessment_start(&StartAssessment {
            config,
            expected_inventory_revision: preview.inventory_revision,
            operation_id: op(),
        })
        .unwrap();
    let item = session.question.unwrap().id;
    let audio = source
        .assessment_audio_start(&AssessmentAudioStart {
            session_id: session.id.clone(),
            item_id: item.clone(),
            expected_revision: session.revision,
            operation_id: op(),
        })
        .unwrap();
    for event in ["started", "ended"] {
        source
            .assessment_audio_event(&AssessmentAudioEvent {
                session_id: session.id.clone(),
                item_id: item.clone(),
                playback_id: audio.playback_id.clone(),
                operation_id: op(),
                event: event.into(),
                seconds: 0.05,
                error: String::new(),
            })
            .unwrap();
    }
    let completed = source
        .assessment_answer_submit(&AssessmentAnswerInput {
            session_id: session.id.clone(),
            item_id: item,
            expected_revision: session.revision,
            operation_id: op(),
            action: "unknown".into(),
            option_id: None,
            text: String::new(),
            uncertain: false,
        })
        .unwrap();
    source.assessment_report(&session.id, None).unwrap();
    // Existing redundant versions are immutable history, including after upgrade.
    {
        let c = source.connection().unwrap();
        let raw: String = c
            .query_row(
                "SELECT report_json FROM assessment_report_versions WHERE session_id=?",
                [&session.id],
                |r| r.get(0),
            )
            .unwrap();
        let mut old: Value = serde_json::from_str(&raw).unwrap();
        old["revision"] = json!(completed.revision + 1);
        old["generatedAt"] = json!(old["generatedAt"].as_i64().unwrap() + 1);
        c.execute(
            "INSERT INTO assessment_report_versions VALUES (?,?,?,?)",
            params![
                session.id,
                completed.revision + 1,
                old.to_string(),
                old["generatedAt"].as_i64().unwrap()
            ],
        )
        .unwrap();
        c.execute(
            "UPDATE assessment_sessions SET revision=revision+1 WHERE id=?",
            [&session.id],
        )
        .unwrap();
        c.execute(
            "DELETE FROM settings WHERE key=?",
            [format!("assessment_current_report:{}", session.id)],
        )
        .unwrap();
    }
    assert_eq!(source.assessment_sync_status(scope).unwrap().pending, 1);
    let first = source
        .assessment_freeze_push(scope, &session.id)
        .unwrap()
        .unwrap();
    assert_eq!(first.bundle.reports.len(), 2);
    source.assessment_sync_saved(scope, &first, 1).unwrap();
    let peer_dir = tempfile::tempdir().unwrap();
    let peer = Store::open(peer_dir.path()).unwrap();
    peer.assessment_sync_capability(scope, Some("assessment-1"))
        .unwrap();
    let media = &first.bundle.media[0];
    let bytes = source
        .assessment_sync_media(&media.digest, &media.format)
        .unwrap();
    peer.assessment_save_remote_media(media, &bytes).unwrap();
    assert!(peer
        .assessment_sync_receive(
            scope,
            &AssessmentDocument {
                session_id: session.id.clone(),
                revision: 1,
                bundle: first.bundle.clone(),
            }
        )
        .unwrap());
    source
        .connection()
        .unwrap()
        .execute(
            "UPDATE assessment_sessions SET updated_at=updated_at+1 WHERE id=?",
            [&session.id],
        )
        .unwrap();
    assert_eq!(source.assessment_sync_status(scope).unwrap().pending, 1);
    let metadata = source
        .assessment_freeze_push(scope, &session.id)
        .unwrap()
        .unwrap();
    source.assessment_sync_saved(scope, &metadata, 2).unwrap();
    assert!(peer
        .assessment_sync_receive(
            scope,
            &AssessmentDocument {
                session_id: session.id.clone(),
                revision: 2,
                bundle: metadata.bundle.clone(),
            }
        )
        .unwrap());
    let reused = peer.assessment_report(&session.id, None).unwrap();
    assert_eq!(reused["revisions"].as_array().unwrap().len(), 2);
    assert_eq!(reused["report"]["counts"]["unknown"], 1);
    let slot = reused["report"]["revision"].as_i64().unwrap();
    let history = peer
        .assessment_history(0, 10, reused["report"]["comparisonKey"].as_str())
        .unwrap();
    assert_eq!(history["total"], 1);
    assert_eq!(history["items"][0]["revision"], slot);
    assert_eq!(
        peer.assessment_report_items(&session.id, slot, 0, 10)
            .unwrap()["items"][0]["answer"]["result"],
        "unknown"
    );
    assert!(peer
        .assessment_freeze_push(scope, &session.id)
        .unwrap()
        .is_none());
    let original = &first.bundle.answers[0];
    peer.assessment_correct(&CorrectAssessment {
        answer_id: original.answer.id.clone(),
        expected_revision: original.revision,
        operation_id: op(),
        result: "correct".into(),
        add_hint: false,
        reason: "正常人工核对".into(),
    })
    .unwrap();
    assert_eq!(peer.assessment_sync_status(scope).unwrap().pending, 1);
    let corrected = peer
        .assessment_freeze_push(scope, &session.id)
        .unwrap()
        .unwrap();
    assert_eq!(corrected.bundle.reports.len(), 3);
    assert_eq!(corrected.bundle.corrections.len(), 1);
    for report in &first.bundle.reports {
        assert!(corrected
            .bundle
            .reports
            .iter()
            .any(|r| r.digest == report.digest && r.report_json == report.report_json));
    }
    peer.assessment_sync_saved(scope, &corrected, 3).unwrap();
    source.assessment_save_remote_media(media, &bytes).unwrap();
    assert!(source
        .assessment_sync_receive(
            scope,
            &AssessmentDocument {
                session_id: session.id.clone(),
                revision: 3,
                bundle: corrected.bundle.clone(),
            }
        )
        .unwrap());
    let current = source.assessment_report(&session.id, None).unwrap();
    assert_eq!(current["revisions"].as_array().unwrap().len(), 3);
    assert_eq!(current["report"]["counts"]["correct"], 1);
    let detail = source
        .assessment_report_items(
            &session.id,
            current["report"]["revision"].as_i64().unwrap(),
            0,
            10,
        )
        .unwrap();
    assert_eq!(detail["items"][0]["originalAnswer"]["result"], "unknown");
    assert_eq!(
        detail["items"][0]["corrections"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        source
            .assessment_sync_media(&media.digest, &media.format)
            .unwrap(),
        bytes
    );
    assert_eq!(
        peer.assessment_sync_media(&media.digest, &media.format)
            .unwrap(),
        bytes
    );
    let samples = crate::local_profile::take_samples();
    let freezes = samples
        .iter()
        .filter(|s| s.operation == "assessment.freeze")
        .collect::<Vec<_>>();
    assert_eq!(freezes.len(), 4);
    assert!(freezes
        .iter()
        .all(|s| !s.timings.contains_key("assessment.export")
            && !s.timings.contains_key("assessment.digest")));
    assert_eq!(
        samples
            .iter()
            .filter(|s| s.operation == "assessment.status")
            .count(),
        3
    );
    println!("local_profile={}", serde_json::to_string(&samples).unwrap());
    if let Some(path) = std::env::var_os("SVL_PROFILE_RESULT") {
        std::fs::write(path, serde_json::to_vec_pretty(&samples).unwrap()).unwrap();
    }
}

fn fixture() -> (tempfile::TempDir, Store) {
    fixture_audio("system")
}

fn fixture_audio(audio_kind: &str) -> (tempfile::TempDir, Store) {
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
                            kind: audio_kind.into(),
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

#[test]
fn sync_delta_append_and_correct_complete_histories() {
    use crate::{sync_delta, reviews::{AnswerInput, CorrectionInput}, review_policy::Outcome};
    let (_dir, store) = fixture();
    let scope = "delta-account";
    let mut cases = Vec::new();
    let entry = crate::sync_test_fixture::large_entry(&store, None, 2);
    let asset = op();
    let bytes = std::fs::read(store.root().join("media/fixture.wav")).unwrap();
    let hash = digest(&bytes);
    {
        let c = store.connection().unwrap();
        c.execute("INSERT INTO media_assets(id,recipe_key,digest,relative_path,kind,format,duration_ms,state,created_at) VALUES (?,?,?,'media/fixture.wav','original','wav',50,'ready',1)",params![asset,hash,hash]).unwrap();
        let example: String = c.query_row("SELECT example_id FROM entry_examples WHERE entry_id=? LIMIT 1",[&entry],|r|r.get(0)).unwrap();
        c.execute("INSERT INTO example_media(example_id,asset_id) VALUES (?,?)",params![example,asset]).unwrap();
    }
    let initial = json!({"changeId":op(),"baseRevision":0,"bundle":store.sync_export(&entry).unwrap()});
    let mut steps = vec![json!({"full":initial,"wire":initial})];
    let mut base = initial["bundle"].clone();
    store.connection().unwrap().execute("INSERT INTO sync_documents(account_scope,remote_id,local_id,remote_revision,base_json,acknowledged_epoch) VALUES (?,?,?,1,?,0)",params![scope,entry,entry,base.to_string()]).unwrap();
    let unit = store.review_units("all","meaning",0,100).unwrap().into_iter().find(|u|u.entry_id==entry).unwrap();
    let q = store.review_question(&unit.id,unit.revision).unwrap();
    let answer = store.review_submit(&AnswerInput { operation_id:op(),question_id:q.id,answer:"大词条同步验证".into(),unable:false }).unwrap();
    for revision in [1,2] {
        if revision==2 {
            store.review_correct(&CorrectionInput { operation_id:op(),attempt_id:answer.id.clone(),expected_revision:answer.revision,outcome:Outcome::Incorrect,reason:"合成核对".into() }).unwrap();
        }
        let full = json!({"changeId":op(),"baseRevision":revision,"bundle":store.sync_export(&entry).unwrap()});
        let saved = store.sync_upload_base(scope,&entry,revision,false).unwrap().unwrap();
        assert_eq!(saved,base);
        let wire = sync_delta::request(&full,Some(&saved),"entryId");
        assert_eq!(wire["entryId"],entry);
        assert_eq!(wire["delta"]["protocol"],sync_delta::PROTOCOL);
        assert!(!wire.to_string().contains("/tables/examples"));
        assert!(wire.to_string().len()<full.to_string().len());
        base = full["bundle"].clone();
        steps.push(json!({"full":full,"wire":wire}));
        store.connection().unwrap().execute("UPDATE sync_documents SET remote_revision=?,base_json=? WHERE account_scope=? AND remote_id=?",params![revision+1,base.to_string(),scope,entry]).unwrap();
    }
    assert_eq!(base["tables"]["review_attempts"].as_array().unwrap().len(),3);
    assert_eq!(base["tables"]["review_corrections"].as_array().unwrap().len(),1);
    cases.push(json!({"kind":"entry","id":entry,"steps":steps}));

    let config = AssessmentConfig {word_top_n:2000,question_count:1,question_types:vec!["recognition".into()],..Default::default()};
    let preview = store.assessment_preview(&config).unwrap();
    let session = store.assessment_start(&StartAssessment {config,expected_inventory_revision:preview.inventory_revision,operation_id:op()}).unwrap();
    let push = store.assessment_freeze_push(scope,&session.id).unwrap().unwrap();
    let full = serde_json::to_value(&push).unwrap();
    let mut steps = vec![json!({"full":full,"wire":full})];
    store.assessment_sync_saved(scope,&push,1).unwrap();
    store.assessment_answer_submit(&AssessmentAnswerInput {session_id:session.id.clone(),item_id:session.question.unwrap().id,expected_revision:session.revision,operation_id:op(),action:"unknown".into(),option_id:None,text:String::new(),uncertain:false}).unwrap();
    for revision in [1,2] {
        let push = store.assessment_freeze_push(scope,&session.id).unwrap().unwrap();
        let full = serde_json::to_value(&push).unwrap();
        let saved = store.sync_upload_base(scope,&session.id,revision,true).unwrap().unwrap();
        let wire = sync_delta::request(&full,Some(&saved),"sessionId");
        assert_eq!(wire["delta"]["protocol"],sync_delta::PROTOCOL);
        assert!(!wire.to_string().contains("\"/items"));
        steps.push(json!({"full":full,"wire":wire}));
        store.assessment_sync_saved(scope,&push,revision+1).unwrap();
        if revision==1 {
            let answer = &push.bundle.answers[0];
            store.assessment_correct(&CorrectAssessment {answer_id:answer.answer.id.clone(),expected_revision:answer.revision,operation_id:op(),result:"incorrect".into(),add_hint:false,reason:"合成核对".into()}).unwrap();
        } else {
            assert_eq!(push.bundle.reports.len(),2);
            assert_eq!(push.bundle.corrections.len(),1);
        }
    }
    cases.push(json!({"kind":"assessment","id":session.id,"steps":steps}));
    if let Ok(path) = std::env::var("SVL_DELTA_FIXTURE_OUTPUT") {
        std::fs::write(path,serde_json::to_vec(&json!({"cases":cases,"audio":{"digest":hash,"format":"wav","bytes":bytes}})).unwrap()).unwrap();
    }
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
#[test]
fn tc56_tc57_reports_and_confirmed_collection() {
    use crate::assessment_reports::*;
    let (_dir, store) = fixture();
    let config = AssessmentConfig {
        word_top_n: 2000,
        question_count: 6,
        ..Default::default()
    };
    let preview = store.assessment_preview(&config).unwrap();
    let mut session = store
        .assessment_start(&StartAssessment {
            config: config.clone(),
            expected_inventory_revision: preview.inventory_revision,
            operation_id: op(),
        })
        .unwrap();
    let mut seen = std::collections::HashMap::<String, usize>::new();
    while let Some(question) = session.question.clone() {
        let (_, item, _) =
            load_item(&store.connection().unwrap(), &question.id, &session.id).unwrap();
        let count = seen.entry(question.question_type.clone()).or_default();
        *count += 1;
        if question.question_type == "listening" {
            let playback = store
                .assessment_audio_start(&AssessmentAudioStart {
                    session_id: session.id.clone(),
                    item_id: question.id.clone(),
                    expected_revision: session.revision,
                    operation_id: op(),
                })
                .unwrap();
            for event in ["started", "ended"] {
                store
                    .assessment_audio_event(&AssessmentAudioEvent {
                        session_id: session.id.clone(),
                        item_id: question.id.clone(),
                        playback_id: playback.playback_id.clone(),
                        operation_id: op(),
                        event: event.into(),
                        seconds: if event == "ended" { 0.05 } else { 0.0 },
                        error: String::new(),
                    })
                    .unwrap();
            }
        }
        if question.question_type == "recognition" && *count == 1 {
            session = store
                .assessment_transition(&TransitionAssessment {
                    session_id: session.id.clone(),
                    expected_revision: session.revision,
                    operation_id: op(),
                    action: "request_hint".into(),
                    item_id: Some(question.id.clone()),
                    draft_text: String::new(),
                    draft_option: None,
                })
                .unwrap();
        }
        let unknown = question.question_type == "recognition" && *count == 2;
        let option = if question.question_type == "listening" && *count == 2 {
            item.content
                .choices
                .iter()
                .find(|c| Some(&c.id) != item.content.correct_option_id.as_ref())
                .map(|c| c.id.clone())
        } else {
            item.content.correct_option_id.clone()
        };
        session = store
            .assessment_answer_submit(&AssessmentAnswerInput {
                session_id: session.id.clone(),
                item_id: question.id,
                expected_revision: session.revision,
                operation_id: op(),
                action: if unknown { "unknown" } else { "answer" }.into(),
                option_id: option,
                text: if question.question_type == "cloze" && *count == 1 {
                    "reasonable variant for manual review".into()
                } else {
                    item.content.target
                },
                uncertain: question.question_type == "cloze" && *count == 2,
            })
            .unwrap();
    }
    let original = store.assessment_report(&session.id, None).unwrap();
    let counts = &original["report"]["counts"];
    assert_eq!(
        (
            counts["planned"].as_u64(),
            counts["processed"].as_u64(),
            counts["unscored"].as_u64(),
            counts["valid"].as_u64()
        ),
        (Some(6), Some(6), Some(1), Some(5))
    );
    assert_eq!(counts["correct"], 3);
    assert_eq!(counts["independentCorrect"], 1);
    assert_eq!(counts["unknown"], 1);
    assert_eq!(counts["hinted"], 1);
    assert_eq!(counts["uncertain"], 1);
    assert_eq!(original["report"]["status"], "provisional");
    assert!(
        original["report"]["byBand"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["insufficientSample"] == true)
    );
    let initial_revision = original["report"]["revision"].as_i64().unwrap();
    let page = store
        .assessment_report_items(&session.id, initial_revision, 0, 20)
        .unwrap();
    let pending = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["answer"]["result"] == "needs_confirmation")
        .unwrap();
    let answer_id = pending["answer"]["id"].as_str().unwrap();
    store
        .assessment_correct(&CorrectAssessment {
            answer_id: answer_id.into(),
            expected_revision: 1,
            operation_id: op(),
            result: "correct".into(),
            add_hint: false,
            reason: "已核对本题可接受的语境变体。".into(),
        })
        .unwrap();
    let revised = store.assessment_report(&session.id, None).unwrap();
    assert_eq!(revised["report"]["counts"]["correct"], 4);
    assert_eq!(revised["report"]["counts"]["valid"], 6);
    assert_eq!(revised["report"]["counts"]["independentCorrect"], 2);
    assert_eq!(revised["report"]["status"], "settled");
    assert_eq!(
        store
            .assessment_report(&session.id, Some(initial_revision))
            .unwrap()["report"],
        original["report"]
    );
    let raw: String = store
        .connection()
        .unwrap()
        .query_row(
            "SELECT answer_json FROM assessment_answers WHERE id=?",
            [answer_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(raw.contains("needs_confirmation"));
    let preview = store.assessment_preview(&config).unwrap();
    let second = store
        .assessment_start(&StartAssessment {
            config,
            expected_inventory_revision: preview.inventory_revision,
            operation_id: op(),
        })
        .unwrap();
    // An ordinary early-ended retest remains comparable in configuration and explicitly incomplete.
    store
        .assessment_transition(&TransitionAssessment {
            session_id: second.id.clone(),
            expected_revision: second.revision,
            operation_id: op(),
            action: "end_early".into(),
            item_id: None,
            draft_text: String::new(),
            draft_option: None,
        })
        .unwrap();
    let comparison = store.assessment_compare(&session.id, &second.id).unwrap();
    assert_eq!(comparison["comparable"], true);
    assert_eq!(comparison["second"]["counts"]["unanswered"], 6);
    assert!(comparison["correctRateDelta"].is_null());
    let history = store.assessment_history(0, 20, None).unwrap();
    assert_eq!(history["total"], 2);
    assert_eq!(
        store.find_entries("word", "bright").unwrap()[0].meanings[0].text,
        "原人工释义"
    );
    let page = store
        .assessment_report_items(
            &session.id,
            revised["report"]["revision"].as_i64().unwrap(),
            0,
            20,
        )
        .unwrap();
    let rows = page["items"].as_array().unwrap();
    let existing = rows
        .iter()
        .find(|r| r["item"]["content"]["target"] == "bright")
        .unwrap();
    let new = rows
        .iter()
        .find(|r| {
            r["item"]["content"]["target"] != "bright" && r["item"]["questionType"] == "listening"
        })
        .unwrap();
    let items = vec![
        existing["id"].as_str().unwrap().into(),
        new["id"].as_str().unwrap().into(),
    ];
    let draft = store
        .assessment_collection_prepare(&PrepareAssessmentCollection {
            session_id: session.id.clone(),
            item_ids: items,
            operation_id: op(),
        })
        .unwrap();
    assert_eq!(
        store
            .connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get::<_, usize>(0))
            .unwrap(),
        1
    );
    for candidate in &draft.items {
        let matched = candidate.matches.first();
        let selection = CollectionSelection {
            session_item_id: candidate.session_item_id.clone(),
            target_entry_id: matched.map(|e| e.id.clone()),
            expected_revision: matched.map(|e| e.revision),
            meaning: candidate.meaning.clone(),
            translation: candidate.translation.clone(),
            save_audio: true,
        };
        let receipt = store
            .assessment_collect_item(&draft.draft_id, &selection)
            .unwrap();
        assert_eq!(receipt.created, matched.is_none());
        let entry = store.get_entry(&receipt.entry_id).unwrap();
        assert!(entry.examples.iter().any(|e|e.text==candidate.sentence&&e.context_meaning==candidate.translation));
        if candidate.audio_kind.is_some() {
            assert!(
                entry
                    .examples
                    .iter()
                    .any(|e| e.audio.iter().any(|a| a.kind == "system"))
            );
        }
    }
    let bright = store.find_entries("word", "bright").unwrap().remove(0);
    assert!(bright.meanings.iter().any(|m| m.text == "原人工释义"));
    assert!(
        bright
            .examples
            .iter()
            .any(|e| e.text == "The light is bright.")
    );
    let c = store.connection().unwrap();
    assert_eq!(
        c.query_row(
            "SELECT COUNT(*) FROM assessment_collection_links",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        c.query_row(
            "SELECT COUNT(*) FROM sources WHERE kind='assessment'",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM review_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        0
    );
}
