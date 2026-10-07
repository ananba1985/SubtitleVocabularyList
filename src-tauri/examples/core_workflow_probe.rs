//! Measures the actual local workflow without controlling desktop windows.
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    media::{ImportOptions, MediaTools},
    reviews::AnswerInput,
    store::Store,
    tasks::{TaskManager, TaskSnapshot},
    vocabulary::{CollectionInput, ExampleInput},
};
use uuid::Uuid;

fn wait(tasks: &TaskManager, task: &TaskSnapshot) -> TaskSnapshot {
    let deadline = Instant::now() + Duration::from_secs(70);
    loop {
        let value = tasks.get(&task.id).unwrap();
        if value.terminal() {
            return value;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(
            "Usage: core_workflow_probe NEW_PRIVATE_STORE BUNDLED_TOOLS PRIVATE_VIDEO".into(),
        );
    }
    let root = Path::new(&args[1]);
    if root.exists() {
        return Err("Use a new private store".into());
    }
    let store = Arc::new(Store::open(root)?);
    for index in 0..1000 {
        store.collect(&CollectionInput {
            operation_id: Uuid::new_v4().to_string(),
            kind: "word".into(),
            text: format!("bench-{index:04}"),
            meaning: "基准释义".into(),
            examples: vec![ExampleInput {
                text: format!("An example for bench-{index:04}."),
                ..Default::default()
            }],
            target_entry_id: None,
            expected_revision: None,
        })?;
    }
    let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
    let settings = Settings {
        tools: MediaTools::bundled(&args[2]),
        ..Default::default()
    };
    let app = Application::new(Arc::clone(&store), Arc::clone(&tasks), settings.clone());
    let import = app.import_with_options(
        vec![args[3].clone()],
        Uuid::new_v4().to_string(),
        ImportOptions {
            subtitle_mode: "speech".into(),
            ..Default::default()
        },
    )?;
    let explanation = app.explain_start(
        "reluctant".into(),
        "She was reluctant to interrupt the conversation.".into(),
        Uuid::new_v4().to_string(),
    )?;
    let started = Instant::now();
    let mut latencies = vec![];
    while !tasks.get(&import.id)?.terminal() || !tasks.get(&explanation.id)?.terminal() {
        let query = Instant::now();
        assert_eq!(store.list_entries("bench-", 0, 40)?.len(), 40);
        latencies.push(query.elapsed().as_micros() as u64);
        if started.elapsed() > Duration::from_secs(70) {
            return Err("Concurrent workflow timed out".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let imported = tasks.get(&import.id)?;
    let explained = tasks.get(&explanation.id)?;
    if imported.state != "succeeded" || explained.state != "succeeded" {
        return Err(format!(
            "Actual local workflow failed: {:?}, {:?}",
            imported.error, explained.error
        )
        .into());
    }
    latencies.sort_unstable();
    assert!(!latencies.is_empty());
    let percentile = latencies[((latencies.len() * 95).div_ceil(100)).saturating_sub(1)];
    let unavailable = Application::new(
        Arc::clone(&store),
        Arc::clone(&tasks),
        Settings {
            model_url: "http://127.0.0.1:1".into(),
            ..settings
        },
    );
    let failed = unavailable.explain_start(
        "unavailable".into(),
        "Synthetic unavailable endpoint.".into(),
        Uuid::new_v4().to_string(),
    )?;
    let failed = wait(&tasks, &failed);
    assert_eq!(failed.state, "failed");
    let fallback = store.collect(&CollectionInput {
        operation_id: Uuid::new_v4().to_string(),
        kind: "word".into(),
        text: "manual-fallback".into(),
        meaning: "手动录入".into(),
        examples: vec![],
        target_entry_id: None,
        expected_revision: None,
    })?;
    let unit = store.review_units("all", "meaning", 0, 40)?.remove(0);
    let question = store.review_question(&unit.id, unit.revision)?;
    let attempt = store.review_submit(&AnswerInput {
        operation_id: Uuid::new_v4().to_string(),
        question_id: question.id,
        answer: "基准释义".into(),
        unable: false,
    })?;
    assert_eq!(
        serde_json::to_value(attempt.outcome)?,
        serde_json::json!("correct")
    );
    assert_eq!(store.get_entry(&fallback.entry_id)?.text, "manual-fallback");
    assert_eq!(store.list_entries("bench-", 0, 40)?.len(), 40);
    println!(
        "{}",
        serde_json::json!({"seedEntries":1000,"readSamples":latencies.len(),"readP95Micros":percentile,"readMaxMicros":latencies.last(),"importMs":imported.updated_at-imported.created_at,"explanationMs":explained.updated_at-explained.created_at,"meaning":explained.result.as_ref().map(|v|&v["meaning"]),"modelUnavailableCode":failed.error.as_ref().map(|e|&e.code),"fallbackCollection":true,"fallbackQuizOutcome":attempt.outcome})
    );
    Ok(())
}
