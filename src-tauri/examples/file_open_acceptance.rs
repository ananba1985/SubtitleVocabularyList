//! Normal file-entry planning and isolated desktop fixture; no GUI or audio.
use std::path::PathBuf;
use subtitle_vocabulary_list_core::{
    playback::{PlaybackState, file_arguments},
    store::Store,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("new acceptance directory required")?);
    let fixture = PathBuf::from(args.next().ok_or("silent video fixture required")?);
    if root.exists() {
        return Err("Existing acceptance data must be preserved; choose a new directory".into());
    }
    std::fs::create_dir_all(root.join("Series"))?;
    std::fs::copy(&fixture, root.join("Start.mkv"))?;
    for name in ["Episode10.mkv", "Episode2.mkv"] {
        std::fs::copy(&fixture, root.join("Series").join(name))?;
    }
    std::fs::write(
        root.join("Chinese.srt"),
        "1\n00:00:00,000 --> 00:00:08,000\n拖入字幕已正常加载。\n",
    )?;
    let root = dunce::canonicalize(root)?;
    let store = Store::open(root.join("Data"))?;
    let inputs = file_arguments(
        [
            "--data-dir",
            "unused-data",
            "Series",
            "Chinese.srt",
            "Start.mkv",
        ]
        .map(str::to_owned),
        &root,
    );
    assert_eq!(inputs.len(), 3);
    let batch = store.playback_files(&inputs, true)?;
    assert_eq!(batch.items.len(), 3);
    assert_eq!(batch.items[0].title, "Episode2");
    assert_eq!(batch.items[1].title, "Episode10");
    assert_eq!(
        batch.open_path,
        Some(root.join("Start.mkv").to_string_lossy().into_owned())
    );
    assert_eq!(
        batch.subtitles,
        [root.join("Chinese.srt").to_string_lossy().into_owned()]
    );
    let mut playback = PlaybackState::default();
    playback.preferences.muted = true;
    store.save_playback_state(&playback)?;
    std::fs::write(
        root.join("files.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "paths": inputs, "playlist": batch.items, "subtitles": batch.subtitles, "openPath": batch.open_path,
            "dataDirectory": root.join("Data"), "muted": true
        }))?,
    )?;
    println!(
        "Normal file planning passed; isolated desktop fixtures: {}",
        root.display()
    );
    Ok(())
}
