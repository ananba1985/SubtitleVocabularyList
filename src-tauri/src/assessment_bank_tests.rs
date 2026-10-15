use crate::{assessment_bank::*, store::Store, vocabulary::digest};
use rusqlite::{Connection, params};
use serde_json::json;
use uuid::Uuid;

// TC-53: one normal upgrade -> reviewed, published bank. Synthetic data stays isolated.
#[test]
fn tc53_upgrade_catalog_and_reviewed_bank() {
    let directory = tempfile::tempdir().unwrap();
    let old = Connection::open(directory.path().join("vocabulary.sqlite3")).unwrap();
    for migration in [
        include_str!("../migrations/001_initial.sql"),
        include_str!("../migrations/002_imports.sql"),
        include_str!("../migrations/003_reviews.sql"),
        include_str!("../migrations/004_example_contexts.sql"),
        include_str!("../migrations/005_sync.sql"),
        include_str!("../migrations/006_explanations.sql"),
        include_str!("../migrations/007_query_results.sql"),
        include_str!("../migrations/008_sentence_translations.sql"),
    ] {
        old.execute_batch(migration).unwrap();
    }
    old.execute_batch("PRAGMA user_version=8;
        INSERT INTO entries(id,kind,text,match_key,created_at,updated_at) VALUES ('personal','word','bright','bright',1,1);
        INSERT INTO meanings(id,entry_id,text,origin,created_at) VALUES ('meaning','personal','人工释义保持','user',1);
        INSERT INTO learning_units(id,entry_id,scope_key,dimension) VALUES ('unit','personal','meaning','meaning');
        INSERT INTO review_states(unit_id,state_json,due_at,relearn_at) VALUES ('unit','{}',123,456);").unwrap();
    drop(old);
    std::fs::create_dir_all(directory.path().join("dictionaries")).unwrap();
    let dictionary = Connection::open(directory.path().join("dictionaries/base.sqlite3")).unwrap();
    dictionary.execute_batch("PRAGMA user_version=2;
        CREATE TABLE dictionary_entries(word TEXT,meaning TEXT);
        CREATE TABLE dictionary_aliases(form TEXT,lemma TEXT);
        CREATE TABLE metadata(key TEXT,value TEXT);
        INSERT INTO metadata VALUES ('source','TC-53 synthetic reference');
        INSERT INTO dictionary_entries VALUES ('bright','明亮的');
        CREATE TABLE frequency_catalogs(id TEXT,kind TEXT,title TEXT,source_url TEXT,source_version TEXT,source_sha256 TEXT,rank_unit TEXT,raw_rows INTEGER,max_rank INTEGER,notice TEXT);
        INSERT INTO frequency_catalogs VALUES ('fixture','word','固定合成词频','https://example.invalid','v1','synthetic','original_rank',3,3,'TC-53 fixture');
        CREATE TABLE frequency_targets(catalog_id TEXT,rank INTEGER,text TEXT,target_key TEXT,lexeme_key TEXT,mapping_basis TEXT,frequency INTEGER,eligible INTEGER,seed_json TEXT);
        INSERT INTO frequency_targets VALUES ('fixture',1,'bright','bright','bright','identity',NULL,1,NULL),('fixture',2,'without-chinese','without-chinese','without-chinese','identity',NULL,1,NULL),('fixture',3,'lights','lights','light','explicit-fixture-lemma',NULL,1,NULL);").unwrap();
    drop(dictionary);
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 9);
    let targets = store.assessment_targets("fixture", 3).unwrap();
    assert_eq!(
        targets.iter().map(|t| t.rank).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(targets[2].lexeme_key, "light");
    assert_eq!(
        store.assessment_inventory().unwrap().catalogs[0].eligible_targets,
        3
    );
    let settings = store.assessment_settings().unwrap();
    let saved = store
        .assessment_save_settings(settings, &Uuid::new_v4().to_string())
        .unwrap();
    assert_eq!(saved.config.question_count, 30);
    let bank = Uuid::new_v4().to_string();
    store.connection().unwrap().execute("INSERT INTO assessment_banks(id,state,sources_json,policy_version,created_at) VALUES (?,'draft',? ,?,1)",params![bank,serde_json::to_string(&store.assessment_catalogs().unwrap()).unwrap(),POLICY]).unwrap();
    let mut wave = b"RIFF".to_vec();
    wave.extend_from_slice(&(36u32 + 800).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&8000u32.to_le_bytes());
    wave.extend_from_slice(&16000u32.to_le_bytes());
    wave.extend_from_slice(&2u16.to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&800u32.to_le_bytes());
    wave.resize(844, 0);
    std::fs::write(store.root().join("media/fixture.wav"), &wave).unwrap();
    let mut questions = vec![];
    for kind in TYPES {
        questions.push((
            kind.into(),
            QuestionDraft {
                target: "bright".into(),
                meaning: "明亮的".into(),
                sentence: "The room is bright.".into(),
                translation: "房间很明亮。".into(),
                prompt: if kind == "cloze" {
                    "The room is ___.".into()
                } else if kind == "listening" {
                    "听音选择含义。".into()
                } else {
                    "bright 在句中是什么意思？".into()
                },
                choices: if kind == "cloze" {
                    vec![]
                } else {
                    vec![
                        Choice {
                            id: "right".into(),
                            text: "明亮的".into(),
                        },
                        Choice {
                            id: "a".into(),
                            text: "潮湿的".into(),
                        },
                        Choice {
                            id: "b".into(),
                            text: "拥挤的".into(),
                        },
                    ]
                },
                correct_option_id: (kind != "cloze").then(|| "right".into()),
                accepted_answers: if kind == "cloze" {
                    vec!["bright".into()]
                } else {
                    vec![]
                },
                explanation: "本题描述房间的光线明亮，人工核对样本。".into(),
                audio: (kind == "listening").then(|| AssessmentAudio {
                    kind: "system".into(),
                    relative_path: "media/fixture.wav".into(),
                    sha256: digest(&wave),
                    text: "bright. The room is bright.".into(),
                    voice_id: "synthetic-test-only".into(),
                    asset_id: None,
                }),
                origin: "TC-53 synthetic fixture".into(),
            },
        ));
    }
    assert_eq!(
        store
            .assessment_save_candidates(&bank, &targets[0], &questions)
            .unwrap(),
        3
    );
    let page = store
        .assessment_bank_items(&bank, Some("needs_review"), 0, 20)
        .unwrap();
    assert_eq!(page.total, 3);
    for item in page.items {
        let current = store.assessment_bank_summary(&bank).unwrap();
        store
            .assessment_bank_review(&ReviewBank {
                operation_id: Uuid::new_v4().to_string(),
                bank_id: bank.clone(),
                expected_revision: current.revision,
                action: "approve".into(),
                item_id: Some(item.id),
                expected_item_revision: Some(item.revision),
                content: None,
                note: "已核对目标、语境、答案与合成音频描述。".into(),
            })
            .unwrap();
    }
    let current = store.assessment_bank_summary(&bank).unwrap();
    let published = store
        .assessment_bank_review(&ReviewBank {
            operation_id: Uuid::new_v4().to_string(),
            bank_id: bank.clone(),
            expected_revision: current.revision,
            action: "publish".into(),
            item_id: None,
            expected_item_revision: None,
            content: None,
            note: "TC-53 核对完成。".into(),
        })
        .unwrap();
    assert_eq!(published["publishedItems"], json!(3));
    drop(store);
    let reopened = Store::open(directory.path()).unwrap();
    let summary = reopened.assessment_bank_summary(&bank).unwrap();
    assert_eq!(summary.state, "published");
    assert_eq!(summary.target_count, 1);
    assert_eq!(summary.item_counts.values().sum::<usize>(), 3);
    let c = reopened.connection().unwrap();
    assert_eq!(
        c.query_row("SELECT text FROM meanings WHERE id='meaning'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "人工释义保持"
    );
    assert_eq!(
        c.query_row(
            "SELECT due_at||':'||relearn_at FROM review_states WHERE unit_id='unit'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "123:456"
    );
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get::<_, usize>(0))
            .unwrap(),
        1
    );
    println!(
        "TC-53 OK: schema 8 -> 9; original ranks 1/2/3 retained including Chinese gap; three types reviewed and published; personal entry/meaning/review unchanged after reopen."
    );
}
