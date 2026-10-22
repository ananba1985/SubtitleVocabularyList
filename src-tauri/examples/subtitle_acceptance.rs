//! One real local import path; use a new, named Data directory and synthetic media.
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    media::ImportOptions,
    store::Store,
    tasks::TaskManager,
    vocabulary::CollectionInput,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("expected new Data directory")?);
    let video = std::fs::canonicalize(PathBuf::from(
        args.next().ok_or("expected bilingual synthetic video")?,
    ))?;
    if root.join("vocabulary.sqlite3").exists() {
        return Err("Refusing to reset existing acceptance data".into());
    }
    std::fs::create_dir_all(&root)?;
    let root = std::fs::canonicalize(root)?;
    let store = Arc::new(Store::open(&root)?);
    store.set_preparation_enabled(false)?;
    let retained = store.collect(&CollectionInput {
        operation_id: uuid::Uuid::new_v4().to_string(),
        kind: "word".into(),
        text: "keep".into(),
        meaning: "保留的人工释义".into(),
        examples: vec![],
        target_entry_id: None,
        expected_revision: None,
    })?;
    let before = serde_json::to_value(store.get_entry(&retained.entry_id)?)?;
    let tasks = TaskManager::new(store.clone(), Arc::new(|_| {}));
    let app = Application::new(store.clone(), tasks.clone(), Settings::default());
    let started = app.import_with_options(
        vec![video.to_string_lossy().into_owned()],
        uuid::Uuid::new_v4().to_string(),
        ImportOptions {
            audio_stream: Some(1),
            subtitle_stream: Some(2),
            subtitle_mode: "embedded".into(),
            external_subtitle: None,
        },
    )?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let task = tasks.get(&started.id)?;
        if task.terminal() {
            assert_eq!(task.state, "succeeded", "{task:?}");
            break;
        }
        if Instant::now() > deadline {
            return Err("normal import did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let source = store.sources()?.remove(0);
    let bundle = store
        .subtitle_bundle(&source.id)?
        .ok_or("subtitle bundle missing")?;
    assert!(
        bundle
            .assets
            .iter()
            .any(|a| a.language == "en" && a.format == "ass")
    );
    assert!(
        bundle
            .assets
            .iter()
            .any(|a| a.language == "zh" && a.cues.len() == 3)
    );
    let raw = bundle.assets.iter().find(|a| a.language == "en").unwrap();
    assert!(std::fs::read_to_string(root.join(&raw.relative_path))?.contains("pos(230,120)"));
    let media = store.watch_media(None, Some(&source.id))?;
    let context = store
        .watch_context(&media, 1500, 0, 1)?
        .ok_or("watch context missing")?;
    assert_eq!(context.text,"I will arrive tomorrow.");
    assert_eq!(context.translation, "我明天会到。\n请放心。");
    assert_eq!(
        store
            .sentence_translation("Please wait for me.")?
            .as_deref(),
        Some("请等我。")
    );
    assert!(
        !store
            .explanation_work(Some(&source.id))?
            .iter()
            .any(|w| w.kind == "sentence")
    );
    assert_eq!(
        before,
        serde_json::to_value(store.get_entry(&retained.entry_id)?)?
    );
    println!(
        "{}",
        serde_json::json!({"sourceId":source.id,"dataDirectory":root,"assets":bundle.info(),"context":context,"cachedSentenceJobs":0,"personalEntryPreserved":true,"modelRequests":0})
    );
    Ok(())
}
