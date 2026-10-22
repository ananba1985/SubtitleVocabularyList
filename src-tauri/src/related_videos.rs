//! Filename-only discovery; importing the other episodes requires an explicit choice.
use crate::{error::AppError, playback, store::Store};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::OnceLock};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RelatedVideos {
    pub key: String,
    pub series: String,
    pub directory: String,
    pub total: usize,
    pub paths: Vec<String>,
}

fn series_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let identified = crate::online_subtitles::identify(name);
    let title = if identified.season.is_some() {
        format!(
            "{}{}",
            identified.title,
            identified
                .year
                .map(|y| format!(" ({y})"))
                .unwrap_or_default()
        )
    } else {
        static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
        let patterns = PATTERNS.get_or_init(|| {
            [
                r"(?i)(?:^|[ ._-])e(?:p(?:isode)?)?[ ._-]*\d{1,3}(?:[ ._-]|$)",
                r"第[零一二三四五六七八九十两\d]+[集话]",
                r"[ ._-]\d{1,3}(?:[ ._-]|$)",
            ]
            .iter()
            .map(|s| Regex::new(s).unwrap())
            .collect()
        });
        let stem = path.file_stem()?.to_str()?;
        let marker = patterns.iter().find_map(|p| p.find(stem))?;
        stem[..marker.start()].to_owned()
    };
    let title = title.nfkc().collect::<String>().replace(['.', '_'], " ");
    let title = title
        .trim_matches([' ', '-', '[', '(', ']'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()).then_some(title)
}

impl Store {
    pub fn related_videos(&self, path: &str) -> Result<Option<RelatedVideos>, AppError> {
        let current = dunce::canonicalize(path)?;
        let Some(series) = series_name(&current) else {
            return Ok(None);
        };
        let directory = current
            .parent()
            .ok_or_else(|| AppError::new("invalid_input", "视频目录不可用。"))?;
        let mut files = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            let candidate = entry?.path();
            if playback::video_file(&candidate)
                && series_name(&candidate)
                    .is_some_and(|name| name.to_lowercase() == series.to_lowercase())
            {
                files.push(candidate);
            }
        }
        files.sort_by(|a, b| playback::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
        let total = files.len();
        let mut paths = Vec::new();
        for file in files {
            if file == current {
                continue;
            }
            let source = self.video_source_at(&file)?;
            if !self.watch_import_ready(source.as_ref().map(|s| s.0.as_str()))? {
                paths.push(file.to_string_lossy().into_owned());
            }
        }
        if paths.is_empty() {
            return Ok(None);
        }
        Ok(Some(RelatedVideos {
            key: crate::vocabulary::digest(
                format!("{}\n{}", directory.display(), series.to_lowercase()).as_bytes(),
            ),
            series,
            directory: directory.to_string_lossy().into_owned(),
            total,
            paths,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_sibling_episodes_without_importing() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "Synthetic.Series.S01E01.mkv",
            "Synthetic.Series.S01E10.mp4",
            "Synthetic.Series.S01E02.mkv",
        ] {
            std::fs::write(root.path().join(name), b"synthetic filename fixture").unwrap();
        }
        let store = Store::open(root.path().join("Data")).unwrap();
        let result = store
            .related_videos(
                root.path()
                    .join("Synthetic.Series.S01E01.mkv")
                    .to_str()
                    .unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(result.series, "Synthetic Series");
        assert_eq!(result.total, 3);
        assert_eq!(
            result
                .paths
                .iter()
                .map(|p| Path::new(p).file_name().unwrap().to_str().unwrap())
                .collect::<Vec<_>>(),
            ["Synthetic.Series.S01E02.mkv", "Synthetic.Series.S01E10.mp4"]
        );
        assert!(store.sources().unwrap().is_empty());
        let tasks: i64 = store
            .connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tasks, 0);
    }
}
