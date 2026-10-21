//! Creates a named, isolated acceptance library using the supplied synthetic video.
use std::path::PathBuf;
use subtitle_vocabulary_list_core::{
    corpus,
    media::{ImportedMedia, Segment},
    store::Store,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("expected isolated Data directory")?);
    let video = std::fs::canonicalize(PathBuf::from(
        args.next().ok_or("expected synthetic video")?,
    ))?;
    let audio = std::fs::canonicalize(PathBuf::from(
        args.next().ok_or("expected synthetic audio")?,
    ))?;
    let store = Store::open(&root)?;
    let imported = ImportedMedia {
        title: "边看边学 · 隔离验收片段".into(),
        duration_ms: 45000,
        audio_path: audio,
        audio_stream: 1,
        subtitle_stream: Some(2),
        text_source: "embedded:srt:2".into(),
        warnings: vec![],
        segments: vec![
            Segment {
                location_key: "subtitle:1000:35000".into(),
                text: "I will arrive tomorrow.".into(),
                start_ms: 1000,
                end_ms: 35000,
            },
            Segment {
                location_key: "subtitle:36000:44000".into(),
                text: "Please wait for me.".into(),
                start_ms: 36000,
                end_ms: 44000,
            },
        ],
    };
    let corpus = root.join("jobs/watch-acceptance-corpus.json");
    std::fs::write(&corpus, serde_json::to_vec(&imported)?)?;
    let source = store.install_corpus(&video, &corpus::file_hash(&video)?, &imported, &corpus)?;
    store.save_sentence_translation(
        "I will arrive tomorrow.",
        "我明天会到。",
        "fixture",
        "saved-import-translation",
    )?;
    // The second line is deliberately left for a later normal background-preparation result.
    println!(
        "{}",
        serde_json::json!({"dataDirectory":root,"sourceId":source.id,"video":video,"translation":"我明天会到。","secondSentence":"Please wait for me."})
    );
    Ok(())
}
