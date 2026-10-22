//! Local bilingual subtitle assets. Originals stay separate from the learning timeline.
use crate::{
    error::AppError,
    media::{self, MediaTools, Segment},
    store::Store,
    vocabulary,
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleAsset {
    pub id: String,
    pub language: String,
    pub title: String,
    pub origin: String,
    pub stream_index: Option<usize>,
    pub format: String,
    pub relative_path: String,
    pub sha256: String,
    pub cues: Vec<Segment>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleBundle {
    pub media_sha256: String,
    pub selected_chinese_id: Option<String>,
    pub assets: Vec<SubtitleAsset>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrackInfo {
    pub id: String,
    pub language: String,
    pub title: String,
    pub origin: String,
    pub stream_index: Option<usize>,
    pub format: String,
    pub relative_path: String,
    pub sha256: String,
    pub cue_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SubtitleInfo {
    pub media_sha256: String,
    pub selected_chinese_id: Option<String>,
    pub assets: Vec<SubtitleTrackInfo>,
    pub warnings: Vec<String>,
}
impl SubtitleBundle {
    pub fn info(&self) -> SubtitleInfo {
        SubtitleInfo {
            media_sha256: self.media_sha256.clone(),
            selected_chinese_id: self.selected_chinese_id.clone(),
            warnings: self.warnings.clone(),
            assets: self
                .assets
                .iter()
                .map(|a| SubtitleTrackInfo {
                    id: a.id.clone(),
                    language: a.language.clone(),
                    title: a.title.clone(),
                    origin: a.origin.clone(),
                    stream_index: a.stream_index,
                    format: a.format.clone(),
                    relative_path: a.relative_path.clone(),
                    sha256: a.sha256.clone(),
                    cue_count: a.cues.len(),
                })
                .collect(),
        }
    }
    /// Temporal overlap permits different segmentation; never zip subtitle rows.
    pub fn translation(&self, start: i64, end: i64) -> Option<String> {
        if end <= start {
            return None;
        }
        let asset = self
            .assets
            .iter()
            .find(|a| Some(&a.id) == self.selected_chinese_id.as_ref())?;
        let mut covered = 0;
        let mut last = start;
        let mut lines = Vec::new();
        for cue in &asset.cues {
            let left = cue.start_ms.max(start);
            let right = cue.end_ms.min(end);
            if right <= left {
                continue;
            }
            // Container/audio offsets can leave a few milliseconds of the next
            // cue overlapping. Such boundary slivers are not another sentence.
            let shorter = (end - start).min(cue.end_ms - cue.start_ms);
            let required = (shorter / 5).max(80).min(shorter);
            if right - left < required {
                continue;
            }
            covered += (right - left.max(last)).max(0);
            last = last.max(right);
            if let Some(text) = chinese_text(&cue.text) {
                if lines.last() != Some(&text) {
                    lines.push(text);
                }
            }
        }
        if covered as f64 / ((end - start) as f64) < 0.65 || lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }
}
fn han(c: char) -> bool {
    ('\u{3400}'..='\u{9fff}').contains(&c)
}
pub fn english_text(text: &str) -> Option<String> {
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.chars().any(han) && line.chars().any(|c| c.is_ascii_alphabetic()))
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}
pub(crate) fn chinese_text(text: &str) -> Option<String> {
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| line.chars().any(han))
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}
fn language(label: &str) -> &'static str {
    let l = label
        .to_lowercase()
        .replace(['_', '-', '&', '+', '.', ' '], ":");
    let words: Vec<_> = l.split(':').collect();
    let zh = words.iter().any(|s| {
        matches!(
            *s,
            "zh" | "zho" | "chi" | "chs" | "cht" | "chinese" | "hans" | "hant"
        )
    }) || l.contains("中文")
        || l.contains("简体")
        || l.contains("繁体")
        || l.contains("双语");
    let en = words.iter().any(|s| matches!(*s, "en" | "eng" | "english"))
        || l.contains("英文")
        || l.contains("英语")
        || l.contains("双语");
    match (en, zh) {
        (true, true) => "en-zh",
        (true, false) => "en",
        (false, true) => "zh",
        _ => "und",
    }
}
fn cancelled(flag: &AtomicBool) -> Result<(), AppError> {
    if flag.load(Ordering::Relaxed) {
        Err(AppError::new("cancelled", "任务已取消。"))
    } else {
        Ok(())
    }
}
fn text_codec(codec: &str) -> Option<(&'static str, &'static str)> {
    match codec {
        "ass" | "ssa" => Some(("ass", "copy")),
        "subrip" => Some(("srt", "copy")),
        "webvtt" => Some(("vtt", "copy")),
        "mov_text" | "text" => Some(("srt", "srt")),
        _ => None,
    }
}

/// Does not install a corpus or collect words. Saves both languages under Data.
pub fn prepare_local(
    tools: &MediaTools,
    input: &Path,
    root: &Path,
    media_hash: &str,
    selected_external: Option<&Path>,
    flag: &AtomicBool,
    progress: &dyn Fn(&str, usize, usize),
) -> Result<SubtitleBundle, AppError> {
    let hash = media_hash.to_owned();
    let directory = root.join("subtitles").join(&hash);
    std::fs::create_dir_all(&directory)?;
    let probe = media::probe(tools, input, flag)?;
    let tracks: Vec<_> = probe
        .streams
        .iter()
        .filter(|s| s.codec_type == "subtitle")
        .collect();
    let mut bundle = SubtitleBundle {
        media_sha256: hash.clone(),
        selected_chinese_id: None,
        assets: vec![],
        warnings: vec![],
    };
    for (n, track) in tracks.iter().enumerate() {
        cancelled(flag)?;
        progress("subtitles", n, tracks.len());
        let title = format!(
            "{} {}",
            track.tags.get("language").map(String::as_str).unwrap_or(""),
            track.tags.get("title").map(String::as_str).unwrap_or("")
        )
        .trim()
        .to_owned();
        let lang = language(&title);
        let Some((extension, codec)) = text_codec(&track.codec_name) else {
            if matches!(
                track.codec_name.as_str(),
                "hdmv_pgs_subtitle" | "dvd_subtitle" | "dvb_subtitle"
            ) {
                let extension = if track.codec_name == "hdmv_pgs_subtitle" {
                    "sup"
                } else {
                    "mks"
                };
                let path = directory.join(format!("track-{}.{}", track.index, extension));
                let result = media::run_tool(
                    &tools.ffmpeg,
                    &[
                        "-nostdin".into(),
                        "-y".into(),
                        "-v".into(),
                        "error".into(),
                        "-i".into(),
                        input.to_string_lossy().into_owned(),
                        "-map".into(),
                        format!("0:{}", track.index),
                        "-c:s".into(),
                        "copy".into(),
                        path.to_string_lossy().into_owned(),
                    ],
                    flag,
                );
                match result {
                    Ok(_) => {
                        let sha256 = crate::corpus::file_hash(&path)?;
                        bundle.assets.push(SubtitleAsset {
                            id: vocabulary::digest(
                                format!("embedded:{}:{sha256}", track.index).as_bytes(),
                            ),
                            language: lang.into(),
                            title: title.clone(),
                            origin: "embedded".into(),
                            stream_index: Some(track.index),
                            format: track.codec_name.clone(),
                            relative_path: path
                                .strip_prefix(root)
                                .unwrap()
                                .to_string_lossy()
                                .replace('\\', "/"),
                            sha256,
                            cues: vec![],
                        });
                        bundle.warnings.push(format!(
                            "图片字幕轨 {} 已保留原资产；未取得文字，播放暂停时可 OCR 核对。",
                            track.index
                        ));
                    }
                    Err(e) if e.code == "cancelled" => return Err(e),
                    Err(e) => bundle
                        .warnings
                        .push(format!("图片字幕轨 {}：{}", track.index, e.message)),
                }
                continue;
            }
            bundle.warnings.push(format!(
                "字幕轨 {}（{}）保留在原视频中，尚未取得可对齐文字。",
                track.index, track.codec_name
            ));
            continue;
        };
        let path = directory.join(format!("track-{}.{}", track.index, extension));
        let result = (|| {
            media::run_tool(
                &tools.ffmpeg,
                &[
                    "-nostdin".into(),
                    "-y".into(),
                    "-v".into(),
                    "error".into(),
                    "-i".into(),
                    input.to_string_lossy().into_owned(),
                    "-map".into(),
                    format!("0:{}", track.index),
                    "-c:s".into(),
                    codec.into(),
                    path.to_string_lossy().into_owned(),
                ],
                flag,
            )?;
            asset(
                tools,
                root,
                &path,
                lang,
                &title,
                "embedded",
                Some(track.index),
                &track.codec_name,
                flag,
            )
        })();
        match result {
            Ok(a) => bundle.assets.push(a),
            Err(e) if e.code != "cancelled" => bundle
                .warnings
                .push(format!("字幕轨 {}：{}", track.index, e.message)),
            Err(e) => return Err(e),
        }
    }
    // Restrict siblings to the exact movie stem plus a language suffix.
    if let (Some(parent), Some(stem)) = (input.parent(), input.file_stem()) {
        let stem = stem.to_string_lossy();
        let mut paths: Vec<PathBuf> = std::fs::read_dir(parent)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                let ext = p
                    .extension()
                    .map(|v| v.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !matches!(ext.as_str(), "srt" | "ass" | "ssa" | "vtt") {
                    return false;
                }
                let name = p.file_stem().unwrap_or_default().to_string_lossy();
                name.eq_ignore_ascii_case(&stem)
                    || name
                        .get(..stem.len())
                        .is_some_and(|s| s.eq_ignore_ascii_case(&stem))
                        && name
                            .get(stem.len()..)
                            .is_some_and(|s| s.starts_with('.') && language(s) != "und")
            })
            .collect();
        if let Some(path) = selected_external {
            if path.is_file() && !paths.iter().any(|p| p == path) {
                paths.push(path.to_path_buf());
            }
        }
        paths.sort();
        for path in paths {
            cancelled(flag)?;
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if std::fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
                bundle
                    .warnings
                    .push(format!("{name} 超过 16 MiB，未读取。"));
                continue;
            }
            let digest = crate::corpus::file_hash(&path)?;
            let target = directory.join(format!(
                "external-{}.{}",
                &digest[..20],
                path.extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
            ));
            if !target.exists() {
                std::fs::copy(&path, &target)?;
            }
            match asset(
                tools,
                root,
                &target,
                language(&name),
                &name,
                "external",
                None,
                &path.extension().unwrap_or_default().to_string_lossy(),
                flag,
            ) {
                Ok(a) => bundle.assets.push(a),
                Err(e) if e.code != "cancelled" => {
                    bundle.warnings.push(format!("{name}：{}", e.message))
                }
                Err(e) => return Err(e),
            }
        }
    }
    let mut seen = HashSet::new();
    bundle
        .assets
        .retain(|a| seen.insert((a.sha256.clone(), a.language.clone())));
    bundle.selected_chinese_id = bundle
        .assets
        .iter()
        .find(|a| a.language == "zh" && !a.cues.is_empty() && a.title.contains("简体"))
        .or_else(|| {
            bundle
                .assets
                .iter()
                .find(|a| a.language == "zh" && !a.cues.is_empty())
        })
        .or_else(|| {
            bundle
                .assets
                .iter()
                .find(|a| a.language == "en-zh" && !a.cues.is_empty())
        })
        .map(|a| a.id.clone());
    progress("subtitles", tracks.len(), tracks.len());
    Ok(bundle)
}
fn asset(
    tools: &MediaTools,
    root: &Path,
    path: &Path,
    lang: &str,
    title: &str,
    origin: &str,
    index: Option<usize>,
    format: &str,
    flag: &AtomicBool,
) -> Result<SubtitleAsset, AppError> {
    let work = path
        .parent()
        .unwrap()
        .join(format!("parse-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&work)?;
    let result = media::external_segments(tools, path, &work, flag);
    // Only remove files created by external_segments; never recursively delete a computed path.
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let _ = std::fs::remove_file(work.join(format!("external-input.{extension}")));
    let _ = std::fs::remove_file(work.join("original-subtitles.srt"));
    let _ = std::fs::remove_dir(&work);
    let mut cues = result?;
    cues.sort_by_key(|c| (c.start_ms, c.end_ms));
    let zh = cues.iter().any(|c| c.text.chars().any(han));
    let en = cues.iter().any(|c| english_text(&c.text).is_some());
    let lang = if zh && en {
        "en-zh"
    } else if zh {
        "zh"
    } else {
        lang
    };
    let hash = crate::corpus::file_hash(path)?;
    Ok(SubtitleAsset {
        id: vocabulary::digest(format!("{origin}:{index:?}:{hash}").as_bytes()),
        language: lang.into(),
        title: title.into(),
        origin: origin.into(),
        stream_index: index,
        format: format.into(),
        relative_path: path
            .strip_prefix(root)
            .map_err(|_| AppError::new("invalid_data", "字幕不在资料目录中。"))?
            .to_string_lossy()
            .replace('\\', "/"),
        sha256: hash,
        cues,
    })
}

impl Store {
    pub fn subtitle_bundle(&self, source_id: &str) -> Result<Option<SubtitleBundle>, AppError> {
        let path: Option<String> = self
            .connection()?
            .query_row(
                "SELECT value_json FROM settings WHERE key=?",
                [format!("subtitles:{source_id}")],
                |r| r.get(0),
            )
            .optional()?;
        path.map(|p| {
            let relative: String = serde_json::from_str(&p)?;
            if !relative.starts_with("subtitles/")
                || Path::new(&relative)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(AppError::new("invalid_data", "字幕资源路径无效。"));
            }
            Ok(serde_json::from_reader(std::fs::File::open(
                self.root().join(relative),
            )?)?)
        })
        .transpose()
    }
    pub fn save_subtitle_bundle(
        &self,
        source_id: &str,
        bundle: &SubtitleBundle,
    ) -> Result<SubtitleInfo, AppError> {
        let mut bundle = bundle.clone();
        if let Some(old) = self.subtitle_bundle(source_id)? {
            if old.selected_chinese_id.is_none()
                || bundle
                    .assets
                    .iter()
                    .any(|a| Some(&a.id) == old.selected_chinese_id.as_ref())
            {
                bundle.selected_chinese_id = old.selected_chinese_id;
            }
        }
        let relative = format!(
            "subtitles/{}/bundle-{}.json",
            bundle.media_sha256,
            vocabulary::digest(source_id.as_bytes())
        );
        let path = self.root().join(&relative);
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(&bundle)?)?;
        std::fs::rename(temporary, path)?;
        self.connection()?.execute("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",params![format!("subtitles:{source_id}"),serde_json::to_string(&relative)?])?;
        // Fill missing sentence caches, keeping existing translations and all personal records.
        // Legacy cache column names are retained; subtitle:// identifies a subtitle, not an AI.
        let examples = {
            let db = self.connection()?;
            let mut q=db.prepare("SELECT text,start_ms,end_ms FROM examples WHERE source_id=? AND archived=0 AND start_ms IS NOT NULL AND end_ms IS NOT NULL")?;
            q.query_map([source_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        for (text, start, end) in examples {
            if let Some(translation) = bundle.translation(start, end) {
                if text.chars().count() > 4000 || translation.chars().count() > 16000 {
                    continue;
                }
                self.save_sentence_translation(
                    &text,
                    &translation,
                    &format!(
                        "subtitle://{}",
                        bundle.selected_chinese_id.as_deref().unwrap_or("")
                    ),
                    "中文字幕",
                )?;
            }
        }
        Ok(bundle.info())
    }
    pub fn select_chinese_subtitle(
        &self,
        source_id: &str,
        asset_id: Option<String>,
    ) -> Result<SubtitleInfo, AppError> {
        let mut bundle = self
            .subtitle_bundle(source_id)?
            .ok_or_else(|| AppError::new("not_found", "请先取得本地字幕。"))?;
        if let Some(id) = &asset_id {
            if !bundle.assets.iter().any(|a| {
                &a.id == id && !a.cues.is_empty() && matches!(a.language.as_str(), "zh" | "en-zh")
            }) {
                return Err(AppError::new("invalid_input", "请选择中文字幕。"));
            }
        }
        bundle.selected_chinese_id = asset_id;
        let relative = format!(
            "subtitles/{}/bundle-{}.json",
            bundle.media_sha256,
            vocabulary::digest(source_id.as_bytes())
        );
        let path = self.root().join(relative);
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(&bundle)?)?;
        std::fs::rename(temporary, path)?;
        Ok(bundle.info())
    }
    pub fn prepare_source_subtitles(
        &self,
        tools: &MediaTools,
        source_id: &str,
        flag: &AtomicBool,
        progress: &dyn Fn(&str, usize, usize),
    ) -> Result<SubtitleInfo, AppError> {
        let (input, fingerprint, corpus): (String, String, Option<String>) = self
            .connection()?
            .query_row(
                "SELECT path_hint,fingerprint,corpus_path FROM sources WHERE id=? AND kind='video'",
                [source_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::new("not_found", "未找到视频来源。"))?;
        let hash = crate::corpus::file_hash(Path::new(&input))?;
        let matches = if let Some(bundle) = self.subtitle_bundle(source_id)? {
            bundle.media_sha256 == hash
        } else {
            let imported = corpus
                .and_then(|p| std::fs::File::open(p).ok())
                .and_then(|f| serde_json::from_reader::<_, media::ImportedMedia>(f).ok());
            fingerprint == hash
                || imported.as_ref().is_some_and(|data| {
                    legacy_fingerprint_matches(&fingerprint, &hash, Path::new(&input), data)
                })
        };
        if !matches {
            return Err(AppError::new(
                "conflict",
                "视频与原导入版本无法核对，请重新导入原视频及字幕；现有资料保留。",
            ));
        }
        let bundle = prepare_local(
            tools,
            Path::new(&input),
            self.root(),
            &hash,
            None,
            flag,
            progress,
        )?;
        cancelled(flag)?;
        self.save_subtitle_bundle(source_id, &bundle)
    }
}

// Old explicit-track imports hashed their options together with the video. Verify
// those recorded choices instead of treating an options digest as a media digest.
fn legacy_fingerprint_matches(
    expected: &str,
    hash: &str,
    input: &Path,
    data: &media::ImportedMedia,
) -> bool {
    let external =
        media::find_external_subtitle(input).and_then(|p| crate::corpus::file_hash(&p).ok());
    for audio in [None, Some(data.audio_stream)] {
        for subtitle in [None, data.subtitle_stream] {
            for mode in ["auto", "embedded", "external", "speech"] {
                for ext in [None, external.as_deref()] {
                    if let Ok(value) =
                        serde_json::to_vec(&("video-options-v1", hash, audio, subtitle, mode, ext))
                    {
                        if vocabulary::digest(&value) == expected {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}
