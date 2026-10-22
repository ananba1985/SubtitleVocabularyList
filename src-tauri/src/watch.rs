//! Playback's local data adapter. Uses the same examples, translations and collection transaction.
use crate::{
    corpus::CandidateExample,
    error::AppError,
    media::ImportedMedia,
    store::Store,
    vocabulary::{self, CollectionInput, CollectionResult},
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WatchContext {
    pub revision: u64,
    pub source_id: String,
    pub example_id: String,
    pub text: String,
    pub translation: String,
    pub translation_revision: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text_source: String,
    pub previous: bool,
    pub location_key: String,
    pub media_sha256: String,
    pub audio_stream: Option<usize>,
    pub precise_timing: bool,
    pub subtitle_start_ms: i64,
    pub subtitle_end_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WatchProgress {
    pub source_id: Option<String>,
    pub path: String,
    pub position: f64,
    pub subtitle_id: Option<i64>,
    pub audio_id: Option<i64>,
    pub subtitle_delay: f64,
}

pub struct WatchMedia {
    pub source_id: Option<String>,
    pub path: PathBuf,
    pub title: String,
    pub subtitle_stream: Option<usize>,
    pub audio_stream: Option<usize>,
    pub text_source: String,
    pub examples: Vec<CandidateExample>,
    pub subtitles: Option<crate::subtitles::SubtitleBundle>,
    pub media_sha256: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct WatchClip {
    pub path: String,
    pub stream: Option<usize>,
}

impl Store {
    pub fn watch_media(
        &self,
        path: Option<&str>,
        source_id: Option<&str>,
    ) -> Result<WatchMedia, AppError> {
        let record: Option<(String, String, String, String, Option<String>, String)> = {
            let db = self.connection()?;
            if let Some(id) = source_id {
                db.query_row("SELECT id,path_hint,title,fingerprint,corpus_path,COALESCE(text_source,'') FROM sources WHERE id=? AND kind='video'", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?
            } else if let Some(path) = path {
                db.query_row("SELECT id,path_hint,title,fingerprint,corpus_path,COALESCE(text_source,'') FROM sources WHERE kind='video' AND path_hint=? ORDER BY imported_at DESC LIMIT 1", [path], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?
            } else {
                None
            }
        };
        if source_id.is_some() && record.is_none() {
            return Err(AppError::new("not_found", "未找到该剧集，请重新选择。"));
        }
        let file = record
            .as_ref()
            .map(|r| r.1.as_str())
            .or(path)
            .ok_or_else(|| AppError::new("invalid_input", "请先选择本地视频。"))?;
        let file = dunce::canonicalize(file).map_err(|_| {
            AppError::new(
                "resource_missing",
                "视频文件已移动或不可用，请重新选择原视频。",
            )
        })?;
        if !file.is_file() {
            return Err(AppError::new("invalid_input", "请选择视频文件。"));
        }
        let media_sha256 = crate::corpus::file_hash(&file)?;
        if let Some((id, _, title, fingerprint, corpus, text_source)) = record {
            let subtitles = self.subtitle_bundle(&id)?;
            let expected = subtitles
                .as_ref()
                .map(|s| s.media_sha256.as_str())
                .unwrap_or(&fingerprint);
            if media_sha256 != expected {
                return Err(AppError::new(
                    "conflict",
                    "视频内容与导入资料不一致，请重新导入后再使用原句译文。",
                ));
            }
            let imported = corpus
                .and_then(|p| std::fs::File::open(p).ok())
                .and_then(|f| serde_json::from_reader::<_, ImportedMedia>(f).ok());
            let examples = {
                let db = self.connection()?;
                let mut q = db.prepare("SELECT id,text,start_ms,end_ms,revision FROM examples WHERE source_id=? AND archived=0 AND start_ms IS NOT NULL AND end_ms IS NOT NULL ORDER BY start_ms,end_ms,id")?;
                q.query_map([&id], |r| {
                    Ok(CandidateExample {
                        id: r.get(0)?,
                        text: r.get(1)?,
                        start_ms: r.get(2)?,
                        end_ms: r.get(3)?,
                        revision: r.get(4)?,
                        decision: None,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
            };
            Ok(WatchMedia {
                source_id: Some(id),
                path: file,
                title,
                subtitle_stream: imported.as_ref().and_then(|v| v.subtitle_stream),
                audio_stream: imported.as_ref().map(|v| v.audio_stream),
                text_source,
                examples,
                subtitles,
                media_sha256,
            })
        } else {
            Ok(WatchMedia {
                source_id: None,
                title: file
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path: file,
                subtitle_stream: None,
                audio_stream: None,
                text_source: "subtitle".into(),
                examples: vec![],
                subtitles: None,
                media_sha256,
            })
        }
    }

    pub fn watch_context(
        &self,
        media: &WatchMedia,
        position_ms: i64,
        subtitle_delay_ms: i64,
        revision: u64,
    ) -> Result<Option<WatchContext>, AppError> {
        let subtitle_time = position_ms - subtitle_delay_ms;
        let example = media
            .examples
            .iter()
            .find(|e| e.start_ms <= subtitle_time && subtitle_time < e.end_ms)
            .or_else(|| {
                media
                    .examples
                    .iter()
                    .rev()
                    .find(|e| e.end_ms <= subtitle_time && subtitle_time - e.end_ms <= 3000)
            });
        let Some(e) = example else { return Ok(None) };
        let translation = media
            .subtitles
            .as_ref()
            .and_then(|b| b.translation(e.start_ms, e.end_ms))
            .or(self.sentence_translation(&e.text)?)
            .unwrap_or_default();
        Ok(Some(WatchContext {
            revision,
            source_id: media.source_id.clone().unwrap_or_default(),
            example_id: e.id.clone(),
            text: e.text.clone(),
            translation_revision: vocabulary::digest(translation.as_bytes()),
            translation,
            start_ms: e.start_ms,
            end_ms: e.end_ms,
            text_source: media.text_source.clone(),
            previous: subtitle_time >= e.end_ms,
            location_key: String::new(),
            media_sha256: media.media_sha256.clone(),
            audio_stream: media.audio_stream,
            precise_timing: true,
            subtitle_start_ms: e.start_ms,
            subtitle_end_ms: e.end_ms,
        }))
    }

    /// Current decoder text is authoritative; importing the whole episode is optional.
    pub fn watch_text_context(
        &self,
        media: &WatchMedia,
        raw: &str,
        start_ms: i64,
        end_ms: i64,
        revision: u64,
        track: &str,
        audio_stream: Option<usize>,
        alignment: Option<(i64, i64)>,
    ) -> Result<Option<WatchContext>, AppError> {
        let Some(text) = crate::subtitles::english_text(raw) else {
            return Ok(None);
        };
        if text.chars().count() > 4000 || start_ms < 0 || end_ms <= start_ms {
            return Ok(None);
        }
        let indexed = media.examples.iter().find(|e| {
            e.text == text && (e.start_ms - start_ms).abs() < 80 && (e.end_ms - end_ms).abs() < 80
        });
        let (cue_start, cue_end) = alignment.unwrap_or((start_ms, end_ms));
        let translation = media
            .subtitles
            .as_ref()
            .and_then(|b| b.translation(cue_start, cue_end))
            .or(self.sentence_translation(&text)?)
            .or_else(|| crate::subtitles::chinese_text(raw))
            .unwrap_or_default();
        Ok(Some(WatchContext {
            revision,
            source_id: media.source_id.clone().unwrap_or_default(),
            example_id: indexed.map(|e| e.id.clone()).unwrap_or_default(),
            location_key: format!(
                "watch:{track}:{start_ms}:{end_ms}:{}",
                vocabulary::digest(text.as_bytes())
            ),
            media_sha256: media.media_sha256.clone(),
            audio_stream,
            precise_timing: true,
            subtitle_start_ms: cue_start,
            subtitle_end_ms: cue_end,
            text,
            translation_revision: vocabulary::digest(translation.as_bytes()),
            translation,
            start_ms: indexed.map(|e| e.start_ms).unwrap_or(start_ms),
            end_ms: indexed.map(|e| e.end_ms).unwrap_or(end_ms),
            text_source: "player_text".into(),
            previous: false,
        }))
    }

    /// Called only for an explicit collection draft or an accepted automatic save.
    pub fn bind_watch_context(
        &self,
        path: &str,
        title: &str,
        context: &WatchContext,
    ) -> Result<WatchContext, AppError> {
        if !context.source_id.is_empty() && !context.example_id.is_empty() {
            self.save_watch_clip(context, path)?;
            return Ok(context.clone());
        }
        if crate::corpus::file_hash(std::path::Path::new(path))? != context.media_sha256 {
            return Err(AppError::new(
                "conflict",
                "视频内容已变化，请重新打开后收录。",
            ));
        }
        let mut bound = context.clone();
        if bound.source_id.is_empty() {
            bound.source_id = self.add_source(&vocabulary::SourceInput {
                kind: "video".into(),
                title: title.into(),
                fingerprint: context.media_sha256.clone(),
                path_hint: Some(path.into()),
                duration_ms: None,
            })?;
        }
        let db = self.connection()?;
        bound.example_id = vocabulary::insert_example(
            &db,
            &vocabulary::ExampleInput {
                text: context.text.clone(),
                source_id: Some(bound.source_id.clone()),
                location_key: context.location_key.clone(),
                start_ms: context.precise_timing.then_some(context.start_ms),
                end_ms: context.precise_timing.then_some(context.end_ms),
                ..Default::default()
            },
            chrono::Utc::now().timestamp_millis(),
        )?;
        db.execute("UPDATE sources SET audio_path=COALESCE(audio_path,?),text_source=COALESCE(text_source,?) WHERE id=?",
            rusqlite::params![path,context.text_source,bound.source_id])?;
        drop(db);
        self.save_watch_clip(&bound, path)?;
        if !context.translation.is_empty() {
            self.save_sentence_translation(
                &context.text,
                &context.translation,
                "subtitle://player",
                "播放器字幕",
            )?;
        }
        Ok(bound)
    }

    fn save_watch_clip(&self, context: &WatchContext, path: &str) -> Result<(), AppError> {
        if context.precise_timing {
            self.connection()?.execute("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",
                rusqlite::params![format!("watch:clip:{}", context.example_id), serde_json::to_string(&WatchClip { path: path.into(), stream: context.audio_stream })?])?;
        }
        Ok(())
    }

    pub fn watch_progress(&self) -> Result<Option<WatchProgress>, AppError> {
        let value: Option<String> = self
            .connection()?
            .query_row(
                "SELECT value_json FROM settings WHERE key='watch:progress'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        value.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
    }
    pub fn save_watch_progress(&self, value: &WatchProgress) -> Result<(), AppError> {
        self.connection()?.execute("INSERT INTO settings(key,value_json) VALUES ('watch:progress',?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",[serde_json::to_string(value)?])?;
        Ok(())
    }

    pub fn watch_receipt(
        &self,
        context: &WatchContext,
    ) -> Result<Option<CollectionResult>, AppError> {
        let value: Option<String> = self
            .connection()?
            .query_row(
                "SELECT result_json FROM collection_actions WHERE operation_id=?",
                [operation_id(context)],
                |r| r.get(0),
            )
            .optional()?;
        value.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
    }

    pub fn watch_collect(
        &self,
        context: &WatchContext,
        asset: Option<&str>,
    ) -> Result<CollectionResult, AppError> {
        if context.translation.trim().is_empty() {
            return Err(AppError::new(
                "invalid_data",
                "没有已保存的完整译文，未自动收录。",
            ));
        }
        let mut example = if context.precise_timing {
            crate::application::example_input_from_corpus(
                self,
                &context.source_id,
                &context.example_id,
                &context.translation,
            )?
        } else {
            let (text, location_key) = self.connection()?.query_row(
                "SELECT text,location_key FROM examples WHERE id=? AND source_id=?",
                rusqlite::params![context.example_id, context.source_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?;
            vocabulary::ExampleInput {
                text,
                location_key,
                context_meaning: context.translation.clone(),
                source_id: Some(context.source_id.clone()),
                ..Default::default()
            }
        };
        let translation = self
            .subtitle_bundle(&context.source_id)?
            .filter(|_| context.precise_timing)
            .and_then(|b| b.translation(context.subtitle_start_ms, context.subtitle_end_ms))
            .or(self.sentence_translation(&context.text)?);
        if example.text != context.text
            || translation.as_deref() != Some(context.translation.as_str())
        {
            return Err(AppError::new(
                "conflict",
                "原句或译文已更新，请重新暂停查看。",
            ));
        }
        if let Some(id) = asset {
            example.media_asset_ids.push(id.into());
        }
        let matches = self.find_entries("sentence", &context.text)?;
        if matches.len() > 1 {
            return Err(AppError::new(
                "conflict",
                "存在多个同句词条，请手动选择合并目标。",
            ));
        }
        let input = CollectionInput {
            operation_id: operation_id(context),
            kind: "sentence".into(),
            text: context.text.clone(),
            meaning: context.translation.clone(),
            examples: vec![example],
            target_entry_id: matches.first().map(|v| v.id.clone()),
            expected_revision: matches.first().map(|v| v.revision),
        };
        let mut db = self.connection()?;
        let tx = db.transaction()?;
        // An already accepted automatic action remains idempotent even after an entry revision changes.
        let receipt: Option<String> = tx
            .query_row(
                "SELECT result_json FROM collection_actions WHERE operation_id=?",
                [&input.operation_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(receipt) = receipt {
            return Ok(serde_json::from_str(&receipt)?);
        }
        let result = vocabulary::collect_auto_on(self.root(), &tx, &input)?;
        tx.commit()?;
        Ok(result)
    }
}

fn operation_id(context: &WatchContext) -> String {
    let key = vocabulary::digest(
        format!(
            "watch-pause-v1:{}:{}:{}",
            context.source_id,
            context.example_id,
            vocabulary::normalize(&context.text)
        )
        .as_bytes(),
    );
    format!(
        "{}-{}-{}-{}-{}",
        &key[..8],
        &key[8..12],
        &key[12..16],
        &key[16..20],
        &key[20..32]
    )
}

/// The clock is armed by visible, complete translation, never by pause or request arrival.
#[derive(Default)]
pub struct DisplayClock {
    visible_at: Option<Instant>,
    revision: u64,
}
impl DisplayClock {
    pub fn arm(&mut self, revision: u64, now: Instant) {
        if self.visible_at.is_none() {
            self.revision = revision;
            self.visible_at = Some(now);
        }
    }
    pub fn cancel(&mut self) {
        self.visible_at = None;
    }
    pub fn remaining(&self, revision: u64, now: Instant, seconds: u32) -> Option<f64> {
        (revision == self.revision)
            .then_some(self.visible_at)
            .flatten()
            .map(|at| {
                (Duration::from_secs(seconds as u64).as_secs_f64()
                    - now.saturating_duration_since(at).as_secs_f64())
                .max(0.0)
            })
    }
}
