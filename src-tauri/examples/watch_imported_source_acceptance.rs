//! One existing explicit-track import -> watch path; no migration, GUI, sound or model.
use std::path::PathBuf;
use subtitle_vocabulary_list_core::{
    corpus::file_hash,
    media::{ImportedMedia, Segment},
    store::Store,
    vocabulary::{CollectionInput, digest},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("new named Data required")?);
    let video = std::fs::canonicalize(args.next().ok_or("synthetic video required")?)?;
    if root.join("vocabulary.sqlite3").exists() {
        return Err("Refusing to reset existing named data".into());
    }
    std::fs::create_dir_all(&root)?;
    let store = Store::open(&root)?;
    store.set_preparation_enabled(false)?;
    let saved = store.collect(&CollectionInput {
        operation_id: uuid::Uuid::new_v4().to_string(),
        kind: "word".into(),
        text: "keep".into(),
        meaning: "原人工释义".into(),
        examples: vec![],
        target_entry_id: None,
        expected_revision: None,
    })?;
    let entry_before = serde_json::to_value(store.get_entry(&saved.entry_id)?)?;
    let hash = file_hash(&video)?;
    let fingerprint = digest(&serde_json::to_vec(&(
        "video-options-v1",
        &hash,
        Some(1usize),
        Some(2usize),
        "embedded",
        None::<String>,
    ))?);
    let imported = ImportedMedia {
        title: "Existing explicit-track import".into(),
        duration_ms: 8000,
        audio_path: video.clone(),
        audio_stream: 1,
        subtitle_stream: Some(2),
        text_source: "subtitle".into(),
        warnings: vec![],
        segments: vec![Segment {
            location_key: "subtitle:1".into(),
            text: "I will arrive tomorrow.".into(),
            start_ms: 1000,
            end_ms: 3000,
        }],
    };
    let corpus = root.join("existing-corpus.json");
    std::fs::write(&corpus, serde_json::to_vec(&imported)?)?;
    let source = store.install_corpus(&video, &fingerprint, &imported, &corpus)?;
    assert!(store.subtitle_bundle(&source.id)?.is_none());
    let before = serde_json::to_value(store.source(&source.id)?)?;
    store.save_sentence_translation(
        "I will arrive tomorrow.",
        "我明天会到。",
        "acceptance://saved",
        "原有译文",
    )?;
    let media = store.watch_media(None, Some(&source.id))?;
    assert_eq!(media.source_id.as_deref(), Some(source.id.as_str()));
    assert_eq!(
        (
            &*media.media_sha256,
            media.audio_stream,
            media.subtitle_stream
        ),
        (&*hash, Some(1), Some(2))
    );
    let context = store
        .watch_context(&media, 1500, 0, 1)?
        .ok_or("existing context missing")?;
    assert_eq!(
        (&*context.text, &*context.translation),
        ("I will arrive tomorrow.", "我明天会到。")
    );
    assert_eq!(serde_json::to_value(store.source(&source.id)?)?, before);
    assert_eq!(
        serde_json::to_value(store.get_entry(&saved.entry_id)?)?,
        entry_before
    );
    let stored_fingerprint: String = store.connection()?.query_row(
        "SELECT fingerprint FROM sources WHERE id=?",
        [&source.id],
        |row| row.get(0),
    )?;
    assert_eq!(stored_fingerprint, fingerprint);
    println!(
        "{}",
        serde_json::json!({"dataDirectory":root,"sourceId":source.id,"mediaSha256":hash,"sourceFingerprint":fingerprint,"context":context,"originalSourcePreserved":true,"personalEntryPreserved":true,"subtitleBundleCreated":false,"reimportRequired":false,"modelRequests":0,"audioPlayed":false})
    );
    Ok(())
}
