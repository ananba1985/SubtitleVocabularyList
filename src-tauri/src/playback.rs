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
}
impl Default for PlaybackPreferences {
    fn default() -> Self {
        Self {
            speed: 1.0,
            volume: 100.0,
            muted: false,
            auto_next: true,
        }
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
impl Store {
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
        let db = self.connection()?;
        files.into_iter().map(|path| {
            let path = path.to_string_lossy().into_owned();
            let source: Option<(String,String)> = db.query_row("SELECT id,title FROM sources WHERE kind='video' AND path_hint=? ORDER BY imported_at DESC LIMIT 1", [&path], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
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
