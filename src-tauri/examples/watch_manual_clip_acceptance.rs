//! One manual OCR interval -> original clip -> existing entry flow; silent and isolated.
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings, example_input_from_corpus},
    media::MediaTools,
    store::Store,
    tasks::TaskManager,
    vocabulary::CollectionInput,
    watch::WatchClipRange,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("new Data directory required")?);
    let video = std::fs::canonicalize(args.next().ok_or("silent synthetic video required")?)?;
    if root.join("vocabulary.sqlite3").exists() {
        return Err("Refusing to reset named data".into());
    }
    let store = Arc::new(Store::open(&root)?);
    store.set_preparation_enabled(false)?;
    let tools = MediaTools::development(".tools/runtime");
    let retained = store.collect(&CollectionInput {
        operation_id: uuid::Uuid::new_v4().to_string(),
        kind: "word".into(),
        text: "arrive".into(),
        meaning: "到达（已有人工释义）".into(),
        examples: vec![],
        target_entry_id: None,
        expected_revision: None,
    })?;
    let before = store.get_entry(&retained.entry_id)?;
    let media = store.watch_media(Some(&video.to_string_lossy()), None)?;
    let mut frame = store
        .watch_text_context(
            &media,
            "I will arrive tomorrow.\n我明天会到。",
            1000,
            3000,
            1,
            "fixture",
            Some(1),
            None,
        )?
        .ok_or("context missing")?;
    frame.start_ms = 1500;
    frame.end_ms = 1500;
    frame.precise_timing = false;
    frame.text_source = "frame_ocr".into();
    frame.location_key = "watch-ocr:1500:acceptance".into();
    let range = WatchClipRange {
        start_ms: 1000,
        end_ms: 3000,
    };
    let manual = frame.with_manual_clip(&range, 8000)?;
    assert!(!frame.precise_timing && frame.start_ms == frame.end_ms);
    assert!(manual.precise_timing && manual.location_key.contains(":manual-clip:1000:3000"));
    let bound = store.bind_watch_context(&video.to_string_lossy(), "Manual OCR clip", &manual)?;
    let clip = store.ensure_clip(
        &tools,
        &bound.source_id,
        &bound.example_id,
        &AtomicBool::new(false),
    )?;
    assert!((clip.duration_ms - 2000).abs() < 120);
    let tasks = TaskManager::new(store.clone(), Arc::new(|_| {}));
    let app = Application::new(
        store.clone(),
        tasks.clone(),
        Settings {
            tools,
            ..Default::default()
        },
    );
    let draft = app.prepare(CollectionInput {
        operation_id: uuid::Uuid::new_v4().to_string(),
        kind: "word".into(),
        text: "arrive".into(),
        meaning: "到达".into(),
        examples: vec![example_input_from_corpus(
            &store,
            &bound.source_id,
            &bound.example_id,
            "我明天会到。",
        )?],
        target_entry_id: None,
        expected_revision: None,
    })?;
    assert_eq!(
        store.get_entry(&retained.entry_id)?.examples.len(),
        0,
        "preview and draft must not collect"
    );
    let started = app.commit(
        draft.draft_id,
        Some(retained.entry_id.clone()),
        Some(before.revision),
        true,
    )?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let task = tasks.get(&started.id)?;
        if task.terminal() {
            assert_eq!(task.state, "succeeded", "{task:?}");
            break;
        }
        if Instant::now() > deadline {
            return Err("normal collection did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let entry = store.get_entry(&retained.entry_id)?;
    assert!(before.meanings.iter().all(|old| {
        entry
            .meanings
            .iter()
            .any(|saved| saved.id == old.id && saved.text == old.text)
    }));
    assert_eq!(entry.examples.len(), 1);
    assert_eq!(entry.examples[0].start_ms, Some(1000));
    assert_eq!(entry.examples[0].end_ms, Some(3000));
    assert_eq!(entry.examples[0].audio.len(), 1);
    assert_eq!(entry.examples[0].audio[0].id, clip.id);
    assert!(store.media_file(&clip.id)?.is_file());
    println!(
        "{}",
        serde_json::json!({"dataDirectory":root,"entryId":entry.id,"sourceId":bound.source_id,"manualRange":range,"audio":clip,"automaticOcrRemainsTextOnly":!frame.precise_timing,"existingMeaningPreserved":true,"previewDidNotCollect":true,"audioPlayed":false,"modelRequests":0})
    );
    Ok(())
}
