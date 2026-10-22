//! One downloaded-subtitle normal path; no HTTP, windows, playback or user data.
use std::{path::PathBuf, sync::atomic::AtomicBool};
use subtitle_vocabulary_list_core::{
    corpus::file_hash,
    media::{ImportedMedia, MediaTools, Segment},
    online_subtitles::{self as online, SubtitleCandidate, SubtitleSearch},
    store::Store,
    subtitles::SubtitleBundle,
    vocabulary::CollectionInput,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("new Data directory required")?);
    let video = std::fs::canonicalize(args.next().ok_or("synthetic video required")?)?;
    let archive = PathBuf::from(args.next().ok_or("Chinese subtitle ZIP required")?);
    if root.join("vocabulary.sqlite3").exists() {
        return Err("Refusing to reset existing named data".into());
    }
    std::fs::create_dir_all(&root)?;
    let root = std::fs::canonicalize(root)?;
    let store = Store::open(&root)?;
    store.set_preparation_enabled(false)?;
    let tools = MediaTools::development(".tools/runtime");
    let flag = AtomicBool::new(false);
    let hash = file_hash(&video)?;
    let retained = store.collect(&CollectionInput {
        operation_id: uuid::Uuid::new_v4().to_string(),
        kind: "word".into(),
        text: "keep".into(),
        meaning: "保留人工释义".into(),
        examples: vec![],
        target_entry_id: None,
        expected_revision: None,
    })?;
    let before = serde_json::to_value(store.get_entry(&retained.entry_id)?)?;
    let imported = ImportedMedia {
        title: "Modern Family S01E02".into(),
        duration_ms: 8000,
        audio_path: video.clone(),
        audio_stream: 1,
        subtitle_stream: Some(2),
        text_source: "subtitle".into(),
        warnings: vec![],
        segments: vec![
            Segment {
                location_key: "subtitle:1".into(),
                text: "I will arrive tomorrow.".into(),
                start_ms: 1000,
                end_ms: 3000,
            },
            Segment {
                location_key: "subtitle:2".into(),
                text: "Please wait for me.".into(),
                start_ms: 3000,
                end_ms: 5000,
            },
        ],
    };
    let corpus_path = root.join("fixture-corpus.json");
    std::fs::write(&corpus_path, serde_json::to_vec(&imported)?)?;
    let source = store.install_corpus(&video, &hash, &imported, &corpus_path)?;
    let local = SubtitleBundle {
        media_sha256: hash.clone(),
        selected_chinese_id: None,
        assets: vec![],
        warnings: vec![],
    };
    store.save_subtitle_bundle(&source.id, &local)?;
    store.save_sentence_translation(
        "Please wait for me.",
        "请在这里等我。",
        "acceptance://retained",
        "已保存译文",
    )?;
    let query = online::identify("Modern.Family.S01E02.1080p.mkv");
    assert_eq!(
        (&*query.title, query.season, query.episode),
        ("Modern Family", Some(1), Some(2))
    );
    let search = SubtitleSearch {
        query,
        media_sha256: hash,
        ..Default::default()
    };
    let candidate = SubtitleCandidate {
        id: "synthetic-subtitle".into(),
        title: "Modern Family".into(),
        title_id: 1,
        language: "zh".into(),
        release_name: "Synthetic acceptance".into(),
        name: "Modern.Family.S01E02.zh.zip".into(),
        url: "https://dl.subdl.com/subtitle/acceptance.zip".into(),
        season: Some(1),
        episode: Some(2),
        fps: "24".into(),
    };
    let download = online::stage_download(
        &store,
        &source.id,
        &search,
        &candidate,
        &std::fs::read(archive)?,
        &flag,
    )?;
    assert_eq!(download.files.len(), 1);
    let file_id = &download.files[0].id;
    let preview = online::preview(&store, &tools, &download.id, file_id, 0, &flag)?;
    assert_eq!((preview.cue_count, preview.matched_sentences), (3, 2));
    assert!(preview.warnings.is_empty(), "{:?}", preview.warnings);
    assert_eq!(preview.pairs[0].chinese, "我明天会到。\n请放心。");
    let info = online::commit(&store, &tools, &download.id, file_id, 0, true, true, &flag)?;
    let id = info
        .selected_chinese_id
        .ok_or("Chinese selection missing")?;
    let bundle = store.subtitle_bundle(&source.id)?.ok_or("bundle missing")?;
    let asset = bundle
        .assets
        .iter()
        .find(|a| a.id == id)
        .ok_or("download asset missing")?;
    assert!(
        asset
            .provenance
            .as_ref()
            .is_some_and(|p| p.provider == "SubDL" && p.version_confirmed)
    );
    let raw_hash = file_hash(&root.join(&asset.relative_path))?;
    assert_eq!(
        store
            .sentence_translation("I will arrive tomorrow.")?
            .as_deref(),
        Some("我明天会到。\n请放心。")
    );
    assert_eq!(
        store
            .sentence_translation("Please wait for me.")?
            .as_deref(),
        Some("请在这里等我。")
    );
    store.set_subtitle_offset(&source.id, &id, 1000)?;
    // A subsequent local rescan keeps the downloaded file, calibration and choice.
    store.save_subtitle_bundle(&source.id, &local)?;
    let restored = store
        .subtitle_bundle(&source.id)?
        .ok_or("bundle missing after refresh")?;
    assert_eq!(restored.selected_chinese_id.as_deref(), Some(id.as_str()));
    assert_eq!(restored.assets[0].time_offset_ms, 1000);
    assert_eq!(
        restored.translation(2000, 4000).as_deref(),
        Some("我明天会到。\n请放心。")
    );
    assert_eq!(
        file_hash(&root.join(&restored.assets[0].relative_path))?,
        raw_hash
    );
    assert_eq!(
        before,
        serde_json::to_value(store.get_entry(&retained.entry_id)?)?
    );
    assert_eq!(store.source(&source.id)?.example_count, 2);
    println!(
        "{}",
        serde_json::json!({"dataDirectory":root,"sourceId":source.id,"preview":preview,"downloadedAssets":restored.info(),"offsetPersistedMs":1000,"rawSubtitlePreserved":true,"existingTranslationPreserved":true,"personalEntryPreserved":true,"httpRequests":0,"modelRequests":0})
    );
    Ok(())
}
