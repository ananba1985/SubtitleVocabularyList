use crate::{
    error::AppError,
    media::{self, ImportedMedia, MediaTools},
    store::Store,
    vocabulary::{self, AudioAsset, ExampleInput, SourceInput},
};
use chrono::Utc;
use regex::Regex;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::Read,
    path::{Path, PathBuf},
    sync::{OnceLock, atomic::AtomicBool},
};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSummary {
    pub id: String,
    pub title: String,
    pub duration_ms: i64,
    pub text_source: String,
    pub example_count: i64,
    pub candidate_count: i64,
    pub imported_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateExample {
    pub id: String,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub decision: Option<String>,
    pub revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub key: String,
    pub text: String,
    pub kind: String,
    pub count: i64,
    pub handled_count: i64,
    pub example_count: i64,
    pub existing_entry_count: i64,
}

#[derive(Clone, Debug)]
struct Token {
    key: String,
    raw: String,
    kind: &'static str,
    start: usize,
    end: usize,
}

fn tokens(text: &str) -> Vec<Token> {
    static WORDS: OnceLock<Regex> = OnceLock::new();
    static ANNOTATIONS: OnceLock<Regex> = OnceLock::new();
    static PHRASES: OnceLock<Vec<Regex>> = OnceLock::new();
    let annotations =
        ANNOTATIONS.get_or_init(|| Regex::new(r"\[[^\]]*\]").expect("fixed annotation pattern"));
    let masked = annotations.replace_all(text, |capture: &regex::Captures<'_>| {
        " ".repeat(capture[0].len())
    });
    let words = WORDS
        .get_or_init(|| Regex::new(r"[A-Za-z]+(?:['’][A-Za-z]+)*").expect("fixed word pattern"));
    let mut result: Vec<_> = words
        .find_iter(&masked)
        .map(|matched| Token {
            key: vocabulary::normalize(matched.as_str()),
            raw: matched.as_str().to_owned(),
            kind: "word",
            start: matched.start(),
            end: matched.end(),
        })
        .collect();
    let phrases = PHRASES.get_or_init(|| {
        [
            "look up",
            "give up",
            "find out",
            "get along",
            "take off",
            "pick up",
            "come up",
            "come on",
            "get over",
            "go on",
            "put on",
            "take care",
            "make sure",
            "a lot",
            "in fact",
            "of course",
            "used to",
            "hang out",
            "get rid of",
            "fall in love",
            "grow up",
            "turn down",
            "turn out",
            "look after",
            "check out",
            "put up with",
            "no way",
            "as long as",
            "at least",
            "right now",
            "kind of",
            "sort of",
            "by the way",
            "wait for",
            "take a look",
            "what about",
            "how about",
        ]
        .iter()
        .map(|phrase| {
            Regex::new(&format!(
                r"(?i)\b{}\b",
                phrase
                    .split_whitespace()
                    .map(regex::escape)
                    .collect::<Vec<_>>()
                    .join(r"\s+")
            ))
            .expect("fixed phrase pattern")
        })
        .collect()
    });
    for phrase in phrases {
        for matched in phrase.find_iter(&masked) {
            result.push(Token {
                key: vocabulary::normalize(matched.as_str()),
                raw: matched.as_str().to_owned(),
                kind: "phrase",
                start: matched.start(),
                end: matched.end(),
            });
        }
    }
    result
}

pub fn file_hash(path: &Path) -> Result<String, AppError> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn video_inputs(paths: &[String]) -> Result<Vec<PathBuf>, AppError> {
    let extensions = ["mkv", "mp4", "mov", "webm", "avi", "m4v"];
    let supported = |path: &Path| {
        path.extension().is_some_and(|extension| {
            extensions.contains(&extension.to_string_lossy().to_lowercase().as_str())
        })
    };
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for input in paths {
        let path = PathBuf::from(input);
        if path.is_dir() {
            for item in walkdir::WalkDir::new(&path).follow_links(false).into_iter() {
                let item = item.map_err(|error| AppError::new("io_error", error.to_string()))?;
                if item.file_type().is_file() && supported(item.path()) {
                    let canonical = item.path().canonicalize()?;
                    if seen.insert(canonical.clone()) {
                        result.push(canonical);
                    }
                }
            }
        } else {
            // Keep explicit files so a bad file gets an independent failure result.
            if seen.insert(path.clone()) {
                result.push(path);
            }
        }
    }
    result.sort();
    if result.is_empty() {
        return Err(AppError::new(
            "invalid_input",
            "所选目录没有可导入的视频文件。",
        ));
    }
    Ok(result)
}

impl Store {
    pub fn install_corpus(
        &self,
        input_path: &Path,
        fingerprint: &str,
        imported: &ImportedMedia,
        corpus_path: &Path,
    ) -> Result<SourceSummary, AppError> {
        let id = self.add_source(&SourceInput {
            kind: "video".into(),
            title: imported.title.clone(),
            fingerprint: fingerprint.into(),
            path_hint: Some(input_path.to_string_lossy().to_string()),
            duration_ms: Some(imported.duration_ms),
        })?;
        let now = Utc::now().timestamp_millis();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        for segment in &imported.segments {
            let example = ExampleInput {
                text: segment.text.replace("\r\n", "\n").replace('\r', "\n"),
                source_id: Some(id.clone()),
                location_key: segment.location_key.clone(),
                start_ms: Some(segment.start_ms),
                end_ms: Some(segment.end_ms),
                ..Default::default()
            };
            let example_id = vocabulary::insert_example(&transaction, &example, now)?;
            let saved_text: String = transaction.query_row(
                "SELECT text FROM examples WHERE id=?",
                [&example_id],
                |row| row.get(0),
            )?;
            for token in tokens(&saved_text) {
                transaction.execute("INSERT OR IGNORE INTO occurrences(id,source_id,example_id,candidate_key,raw_text,token_start,token_end,kind) VALUES (?,?,?,?,?,?,?,?)",params![Uuid::new_v4().to_string(),id,example_id,token.key,token.raw,token.start as i64,token.end as i64,token.kind])?;
            }
        }
        transaction.execute("UPDATE sources SET corpus_path=?,audio_path=?,text_source=?,imported_at=?,duration_ms=? WHERE id=?",params![corpus_path.to_string_lossy(),imported.audio_path.to_string_lossy(),imported.text_source,now,imported.duration_ms,id])?;
        transaction.commit()?;
        drop(connection);
        self.source(&id)
    }

    pub fn source(&self, id: &str) -> Result<SourceSummary, AppError> {
        let connection = self.connection()?;
        connection.query_row("SELECT s.id,s.title,s.duration_ms,s.text_source,s.imported_at,(SELECT COUNT(*) FROM examples WHERE source_id=s.id AND archived=0),(SELECT COUNT(DISTINCT kind||':'||candidate_key) FROM occurrences WHERE source_id=s.id) FROM sources s WHERE s.id=? AND s.corpus_path IS NOT NULL",[id],|row|Ok(SourceSummary{id:row.get(0)?,title:row.get(1)?,duration_ms:row.get(2)?,text_source:row.get(3)?,imported_at:row.get(4)?,example_count:row.get(5)?,candidate_count:row.get(6)?})).optional()?.ok_or_else(||AppError::new("not_found","剧集预习资料不存在。"))
    }

    pub fn sources(&self) -> Result<Vec<SourceSummary>, AppError> {
        let ids = {
            let connection = self.connection()?;
            let mut query = connection.prepare(
                "SELECT id FROM sources WHERE corpus_path IS NOT NULL ORDER BY imported_at DESC,id",
            )?;
            query
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        ids.iter().map(|id| self.source(id)).collect()
    }

    pub fn cached_source(&self, fingerprint: &str) -> Result<Option<SourceSummary>, AppError> {
        let record: Option<(String, String, String)> = self.connection()?.query_row(
            "SELECT id,corpus_path,audio_path FROM sources WHERE kind='video' AND fingerprint=? AND corpus_path IS NOT NULL AND audio_path IS NOT NULL",
            [fingerprint], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).optional()?;
        if let Some((id, corpus, audio)) = record
            && [corpus, audio].iter().all(|path| {
                std::fs::metadata(path)
                    .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
            })
        {
            return self.source(&id).map(Some);
        }
        Ok(None)
    }

    pub fn candidates(
        &self,
        source_id: &str,
        search: &str,
        kind: &str,
        only_pending: bool,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Candidate>, AppError> {
        let connection = self.connection()?;
        let mut query=connection.prepare("SELECT o.candidate_key,MIN(o.raw_text),o.kind,COUNT(*),COUNT(DISTINCT o.example_id),(SELECT COUNT(*) FROM entries en WHERE en.kind=o.kind AND en.match_key=o.candidate_key),SUM(CASE WHEN d.decision IN ('familiar','collected') THEN 1 ELSE 0 END) FROM occurrences o LEFT JOIN candidate_decisions d ON d.source_id=o.source_id AND d.candidate_key=o.candidate_key AND d.scope_key=o.example_id WHERE o.source_id=? AND instr(o.candidate_key,?)>0 AND (?='' OR o.kind=?) GROUP BY o.candidate_key,o.kind HAVING (?=0 OR SUM(CASE WHEN d.decision IN ('familiar','collected') THEN 1 ELSE 0 END)<COUNT(*)) ORDER BY length(o.candidate_key) DESC,COUNT(*) DESC,o.candidate_key LIMIT ? OFFSET ?")?;
        Ok(query
            .query_map(
                params![
                    source_id,
                    vocabulary::normalize(search),
                    kind,
                    kind,
                    only_pending,
                    limit.clamp(1, 100),
                    offset
                ],
                |row| {
                    Ok(Candidate {
                        key: row.get(0)?,
                        text: row.get(1)?,
                        kind: row.get(2)?,
                        count: row.get(3)?,
                        example_count: row.get(4)?,
                        existing_entry_count: row.get(5)?,
                        handled_count: row.get(6)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn candidate_examples(
        &self,
        source_id: &str,
        key: &str,
    ) -> Result<Vec<CandidateExample>, AppError> {
        let connection = self.connection()?;
        let mut query=connection.prepare("SELECT DISTINCT e.id,e.text,e.start_ms,e.end_ms,e.revision,d.decision FROM occurrences o JOIN examples e ON e.id=o.example_id LEFT JOIN candidate_decisions d ON d.source_id=o.source_id AND d.candidate_key=o.candidate_key AND d.scope_key=o.example_id WHERE o.source_id=? AND o.candidate_key=? ORDER BY e.start_ms,e.id")?;
        Ok(query
            .query_map(params![source_id, key], |row| {
                Ok(CandidateExample {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    start_ms: row.get(2)?,
                    end_ms: row.get(3)?,
                    revision: row.get(4)?,
                    decision: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn decide_candidate(
        &self,
        source_id: &str,
        key: &str,
        example_id: &str,
        decision: &str,
    ) -> Result<(), AppError> {
        if !matches!(decision, "familiar" | "uncertain") {
            return Err(AppError::new("invalid_input", "候选判断无效。"));
        }
        let connection = self.connection()?;
        let found=connection.query_row("SELECT 1 FROM occurrences WHERE source_id=? AND candidate_key=? AND example_id=? LIMIT 1",params![source_id,key,example_id],|row|row.get::<_,i64>(0)).optional()?.is_some();
        if !found {
            return Err(AppError::new("not_found", "本次候选语境不存在。"));
        }
        connection.execute("INSERT INTO candidate_decisions(source_id,candidate_key,scope_key,decision,updated_at) VALUES (?,?,?,?,?) ON CONFLICT(source_id,candidate_key,scope_key) DO UPDATE SET decision=excluded.decision,updated_at=excluded.updated_at",params![source_id,key,example_id,decision,Utc::now().timestamp_millis()])?;
        Ok(())
    }

    pub fn source_example(&self, source_id: &str, id: &str) -> Result<CandidateExample, AppError> {
        self.connection()?
            .query_row(
                "SELECT id,text,start_ms,end_ms,revision FROM examples WHERE id=? AND source_id=?",
                params![id, source_id],
                |row| {
                    Ok(CandidateExample {
                        id: row.get(0)?,
                        text: row.get(1)?,
                        start_ms: row.get(2)?,
                        end_ms: row.get(3)?,
                        revision: row.get(4)?,
                        decision: None,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| AppError::new("not_found", "例句不存在。"))
    }

    pub fn update_source_example(
        &self,
        source_id: &str,
        id: &str,
        revision: i64,
        text: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<CandidateExample, AppError> {
        let source = self.source(source_id)?;
        if text.trim().is_empty()
            || start_ms < 0
            || end_ms <= start_ms
            || end_ms > source.duration_ms
        {
            return Err(AppError::new(
                "invalid_input",
                "请确认非空原句和有效时间范围。",
            ));
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let original:Option<(i64,i64,i64)>=transaction.query_row("SELECT start_ms,end_ms,revision FROM examples WHERE id=? AND source_id=? AND archived=0",params![id,source_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
        let (old_start, old_end, old_revision) =
            original.ok_or_else(|| AppError::new("not_found", "原始例句不存在。"))?;
        if old_revision != revision {
            return Err(AppError::new(
                "conflict",
                "例句已发生变化，请刷新后再修改。",
            ));
        }
        let collected: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM entry_examples WHERE example_id=?",
            [id],
            |row| row.get(0),
        )?;
        if collected > 0 {
            let affected = {
                let mut q = transaction
                    .prepare("SELECT DISTINCT entry_id FROM entry_examples WHERE example_id=?")?;
                q.query_map([id], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let archived_id = Uuid::new_v4().to_string();
            transaction.execute("INSERT INTO examples(id,source_id,location_key,identity_key,text,start_ms,end_ms,revision,created_at,archived) SELECT ?,source_id,location_key||':history:'||revision,identity_key||':history:'||revision,text,start_ms,end_ms,revision,created_at,1 FROM examples WHERE id=?",params![archived_id,id])?;
            transaction.execute(
                "UPDATE entry_examples SET example_id=? WHERE example_id=?",
                params![archived_id, id],
            )?;
            transaction.execute("INSERT INTO example_media(example_id,asset_id) SELECT ?,asset_id FROM example_media WHERE example_id=?",params![archived_id,id])?;
            for entry in affected {
                crate::synchronization::mark_dirty(&transaction, &entry)?;
            }
        }
        let old_links: std::collections::HashMap<String, String> = {
            let mut query=transaction.prepare("SELECT candidate_key,entry_id FROM occurrences WHERE source_id=? AND example_id=? AND entry_id IS NOT NULL")?;
            query
                .query_map(params![source_id, id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<_, _>>()?
        };
        let changed=transaction.execute("UPDATE examples SET text=?,start_ms=?,end_ms=?,revision=revision+1 WHERE id=? AND source_id=? AND revision=?",params![text.trim(),start_ms,end_ms,id,source_id,revision])?;
        if changed == 0 {
            return Err(AppError::new(
                "conflict",
                "例句已发生变化，请刷新后再修改。",
            ));
        }
        transaction.execute(
            "DELETE FROM occurrences WHERE source_id=? AND example_id=?",
            params![source_id, id],
        )?;
        transaction.execute(
            "DELETE FROM candidate_decisions WHERE source_id=? AND scope_key=?",
            params![source_id, id],
        )?;
        for token in tokens(text) {
            transaction.execute("INSERT OR IGNORE INTO occurrences(id,source_id,example_id,candidate_key,raw_text,token_start,token_end,kind,entry_id) VALUES (?,?,?,?,?,?,?,?,?)",params![Uuid::new_v4().to_string(),source_id,id,token.key,token.raw,token.start as i64,token.end as i64,token.kind,old_links.get(&token.key)])?;
        }
        if old_start != start_ms || old_end != end_ms {
            transaction.execute("DELETE FROM example_media WHERE example_id=?", [id])?;
        }
        transaction.commit()?;
        drop(connection);
        self.source_example(source_id, id)
    }

    pub fn ensure_clip(
        &self,
        tools: &MediaTools,
        source_id: &str,
        example_id: &str,
        cancelled: &AtomicBool,
    ) -> Result<AudioAsset, AppError> {
        let example = self.source_example(source_id, example_id)?;
        let recipe = vocabulary::digest(
            format!(
                "aac128-v1:{source_id}:{}:{}",
                example.start_ms, example.end_ms
            )
            .as_bytes(),
        );
        if let Some(asset) = self.asset_by_recipe(&recipe)?
            && self.media_file(&asset.id).is_ok()
        {
            return Ok(asset);
        }
        let audio: String = self.connection()?.query_row(
            "SELECT audio_path FROM sources WHERE id=?",
            [source_id],
            |row| row.get(0),
        )?;
        if !Path::new(&audio).is_file() {
            return Err(AppError::new(
                "resource_missing",
                "预习原声音频不可用，请重新导入素材；已有收录片段仍可使用。",
            ));
        }
        let temporary = self
            .root()
            .join("jobs")
            .join(format!("clip-{}.m4a", Uuid::new_v4()));
        let result = (|| {
            let duration = media::extract_clip(
                tools,
                Path::new(&audio),
                &temporary,
                example.start_ms,
                example.end_ms,
                cancelled,
            )?;
            let digest = file_hash(&temporary)?;
            let relative = format!("media/{digest}.m4a");
            let destination = self.root().join(&relative);
            if destination.exists() {
                if file_hash(&destination)? != digest {
                    std::fs::remove_file(&destination)?;
                    std::fs::rename(&temporary, &destination)?;
                }
            } else {
                // A simultaneous preview may have installed the same content.
                if let Err(error) = std::fs::rename(&temporary, &destination)
                    && (!destination.is_file() || file_hash(&destination)? != digest)
                {
                    return Err(error.into());
                }
            }
            let connection = self.connection()?;
            let existing: Option<String> = connection
                .query_row(
                    "SELECT id FROM media_assets WHERE recipe_key=?",
                    [&recipe],
                    |row| row.get(0),
                )
                .optional()?;
            let id = existing.unwrap_or_else(|| Uuid::new_v4().to_string());
            connection.execute("INSERT INTO media_assets(id,recipe_key,digest,relative_path,kind,format,duration_ms,state,created_at) VALUES (?,?,?,?,'original','m4a',?,'ready',?) ON CONFLICT(recipe_key) DO UPDATE SET digest=excluded.digest,relative_path=excluded.relative_path,duration_ms=excluded.duration_ms,state='ready'",params![id,recipe,digest,relative,duration,Utc::now().timestamp_millis()])?;
            Ok(AudioAsset {
                id,
                relative_path: relative,
                duration_ms: duration,
                kind: "original".into(),
                state: "ready".into(),
            })
        })();
        let _ = std::fs::remove_file(&temporary);
        result
    }

    fn asset_by_recipe(&self, recipe: &str) -> Result<Option<AudioAsset>, AppError> {
        Ok(self.connection()?.query_row("SELECT id,relative_path,duration_ms,kind,state FROM media_assets WHERE recipe_key=?",[recipe],|row|Ok(AudioAsset{id:row.get(0)?,relative_path:row.get(1)?,duration_ms:row.get(2)?,kind:row.get(3)?,state:row.get(4)?})).optional()?)
    }

    pub fn media_file(&self, id: &str) -> Result<PathBuf, AppError> {
        let record: Option<(String, String, String)> = self
            .connection()?
            .query_row(
                "SELECT relative_path,digest,state FROM media_assets WHERE id=?",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (relative, digest, state) =
            record.ok_or_else(|| AppError::new("not_found", "音频资料不存在。"))?;
        validate_media_file(self.root(), &relative, &digest, &state)
    }
}

pub(crate) fn validate_media_file(
    root: &Path,
    relative: &str,
    digest: &str,
    state: &str,
) -> Result<PathBuf, AppError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(AppError::new("invalid_data", "音频位置无效。"));
    }
    let path = root.join(path);
    let missing = || {
        AppError::new(
            "resource_missing",
            "已保存原声缺失或损坏，可以重新截取或选择系统语音。",
        )
    };
    let resolved = path.canonicalize().map_err(|_| missing())?;
    if !resolved.starts_with(root.join("media").canonicalize()?) {
        return Err(AppError::new(
            "invalid_data",
            "音频位置不在词库媒体目录内。",
        ));
    }
    if state != "ready" || !resolved.is_file() || file_hash(&resolved)? != digest {
        return Err(missing());
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn corpus() -> ImportedMedia {
        ImportedMedia {
            title: "Synthetic episode".into(),
            duration_ms: 5000,
            audio_path: PathBuf::from("audio.m4a"),
            audio_stream: 0,
            subtitle_stream: Some(1),
            text_source: "embedded_text".into(),
            warnings: vec![],
            segments: vec![
                media::Segment {
                    location_key: "cue:0".into(),
                    text: "[Claire] Please give up. Give up!".into(),
                    start_ms: 0,
                    end_ms: 2000,
                },
                media::Segment {
                    location_key: "cue:1".into(),
                    text: "I will give up tomorrow.".into(),
                    start_ms: 2000,
                    end_ms: 5000,
                },
            ],
        }
    }
    #[test]
    fn missing_import_audio_requires_reimport_without_losing_corrected_text() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let mut data = corpus();
        data.audio_path = directory.path().join("jobs/audio.m4a");
        let corpus_path = directory.path().join("jobs/corpus.json");
        std::fs::write(&data.audio_path, b"synthetic audio").unwrap();
        std::fs::write(&corpus_path, serde_json::to_vec(&data).unwrap()).unwrap();
        let source = store
            .install_corpus(Path::new("test.mkv"), "same", &data, &corpus_path)
            .unwrap();
        assert!(store.cached_source("same").unwrap().is_some());
        let example = store
            .candidate_examples(&source.id, "tomorrow")
            .unwrap()
            .remove(0);
        store
            .update_source_example(
                &source.id,
                &example.id,
                example.revision,
                "I will give up next week.",
                2000,
                5000,
            )
            .unwrap();
        std::fs::remove_file(&data.audio_path).unwrap();
        assert!(store.cached_source("same").unwrap().is_none());
        std::fs::write(&data.audio_path, b"restored audio").unwrap();
        store
            .install_corpus(Path::new("test.mkv"), "same", &data, &corpus_path)
            .unwrap();
        assert_eq!(
            store.source_example(&source.id, &example.id).unwrap().text,
            "I will give up next week."
        );
        assert!(
            store
                .candidates(&source.id, "tomorrow", "word", false, 0, 50)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store
                .candidates(&source.id, "week", "word", false, 0, 50)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn candidate_counts_keep_occurrences_and_context_decisions_separate() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let data = corpus();
        let source = store
            .install_corpus(
                Path::new("test.mkv"),
                "same",
                &data,
                Path::new("corpus.json"),
            )
            .unwrap();
        let candidates = store
            .candidates(&source.id, "give up", "phrase", true, 0, 50)
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].count, 3);
        assert_eq!(candidates[0].example_count, 2);
        assert!(
            store
                .candidates(&source.id, "claire", "word", false, 0, 50)
                .unwrap()
                .is_empty()
        );
        let examples = store.candidate_examples(&source.id, "give up").unwrap();
        store
            .decide_candidate(&source.id, "give up", &examples[0].id, "familiar")
            .unwrap();
        assert_eq!(
            store
                .candidates(&source.id, "give up", "phrase", true, 0, 50)
                .unwrap()
                .len(),
            1
        );
        store
            .install_corpus(
                Path::new("test.mkv"),
                "same",
                &data,
                Path::new("corpus.json"),
            )
            .unwrap();
        assert_eq!(
            store
                .candidates(&source.id, "give up", "phrase", false, 0, 50)
                .unwrap()[0]
                .count,
            3
        );
    }

    #[test]
    fn correcting_source_preserves_collected_quote_and_its_original_timing() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let data = corpus();
        let source = store
            .install_corpus(
                Path::new("test.mkv"),
                "same",
                &data,
                Path::new("corpus.json"),
            )
            .unwrap();
        let example = store
            .candidate_examples(&source.id, "give up")
            .unwrap()
            .remove(0);
        let input =
            crate::application::example_input_from_corpus(&store, &source.id, &example.id, "放弃")
                .unwrap();
        let result = store
            .collect(&vocabulary::CollectionInput {
                operation_id: Uuid::new_v4().to_string(),
                kind: "phrase".into(),
                text: "give up".into(),
                meaning: "放弃".into(),
                examples: vec![input],
                target_entry_id: None,
                expected_revision: None,
            })
            .unwrap();
        assert_eq!(
            store.get_entry(&result.entry_id).unwrap().occurrence_count,
            3
        );
        store
            .update_source_example(
                &source.id,
                &example.id,
                example.revision,
                "Please keep trying.",
                100,
                1900,
            )
            .unwrap();
        let entry = store.get_entry(&result.entry_id).unwrap();
        assert_eq!(entry.examples[0].text, "[Claire] Please give up. Give up!");
        assert_eq!(entry.examples[0].start_ms, Some(0));
        assert_eq!(store.source(&source.id).unwrap().example_count, 2);
        assert_eq!(
            store.source_example(&source.id, &example.id).unwrap().text,
            "Please keep trying."
        );
    }
}
