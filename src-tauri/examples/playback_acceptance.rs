//! One normal local queue/preferences persistence path. No GUI or sound.
use std::path::PathBuf;
use subtitle_vocabulary_list_core::{
    playback::{PlaybackPreferences, PlaybackState},
    store::Store,
    watch::WatchProgress,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("new acceptance directory required")?);
    let fixture = PathBuf::from(args.next().ok_or("silent video fixture required")?);
    if root.exists() {
        return Err("Use a new named acceptance directory; existing data is preserved".into());
    }
    std::fs::create_dir_all(root.join("videos"))?;
    for name in ["Episode10.mkv", "Episode2.mkv"] {
        std::fs::copy(&fixture, root.join("videos").join(name))?;
    }
    let data = root.join("Data");
    let store = Store::open(&data)?;
    let queue = store.playlist_items(&[root.join("videos").to_string_lossy().into_owned()])?;
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].title, "Episode2");
    assert_eq!(queue[1].title, "Episode10");
    let state = PlaybackState {
        preferences: PlaybackPreferences {
            speed: 1.25,
            volume: 32.0,
            muted: true,
            auto_next: true,
            subtitle_scale: 1.25,
            subtitle_position: 90.0,
            loop_mode: "file".into(),
            ..Default::default()
        },
        current_path: Some(queue[1].path.clone()),
        playlist: queue,
    };
    store.save_playback_state(&state)?;
    for (index, item) in state.playlist.iter().enumerate() {
        let media = store.watch_media(Some(&item.path), None)?;
        assert_eq!(media.path.to_string_lossy(), item.path);
        store.save_watch_progress(&WatchProgress {
            path: item.path.clone(),
            position: 2.0 + index as f64,
            audio_id: Some(1),
            subtitle_id: Some(1),
            subtitle_delay: 0.2,
            ..Default::default()
        })?;
    }
    drop(store);
    let store = Store::open(&data)?;
    assert_eq!(store.playback_state()?, state);
    assert_eq!(
        subtitle_vocabulary_list_core::playback::parse_time("00:01:30.5")?,
        90.5
    );
    for (index, item) in state.playlist.iter().enumerate() {
        let progress = store
            .watch_progress_for(&item.path)?
            .ok_or("missing per-file progress")?;
        assert_eq!(progress.position, 2.0 + index as f64);
        assert_eq!(progress.audio_id, Some(1));
        assert_eq!(progress.subtitle_delay, 0.2);
    }
    std::fs::write(
        root.join("persistence.json"),
        serde_json::to_vec_pretty(&state)?,
    )?;
    println!(
        "PASS: naturally sorted queue, per-file progress/tracks, preferences and current item survive reopening. {}",
        root.display()
    );
    Ok(())
}
