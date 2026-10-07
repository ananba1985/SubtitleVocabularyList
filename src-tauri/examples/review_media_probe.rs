//! Installs an already parsed private corpus for real review/audio integration checks.
//! Does not run transcription again or contain media, credentials or personal vocabulary.
use std::{path::PathBuf, sync::atomic::AtomicBool};
use subtitle_vocabulary_list::{
    application::example_input_from_corpus,
    corpus::file_hash,
    media::{ImportedMedia, MediaTools},
    store::Store,
    vocabulary::CollectionInput,
};
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 8 {
        return Err("Usage: review_media_probe PRIVATE_STORE CORPUS_JSON INPUT_VIDEO FFMPEG FFPROBE TARGET MEANING".into());
    }
    let store = Store::open(&args[1])?;
    let corpus_path = PathBuf::from(&args[2]);
    let input = PathBuf::from(&args[3]);
    let imported: ImportedMedia = serde_json::from_slice(&std::fs::read(&corpus_path)?)?;
    let source = store.install_corpus(&input, &file_hash(&input)?, &imported, &corpus_path)?;
    let example = store
        .candidate_examples(&source.id, &args[6].to_lowercase())?
        .into_iter()
        .next()
        .ok_or("Target missing from corpus")?;
    let tools = MediaTools {
        ffmpeg: PathBuf::from(&args[4]),
        ffprobe: PathBuf::from(&args[5]),
        ..MediaTools::development("tools")
    };
    let asset = store.ensure_clip(&tools, &source.id, &example.id, &AtomicBool::new(false))?;
    let mut example_input =
        example_input_from_corpus(&store, &source.id, &example.id, "真实原声验证语境")?;
    example_input.media_asset_ids.push(asset.id.clone());
    let result = store.collect(&CollectionInput {
        operation_id: Uuid::new_v4().to_string(),
        kind: "word".into(),
        text: args[6].clone(),
        meaning: args[7].clone(),
        examples: vec![example_input],
        target_entry_id: None,
        expected_revision: None,
    })?;
    println!(
        "{}",
        serde_json::json!({"entryId":result.entry_id,"sourceId":source.id,"assetId":asset.id,"durationMs":asset.duration_ms,"schema":store.schema_version()?})
    );
    Ok(())
}
