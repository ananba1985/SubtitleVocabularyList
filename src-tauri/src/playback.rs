//! Local playback queue and preferences, stored separately from learning records.
use crate::{error::AppError, store::Store, watch::WatchProgress};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct PlaybackPreferences {
    pub speed: f64,
    pub volume: f64,
    pub muted: bool,
    pub auto_next: bool,
    pub subtitle_scale: f64,
    pub subtitle_position: f64,
    pub subtitle_visible: bool,
    pub secondary_subtitle_visible: bool,
    pub loop_mode: String,
}
impl Default for PlaybackPreferences {
    fn default() -> Self {
        Self {
            speed: 1.0,
            volume: 100.0,
            muted: false,
            auto_next: true,
            subtitle_scale: 1.0,
            subtitle_position: 100.0,
            subtitle_visible: true,
            secondary_subtitle_visible: true,
            loop_mode: "none".into(),
        }
    }
}

/// A timestamp from the native mpv input prompt; bare numbers mean seconds.
pub fn parse_time(text: &str) -> Result<f64, AppError> {
    let parts = text.trim().split(':').collect::<Vec<_>>();
    let invalid = || {
        AppError::new(
            "invalid_input",
            "请输入秒数或 时:分:秒，例如 90、01:30、00:01:30。",
        )
    };
    if parts.is_empty() || parts.len() > 3 {
        return Err(invalid());
    }
    let mut seconds = 0.0;
    for (index, part) in parts.iter().enumerate() {
        let number: f64 = part.parse().map_err(|_| invalid())?;
        if !number.is_finite()
            || number < 0.0
            || (index > 0 && number >= 60.0)
            || (index + 1 < parts.len() && number.fract() != 0.0)
        {
            return Err(invalid());
        }
        seconds = seconds * 60.0 + number;
    }
    if seconds.is_finite() {
        Ok(seconds)
    } else {
        Err(invalid())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItem {
    pub path: String,
    pub title: String,
    pub source_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct PlaybackState {
    pub preferences: PlaybackPreferences,
    pub playlist: Vec<PlaylistItem>,
    pub current_path: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlaylistChange {
    pub action: String,
    pub paths: Vec<String>,
    pub index: Option<usize>,
    pub destination: Option<usize>,
}

// Numeric filename ordering keeps E2 before E10, including unpadded episode names.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (start_a, start_b) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let (mut x, mut y) = (start_a, start_b);
            while x < i && a[x] == b'0' {
                x += 1;
            }
            while y < j && b[y] == b'0' {
                y += 1;
            }
            let order = (i - x).cmp(&(j - y)).then_with(|| a[x..i].cmp(&b[y..j]));
            if order != Ordering::Equal {
                return order;
            }
        } else {
            let order = a[i].cmp(&b[j]);
            if order != Ordering::Equal {
                return order;
            }
            i += 1;
            j += 1;
        }
    }
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}
pub fn video_file(path: &Path) -> bool {
    path.is_file()
        && path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "mkv"
                    | "mp4"
                    | "avi"
                    | "mov"
                    | "webm"
                    | "m4v"
                    | "ts"
                    | "mts"
                    | "m2ts"
                    | "wmv"
                    | "flv"
            )
        })
}

pub struct PlaybackFiles {
    pub items: Vec<PlaylistItem>,
    pub subtitles: Vec<String>,
    pub open_path: Option<String>,
}

/// Windows passes already-separated arguments, including paths with spaces.
pub fn file_arguments(args: impl IntoIterator<Item = String>, cwd: &Path) -> Vec<String> {
    let mut args = args.into_iter();
    let mut paths = vec![];
    let mut positional = false;
    while let Some(arg) = args.next() {
        if !positional && arg == "--data-dir" {
            args.next();
        } else if !positional && arg == "--" {
            positional = true;
        } else if positional || !arg.starts_with('-') {
            let path = Path::new(&arg);
            paths.push(
                if path.is_absolute() {
                    path.to_owned()
                } else {
                    cwd.join(path)
                }
                .to_string_lossy()
                .into_owned(),
            );
        }
    }
    paths
}

impl Store {
    pub fn playback_files(
        &self,
        paths: &[String],
        activate: bool,
    ) -> Result<PlaybackFiles, AppError> {
        let mut videos = vec![];
        let mut subtitles = vec![];
        let mut first_video = None;
        for path in paths {
            let path = dunce::canonicalize(path)
                .map_err(|e| AppError::new("resource_missing", e.to_string()))?;
            let text = path.to_string_lossy().into_owned();
            if path.is_dir() || video_file(&path) {
                if first_video.is_none() && video_file(&path) {
                    first_video = Some(text.clone());
                }
                videos.push(text);
            } else if path.is_file()
                && path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    matches!(
                        e.to_ascii_lowercase().as_str(),
                        "srt" | "ass" | "ssa" | "vtt" | "sup"
                    )
                })
            {
                if !subtitles.contains(&text) {
                    subtitles.push(text);
                }
            }
        }
        let items = self.playlist_items(&videos)?;
        if items.is_empty() && subtitles.is_empty() {
            return Err(AppError::new(
                "invalid_input",
                "没有可打开的视频或字幕。支持拖入视频、视频目录及 SRT／ASS／SSA／VTT／SUP 字幕。",
            ));
        }
        let open_path = activate
            .then(|| first_video.or_else(|| items.first().map(|i| i.path.clone())))
            .flatten();
        Ok(PlaybackFiles {
            items,
            subtitles,
            open_path,
        })
    }
    pub fn playback_state(&self) -> Result<PlaybackState, AppError> {
        let value: Option<String> = self
            .connection()?
            .query_row(
                "SELECT value_json FROM settings WHERE key='watch:playback'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(value
            .map(|v| serde_json::from_str(&v))
            .transpose()?
            .unwrap_or_default())
    }
    pub fn save_playback_state(&self, state: &PlaybackState) -> Result<(), AppError> {
        self.connection()?.execute("INSERT INTO settings(key,value_json) VALUES ('watch:playback',?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json", [serde_json::to_string(state)?])?;
        Ok(())
    }
    pub fn playlist_items(&self, paths: &[String]) -> Result<Vec<PlaylistItem>, AppError> {
        let mut files = vec![];
        for path in paths {
            let path = dunce::canonicalize(path)
                .map_err(|e| AppError::new("resource_missing", e.to_string()))?;
            if path.is_dir() {
                for entry in std::fs::read_dir(path)? {
                    let file = entry?.path();
                    if video_file(&file) {
                        files.push(file);
                    }
                }
            } else if video_file(&path) {
                files.push(path);
            } else {
                return Err(AppError::new("invalid_input", "请选择本地视频或视频目录。"));
            }
        }
        files.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
        files.dedup();
        files.into_iter().map(|path| {
            let source = self.video_source_at(&path)?;
            let path = path.to_string_lossy().into_owned();
            let title = source.as_ref().map(|s| s.1.clone()).unwrap_or_else(|| Path::new(&path).file_stem().unwrap_or_default().to_string_lossy().into_owned());
            Ok(PlaylistItem { path, title, source_id: source.map(|s| s.0) })
        }).collect()
    }
    pub fn watch_progress_for(&self, path: &str) -> Result<Option<WatchProgress>, AppError> {
        let key = format!(
            "watch:progress:{}",
            crate::vocabulary::digest(path.as_bytes())
        );
        let value: Option<String> = self
            .connection()?
            .query_row("SELECT value_json FROM settings WHERE key=?", [key], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(value) = value {
            return Ok(Some(serde_json::from_str(&value)?));
        }
        Ok(self.watch_progress()?.filter(|p| p.path == path))
    }
}
