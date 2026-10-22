//! Minimal normal-path check without opening windows or playing audio.
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    media::MediaTools,
    ocr,
    queries::{QueryInput, QueryOrigin},
    store::Store,
    tasks::TaskManager,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("new Data directory required")?);
    let video = std::fs::canonicalize(PathBuf::from(
        args.next().ok_or("synthetic video required")?,
    ))?;
    let image = PathBuf::from(args.next().ok_or("known English frame required")?);
    if root.join("vocabulary.sqlite3").exists() {
        return Err("Refusing to reset existing named data".into());
    }
    let store = Arc::new(Store::open(&root)?);
    store.set_preparation_enabled(false)?;
    let tools = MediaTools::development(".tools/runtime");
    let media = store.watch_media(Some(&video.to_string_lossy()), None)?;
    let context = store
        .watch_text_context(
            &media,
            "I will arrive tomorrow.\n我明天会到。",
            1021,
            3021,
            1,
            "ff:2",
            Some(1),
            None,
        )?
        .ok_or("missing context")?;
    assert!(store.list_entries("", 0, 50)?.is_empty());
    assert_eq!(
        store
            .connection()?
            .query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))?,
        0
    );
    let bound = store.bind_watch_context(&video.to_string_lossy(), "Direct playback", &context)?;
    let clip = store.ensure_clip(
        &tools,
        &bound.source_id,
        &bound.example_id,
        &AtomicBool::new(false),
    )?;
    let saved = store.watch_collect(&bound, Some(&clip.id))?;
    let entry = store.get_entry(&saved.entry_id)?;
    assert_eq!(entry.examples.len(), 1);
    assert_eq!(entry.examples[0].audio.len(), 1);
    assert!((clip.duration_ms - 2000).abs() < 120);
    assert!(
        store.sources()?.is_empty(),
        "light source must not claim full import"
    );

    let recognized = ocr::recognize(&tools.tesseract, &image, &AtomicBool::new(false))?;
    assert!(recognized.contains("will arrive tomorrow"), "{recognized}");
    let mut frame = context.clone();
    // Proofreading is required: this fixture's initial I may be recognized as |.
    frame.text = "I will arrive tomorrow.".into();
    frame.translation = "我明天会到。".into();
    frame.source_id = bound.source_id.clone();
    frame.example_id.clear();
    frame.location_key = "watch-ocr:1500:acceptance".into();
    frame.start_ms = 1500;
    frame.end_ms = 1500;
    frame.precise_timing = false;
    frame.text_source = "frame_ocr".into();
    frame.audio_stream = None;
    let frame = store.bind_watch_context(&video.to_string_lossy(), "Direct playback", &frame)?;
    let text_saved = store.watch_collect(&frame, None)?;
    let text_entry = store.get_entry(&text_saved.entry_id)?;
    let text_example = text_entry
        .examples
        .iter()
        .find(|e| e.id == frame.example_id)
        .ok_or("OCR example missing")?;
    assert!(
        text_example.start_ms.is_none()
            && text_example.end_ms.is_none()
            && text_example.audio.is_empty()
    );

    let tasks = TaskManager::new(store.clone(), Arc::new(|_| {}));
    let app = Arc::new(Application::new(
        store.clone(),
        tasks,
        Settings {
            tools,
            ..Default::default()
        },
    ));
    let input = |kind: &str, target: &str, mode: &str| QueryInput {
        target: target.into(),
        context: String::new(),
        mode: mode.into(),
        source_language: "en".into(),
        target_language: "zh-CN".into(),
        origin: QueryOrigin {
            kind: kind.into(),
            source_id: None,
            source_title: String::new(),
            location_key: "acceptance:1".into(),
        },
    };
    let selection = app
        .queries
        .open(input("selection", "keep", "explain"), None)?;
    let watch = app.queries.open(
        input("watch", "The train arrives at noon.", "translate"),
        None,
    )?;
    assert_eq!(
        app.queries.get(None)?.unwrap().session_id,
        selection.session_id
    );
    let previews = Arc::new(AtomicUsize::new(0));
    let observed = previews.clone();
    app.queries.observe(Arc::new(move |s| {
        if s.results
            .iter()
            .any(|r| r.state == "running" && r.value.is_some())
        {
            observed.fetch_add(1, Ordering::Relaxed);
        }
    }));
    app.query_start(
        watch.session_id.clone(),
        watch.input_revision,
        vec!["qwen".into()],
        false,
        true,
    )?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let result = loop {
        let state = app.queries.get(Some(&watch.session_id))?.unwrap();
        if state
            .results
            .iter()
            .all(|r| !matches!(r.state.as_str(), "queued" | "running"))
        {
            break state;
        }
        if Instant::now() > deadline {
            app.queries.cancel(&watch.session_id, None)?;
            return Err("query did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(result.results[0].state, "ready", "{:?}", result.results);
    assert!(
        result.results[0]
            .value
            .as_ref()
            .unwrap()
            .translation
            .chars()
            .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
    );
    assert_eq!(
        app.queries.get(None)?.unwrap().session_id,
        selection.session_id
    );
    println!(
        "{}",
        serde_json::json!({"dataDirectory":root,"sourceCreatedOnlyOnCollection":true,"fullImportCreated":false,"originalClipMs":clip.duration_ms,"ocrRecognized":recognized,"ocrTextOnly":true,"query":result,"streamPreviews":previews.load(Ordering::Relaxed),"globalSelectionPreserved":true})
    );
    Ok(())
}
