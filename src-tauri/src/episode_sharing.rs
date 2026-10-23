//! Account-scoped prepared text. This is separate from collected learning facts.
use crate::{error::AppError, explanations::Explanation, store::Store, vocabulary};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub(crate) const PROTOCOL: &str = "svl-prepared-1";
const PREPARATION_VERSION: &str = "svl-preparation-1";
const MAX_BYTES: usize = crate::sync_limits::MAX_BUNDLE_BYTES;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreparedCue {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub translation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreparedWord {
    pub target: String,
    pub context: String,
    pub meaning: String,
    pub translation: String,
    pub notes: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreparedEpisode {
    pub protocol: String,
    pub preparation_version: String,
    pub media_sha256: String,
    pub title: String,
    pub duration_ms: i64,
    pub prepared_at: i64,
    pub subtitle_revision: String,
    pub subtitle_delay_ms: i64,
    pub chinese_offset_ms: i64,
    pub cues: Vec<PreparedCue>,
    pub explanations: Vec<PreparedWord>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedEpisodeMeta {
    pub media_sha256: String,
    pub revision: i64,
    pub digest: String,
    pub bytes: u64,
    pub title: String,
    pub cue_count: u32,
    pub translated_count: u32,
    pub explanation_count: u32,
    pub prepared_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PreparedEpisodeLocal {
    pub media_sha256: String,
    pub title: String,
    pub cue_count: u32,
    pub applied: Option<PreparedEpisodeMeta>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreparedDocument {
    pub metadata: PreparedEpisodeMeta,
    pub package: PreparedEpisode,
}
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PreparedEpisodeApplied {
    pub metadata: PreparedEpisodeMeta,
    pub source_id: String,
}

fn invalid(message: &str) -> AppError {
    AppError::new("invalid_data", message)
}
fn digest_valid(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn text_valid(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.contains('\0')
}
fn has_chinese(text: &str) -> bool {
    text.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
}
pub(crate) fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, AppError> {
    Ok(serde_json::to_vec(&serde_json::to_value(value)?)?)
}

impl PreparedEpisode {
    pub(crate) fn validate(&self) -> Result<(), AppError> {
        if self.protocol != PROTOCOL
            || self.preparation_version != PREPARATION_VERSION
            || !digest_valid(&self.media_sha256)
            || !digest_valid(&self.subtitle_revision)
            || self.title.trim().is_empty()
            || !text_valid(&self.title, 500)
            || self.duration_ms <= 0
            || self.prepared_at < 0
            || self.subtitle_delay_ms.unsigned_abs() > 3600000
            || self.chinese_offset_ms.unsigned_abs() > 3600000
            || self.cues.is_empty()
            || self.cues.len() > 100000
            || self.explanations.len() > 100000
        {
            return Err(invalid("共享剧集身份、版本或资料数量无效。"));
        }
        let mut last = -1;
        let mut contexts = HashSet::new();
        for cue in &self.cues {
            if cue.start_ms < 0
                || cue.start_ms < last
                || cue.end_ms <= cue.start_ms
                || cue.end_ms > self.duration_ms.saturating_add(1000)
                || !text_valid(&cue.text, 4000)
                || !cue.text.chars().any(|c| c.is_ascii_alphabetic())
                || !text_valid(&cue.translation, 16000)
                || (!cue.translation.trim().is_empty() && !has_chinese(&cue.translation))
            {
                return Err(invalid("共享字幕的文字、译文或时间无效。"));
            }
            last = cue.start_ms;
            contexts.insert(cue.text.as_str());
        }
        for word in &self.explanations {
            if word.target.trim().is_empty()
                || !text_valid(&word.target, 4000)
                || !text_valid(&word.context, 20000)
                || (!word.context.is_empty() && !contexts.contains(word.context.as_str()))
            {
                return Err(invalid("共享词语解释不属于本剧集语境。"));
            }
            Explanation {
                meaning: word.meaning.clone(),
                translation: word.translation.clone(),
                notes: word.notes.clone(),
            }
            .validate(&word.context)?;
        }
        if self.subtitle_revision
            != vocabulary::digest(&canonical_bytes(&(&self.cues, self.chinese_offset_ms))?)
        {
            return Err(invalid("共享字幕内容与字幕版本不一致。"));
        }
        let size = canonical_bytes(self)?.len();
        if size > MAX_BYTES {
            return Err(crate::sync_limits::capacity_error(
                "完整剧集准备资料",
                size,
                MAX_BYTES,
            ));
        }
        Ok(())
    }
}
impl PreparedEpisodeMeta {
    pub(crate) fn validate(&self, hash: &str) -> Result<(), AppError> {
        if self.media_sha256 != hash
            || !digest_valid(hash)
            || !digest_valid(&self.digest)
            || self.revision < 1
            || self.bytes == 0
            || self.bytes > MAX_BYTES as u64
            || self.cue_count == 0
            || self.cue_count > 100000
            || self.translated_count > self.cue_count
            || self.explanation_count > 100000
            || !text_valid(&self.title, 500)
            || self.title.trim().is_empty()
            || self.prepared_at < 0
            || self.updated_at < 0
        {
            return Err(invalid("服务器剧集准备版本无效。"));
        }
        Ok(())
    }
}
impl PreparedDocument {
    pub(crate) fn validate(&self, hash: &str) -> Result<(), AppError> {
        self.metadata.validate(hash)?;
        self.package.validate()?;
        let bytes = canonical_bytes(&self.package)?;
        if self.package.media_sha256 != hash
            || self.metadata.digest != vocabulary::digest(&bytes)
            || self.metadata.bytes != bytes.len() as u64
            || self.metadata.title != self.package.title
            || self.metadata.cue_count != self.package.cues.len() as u32
            || self.metadata.translated_count
                != self
                    .package
                    .cues
                    .iter()
                    .filter(|c| !c.translation.trim().is_empty())
                    .count() as u32
            || self.metadata.explanation_count != self.package.explanations.len() as u32
            || self.metadata.prepared_at != self.package.prepared_at
        {
            return Err(invalid("共享剧集完整内容与版本摘要不一致。"));
        }
        Ok(())
    }
}

impl Store {
    pub(crate) fn prepared_episode_local(
        &self,
        source_id: &str,
        scope: Option<&str>,
    ) -> Result<PreparedEpisodeLocal, AppError> {
        let (title,count):(String,u32)=self.connection()?.query_row("SELECT title,(SELECT COUNT(*) FROM examples WHERE source_id=sources.id AND archived=0 AND start_ms IS NOT NULL) FROM sources WHERE id=?",[source_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let hash = self
            .subtitle_bundle(source_id)?
            .map(|b| b.media_sha256)
            .ok_or_else(|| AppError::new("resource_missing", "请先完成本地视频指纹与字幕准备。"))?;
        if !digest_valid(&hash) {
            return Err(invalid("本地视频摘要无效，请重新准备资料。"));
        }
        let applied = scope
            .map(|scope| {
                crate::mobile_video::setting(
                    self,
                    &format!(
                        "prepared.applied:{}:{source_id}",
                        vocabulary::digest(scope.as_bytes())
                    ),
                )
            })
            .transpose()?
            .flatten();
        Ok(PreparedEpisodeLocal {
            media_sha256: hash,
            title,
            cue_count: count,
            applied,
        })
    }
    pub(crate) fn export_prepared_episode(
        &self,
        source_id: &str,
    ) -> Result<PreparedEpisode, AppError> {
        let bundle = self
            .subtitle_bundle(source_id)?
            .ok_or_else(|| AppError::new("resource_missing", "请先准备本集字幕资料。"))?;
        let (title,duration,path,corpus,imported):(String,i64,Option<String>,Option<String>,Option<i64>)=self.connection()?.query_row("SELECT title,COALESCE(duration_ms,0),path_hint,corpus_path,imported_at FROM sources WHERE id=?",[source_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        let corpus = corpus
            .ok_or_else(|| AppError::new("resource_missing", "本机尚未导入此剧集的英文资料。"))?;
        let data: crate::media::ImportedMedia =
            serde_json::from_reader(std::fs::File::open(corpus)?)?;
        let mut cues = Vec::new();
        for segment in data.segments {
            if let Some(text) = crate::subtitles::english_text(&segment.text) {
                let translation = bundle
                    .translation(segment.start_ms, segment.end_ms)
                    .or(self.sentence_translation(&text)?)
                    .unwrap_or_default();
                cues.push(PreparedCue {
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    text,
                    translation,
                });
            }
        }
        cues.sort_by_key(|c| (c.start_ms, c.end_ms));
        let contexts = cues.iter().map(|c| c.text.as_str()).collect::<HashSet<_>>();
        let mut explanations = Vec::new();
        let mut prepared_at = imported.unwrap_or(0);
        {
            let db = self.connection()?;
            let mut query=db.prepare("SELECT target,context,meaning,translation,notes,created_at FROM explanations WHERE context IN (SELECT text FROM examples WHERE source_id=?) OR (context='' AND target IN (SELECT candidate_key FROM occurrences WHERE source_id=?)) ORDER BY target,context")?;
            let rows = query.query_map(params![source_id, source_id], |r| {
                Ok((
                    PreparedWord {
                        target: r.get(0)?,
                        context: r.get(1)?,
                        meaning: r.get(2)?,
                        translation: r.get(3)?,
                        notes: r.get(4)?,
                    },
                    r.get::<_, i64>(5)?,
                ))
            })?;
            for row in rows {
                let (word, created) = row?;
                if (word.context.is_empty() || contexts.contains(word.context.as_str()))
                    && (Explanation {
                        meaning: word.meaning.clone(),
                        translation: word.translation.clone(),
                        notes: word.notes.clone(),
                    })
                    .validate(&word.context)
                    .is_ok()
                {
                    prepared_at = prepared_at.max(created);
                    explanations.push(word);
                }
            }
        }
        let latest:i64=self.connection()?.query_row("SELECT COALESCE(MAX(created_at),0) FROM sentence_translations WHERE sentence IN (SELECT text FROM examples WHERE source_id=?)",[source_id],|r|r.get(0))?;
        prepared_at = prepared_at.max(latest);
        let chinese_offset_ms = bundle
            .assets
            .iter()
            .find(|a| Some(&a.id) == bundle.selected_chinese_id.as_ref())
            .map(|a| a.time_offset_ms)
            .unwrap_or(0);
        let delay = path
            .as_deref()
            .map(|p| self.watch_progress_for(p))
            .transpose()?
            .flatten()
            .map(|p| (p.subtitle_delay * 1000.0).round() as i64)
            .unwrap_or(0);
        let subtitle_revision = vocabulary::digest(&canonical_bytes(&(&cues, chinese_offset_ms))?);
        let title = title
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&title)
            .chars()
            .take(500)
            .collect();
        let packet = PreparedEpisode {
            protocol: PROTOCOL.into(),
            preparation_version: PREPARATION_VERSION.into(),
            media_sha256: bundle.media_sha256,
            title,
            duration_ms: duration,
            prepared_at,
            subtitle_revision,
            subtitle_delay_ms: delay,
            chinese_offset_ms,
            cues,
            explanations,
        };
        packet.validate()?;
        Ok(packet)
    }
    pub(crate) fn cache_prepared_episode(
        &self,
        scope: &str,
        document: &PreparedDocument,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(), AppError> {
        document.validate(&document.metadata.media_sha256)?;
        let directory = self
            .root()
            .join("prepared")
            .join(vocabulary::digest(scope.as_bytes()))
            .join(&document.metadata.media_sha256);
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{}.json", document.metadata.digest));
        if !path.is_file() {
            let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
            std::fs::write(&temporary, canonical_bytes(document)?)?;
            std::fs::rename(temporary, &path)?;
        }
        for word in &document.package.explanations {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(AppError::new(
                    "cancelled",
                    "准备资料下载已取消，本机学习资料保持。",
                ));
            }
            self.save_explanation(
                &word.target,
                &word.context,
                &Explanation {
                    meaning: word.meaning.clone(),
                    translation: word.translation.clone(),
                    notes: word.notes.clone(),
                },
                &format!("prepared://{}", document.metadata.digest),
                &document.package.preparation_version,
            )?;
        }
        Ok(())
    }
    pub(crate) fn prepared_episode_receipt(
        &self,
        scope: &str,
        source_id: &str,
        meta: &PreparedEpisodeMeta,
    ) -> Result<(), AppError> {
        crate::mobile_video::save(
            self,
            &format!(
                "prepared.applied:{}:{source_id}",
                vocabulary::digest(scope.as_bytes())
            ),
            meta,
        )
    }
}

#[cfg(any(
    all(windows, feature = "desktop"),
    all(target_os = "android", feature = "mobile")
))]
impl crate::application::Application {
    pub(crate) fn prepared_episode_local(
        &self,
        source_id: &str,
    ) -> Result<PreparedEpisodeLocal, AppError> {
        let status = self.connection_status()?;
        let scope = status
            .account_scope
            .map(|account| format!("{}|{account}", status.site_url));
        self.store
            .prepared_episode_local(source_id, scope.as_deref())
    }
    pub(crate) fn prepared_episode_check(
        self: &std::sync::Arc<Self>,
        source_id: String,
        operation_id: String,
    ) -> Result<crate::tasks::TaskHandle<Option<PreparedEpisodeMeta>>, AppError> {
        let local = self.prepared_episode_local(&source_id)?;
        let mut remote = crate::sync_http::HttpRemote::new(std::sync::Arc::clone(self))?;
        let hash = local.media_sha256.clone();
        self.start_network_task_typed(
            "prepared_episode_check",
            &operation_id,
            &hash,
            move |context| {
                remote.follow_cancellation(&context);
                context.subject(&local.title);
                context.check_cancelled()?;
                remote.account()?;
                let result = remote.prepared_info(&local.media_sha256)?;
                context.check_cancelled()?;
                Ok(result)
            },
        )
    }
    pub(crate) fn prepared_episode_publish(
        self: &std::sync::Arc<Self>,
        source_id: String,
        expected_revision: i64,
        operation_id: String,
    ) -> Result<crate::tasks::TaskHandle<PreparedEpisodeMeta>, AppError> {
        if expected_revision < 0 {
            return Err(invalid("请先核对服务器准备版本。"));
        }
        let local = self.prepared_episode_local(&source_id)?;
        let mut remote = crate::sync_http::HttpRemote::new(std::sync::Arc::clone(self))?;
        let store = std::sync::Arc::clone(&self.store);
        let guard = std::sync::Arc::clone(&self.sync_guard);
        let hash =
            vocabulary::digest(format!("{}:{expected_revision}", local.media_sha256).as_bytes());
        self.start_network_task_typed(
            "prepared_episode_publish",
            &operation_id,
            &hash,
            move |context| {
                let _guard = guard
                    .try_lock()
                    .map_err(|_| AppError::new("sync_busy", "请等待当前同步或资料交换结束。"))?;
                remote.follow_cancellation(&context);
                context.subject(&local.title);
                context.progress(
                    "prepare",
                    0,
                    1,
                    "正在读取本集已准备字幕、译文和词义，不重新生成翻译",
                );
                context.check_cancelled()?;
                let packet = store.export_prepared_episode(&source_id)?;
                context.check_cancelled()?;
                remote.account()?;
                context.progress("publish", 0, 1, "正在向同账号服务器发布完整准备资料");
                let meta = remote.prepared_publish(&packet, expected_revision)?;
                context.check_cancelled()?;
                store.prepared_episode_receipt(&remote.scope(), &source_id, &meta)?;
                Ok(meta)
            },
        )
    }
    pub(crate) fn prepared_episode_download(
        self: &std::sync::Arc<Self>,
        source_id: String,
        expected_revision: i64,
        operation_id: String,
    ) -> Result<crate::tasks::TaskHandle<PreparedEpisodeApplied>, AppError> {
        let local = self.prepared_episode_local(&source_id)?;
        let mut remote = crate::sync_http::HttpRemote::new(std::sync::Arc::clone(self))?;
        let store = std::sync::Arc::clone(&self.store);
        let guard = std::sync::Arc::clone(&self.sync_guard);
        let hash =
            vocabulary::digest(format!("{}:{expected_revision}", local.media_sha256).as_bytes());
        self.start_network_task_typed(
            "prepared_episode_download",
            &operation_id,
            &hash,
            move |context| {
                let _guard = guard
                    .try_lock()
                    .map_err(|_| AppError::new("sync_busy", "请等待当前同步或资料交换结束。"))?;
                remote.follow_cancellation(&context);
                context.subject(&local.title);
                context.progress("download", 0, 1, "正在从同账号服务器下载本视频的准备资料");
                context.check_cancelled()?;
                remote.account()?;
                let document = remote.prepared_download(&local.media_sha256)?;
                if document.metadata.revision != expected_revision {
                    return Err(AppError::new(
                        "conflict",
                        "服务器准备版本已更新，请刷新核对后再下载。",
                    ));
                }
                document.validate(&local.media_sha256)?;
                context.check_cancelled()?;
                store.cache_prepared_episode(&remote.scope(), &document, &context.cancelled)?;
                context.check_cancelled()?;
                #[cfg(all(target_os = "android", feature = "mobile"))]
                let applied_source =
                    crate::mobile_video::apply_prepared_episode(&store, &source_id, &document)?;
                #[cfg(not(all(target_os = "android", feature = "mobile")))]
                let applied_source = source_id;
                store.prepared_episode_receipt(
                    &remote.scope(),
                    &applied_source,
                    &document.metadata,
                )?;
                Ok(PreparedEpisodeApplied {
                    metadata: document.metadata,
                    source_id: applied_source,
                })
            },
        )
    }
}
