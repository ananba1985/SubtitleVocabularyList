//! A private native process used to verify the file/SQLite commit boundary.
//! The caller may terminate only this probe after its queued task is recorded.
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::example_input_from_corpus,
    corpus::file_hash,
    error::AppError,
    media::{ImportedMedia, MediaTools},
    reviews::AnswerInput,
    store::Store,
    tasks::{TaskManager, TaskSnapshot},
    vocabulary::CollectionInput,
};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    source_id: String,
    example_id: String,
    operation_id: String,
    baseline_entry: String,
    baseline_asset: String,
    baseline_digest: String,
}

fn submit(
    store: &Store,
    tools: &MediaTools,
    fixture: &Fixture,
    cancelled: &AtomicBool,
) -> Result<serde_json::Value, AppError> {
    let asset = store.ensure_clip(tools, &fixture.source_id, &fixture.example_id, cancelled)?;
    let mut example =
        example_input_from_corpus(store, &fixture.source_id, &fixture.example_id, "喊叫")?;
    example.media_asset_ids.push(asset.id);
    let result = store.collect(&CollectionInput {
        operation_id: fixture.operation_id.clone(),
        kind: "word".into(),
        text: "yelling".into(),
        meaning: "喊叫".into(),
        examples: vec![example],
        target_entry_id: None,
        expected_revision: None,
    })?;
    Ok(serde_json::to_value(result)?)
}

fn start(
    store: &Arc<Store>,
    tasks: &Arc<TaskManager>,
    tools: MediaTools,
    fixture: Fixture,
    gate: bool,
) -> Result<TaskSnapshot, AppError> {
    let operation = fixture.operation_id.clone();
    let worker_store = Arc::clone(store);
    tasks.start(
        "collection",
        &operation,
        "private-file-commit-boundary",
        move |context| {
            if gate {
                let deadline = Instant::now() + Duration::from_secs(20);
                while !worker_store.root().join("writer-locked").is_file() {
                    context.check_cancelled()?;
                    if Instant::now() > deadline {
                        return Err(AppError::new(
                            "timeout",
                            "Private fault-injection gate was not released",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            submit(&worker_store, &tools, &fixture, &context.cancelled)
        },
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 4 {
        return Err("Usage: crash_recovery_probe prepare|run|recover PRIVATE_STORE BUNDLED_TOOLS [CORPUS_JSON VIDEO]".into());
    }
    let root = Path::new(&args[2]);
    let tools = MediaTools::bundled(&args[3]);
    let store = Arc::new(Store::open(root)?);
    let metadata = root.join("crash-fixture.json");
    if args[1] == "prepare" {
        if args.len() != 6 || metadata.exists() {
            return Err("Use a new dedicated private directory and supply corpus/video".into());
        }
        let mut imported: ImportedMedia = serde_json::from_slice(&std::fs::read(&args[4])?)?;
        let audio = root.join("jobs/source.m4a");
        std::fs::copy(&imported.audio_path, &audio)?;
        imported.audio_path = audio;
        let corpus = root.join("jobs/corpus.json");
        std::fs::write(&corpus, serde_json::to_vec(&imported)?)?;
        let source = store.install_corpus(
            Path::new(&args[5]),
            &file_hash(Path::new(&args[5]))?,
            &imported,
            &corpus,
        )?;
        let baseline = store.candidate_examples(&source.id, "breakfast")?.remove(0);
        let asset = store.ensure_clip(&tools, &source.id, &baseline.id, &AtomicBool::new(false))?;
        let mut example =
            example_input_from_corpus(&store, &source.id, &baseline.id, "已确认保存")?;
        example.media_asset_ids.push(asset.id.clone());
        let result = store.collect(&CollectionInput {
            operation_id: Uuid::new_v4().to_string(),
            kind: "word".into(),
            text: "breakfast".into(),
            meaning: "已确认保存".into(),
            examples: vec![example],
            target_entry_id: None,
            expected_revision: None,
        })?;
        let unit = store.review_units("all", "meaning", 0, 20)?.remove(0);
        let question = store.review_question(&unit.id, unit.revision)?;
        store.review_submit(&AnswerInput {
            operation_id: Uuid::new_v4().to_string(),
            question_id: question.id,
            answer: "已确认保存".into(),
            unable: false,
        })?;
        let fixture = Fixture {
            source_id: source.id.clone(),
            example_id: store
                .candidate_examples(&source.id, "yelling")?
                .remove(0)
                .id,
            operation_id: Uuid::new_v4().to_string(),
            baseline_entry: result.entry_id,
            baseline_asset: asset.id.clone(),
            baseline_digest: file_hash(&store.media_file(&asset.id)?)?,
        };
        std::fs::write(&metadata, serde_json::to_vec_pretty(&fixture)?)?;
        println!(
            "{}",
            serde_json::json!({"prepared":true,"schemaVersion":store.schema_version()?})
        );
        return Ok(());
    }
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(&metadata)?)?;
    if !matches!(args[1].as_str(), "run" | "recover") {
        return Err("Unknown probe phase".into());
    }
    let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
    if args[1] == "recover" {
        // The parent verified the sole prior process exited; this private root
        // is never shared with a running desktop application.
        tasks.recover_interrupted()?;
        assert!(
            tasks
                .list()?
                .iter()
                .any(|t| t.operation_id == fixture.operation_id
                    && t.error.as_ref().is_some_and(|e| e.code == "interrupted"))
        );
    }
    let baseline_entry = fixture.baseline_entry.clone();
    let baseline_asset = fixture.baseline_asset.clone();
    let baseline_digest = fixture.baseline_digest.clone();
    assert_eq!(store.get_entry(&baseline_entry)?.collection_count, 1);
    assert_eq!(
        file_hash(&store.media_file(&baseline_asset)?)?,
        baseline_digest
    );
    let task = start(&store, &tasks, tools, fixture, args[1] == "run")?;
    println!(
        "{}",
        serde_json::json!({"taskId":task.id,"pid":std::process::id(),"phase":args[1]})
    );
    std::io::stdout().flush()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let current = tasks.get(&task.id)?;
        if current.terminal() {
            if current.state != "succeeded" {
                return Err(serde_json::to_string(&current.error)?.into());
            }
            assert_eq!(store.get_entry(&baseline_entry)?.collection_count, 1);
            assert_eq!(
                file_hash(&store.media_file(&baseline_asset)?)?,
                baseline_digest
            );
            let entries = store.list_entries("yelling", 0, 10)?;
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].collection_count, 1);
            assert_eq!(store.review_history(None, 0, 30)?.len(), 1);
            println!(
                "{}",
                serde_json::json!({"state":current.state,"taskId":current.id,"result":current.result,"baselinePreserved":true,"newCollectionCount":1,"learningHistoryCount":1})
            );
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("Probe task did not finish within its bounded wait".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
