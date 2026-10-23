use crate::sync_transport::{CHUNK_BYTES, MAX_PACKET_BYTES, Transport, read_json};
use crate::{
    application::Application,
    credentials::{self, SiteCredential},
    error::AppError,
    site_connection::site_origin,
    synchronization::{
        ChangePage, Push, PushReply, Remote, RemoteChange, RemoteDocument, SyncConflict, SyncStatus,
    },
    tasks::{TaskHandle, results::{SyncTaskResult, SyncResolution}},
    vocabulary::digest,
};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) struct HttpRemote {
    app: Arc<Application>,
    site: String,
    credential: SiteCredential,
    client: Client,
    supports_chunks: bool,
    supports_assessments: bool,
    supports_delta: bool,
    supports_delta_download: bool,
    supports_prepared_episodes: bool,
    cancelled: Option<Arc<AtomicBool>>,
    confirmed_media: BTreeSet<(String, String)>,
    #[cfg(target_os = "android")]
    metered_audio_remaining: Option<usize>,
}
fn failure(code: &str, message: &str) -> AppError {
    AppError::new(code, message)
}
fn mime(format: &str) -> &'static str {
    match format {
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        _ => "audio/webm",
    }
}
impl HttpRemote {
    pub(crate) fn new(app: Arc<Application>) -> Result<Self, AppError> {
        let settings = app.settings()?;
        if settings.offline_mode {
            return Err(failure(
                "offline_mode",
                "已启用手动离线，已有资料仍可本地学习。",
            ));
        }
        let site = site_origin(&settings.site_url)?;
        let credential = credentials::load(app.store.root(), &site)?
            .filter(|c| {
                c.account_scope.is_some() && c.expires_at > chrono::Utc::now().timestamp_millis()
            })
            .ok_or_else(|| failure("auth_required", "请先由浏览器批准本机的站点账号连接。"))?;
        #[cfg(target_os = "android")]
        crate::device_sync::require_bound(&app.store, &format!("{site}|{}", credential.account_scope.as_deref().unwrap()))?;
        Ok(Self {
            confirmed_media: BTreeSet::new(),
            app,
            site,
            credential,
            client: crate::site_connection::host_client()?,
            supports_chunks: false,
            supports_assessments: false,
            supports_delta: false,
            supports_delta_download: false,
            supports_prepared_episodes: false,
            cancelled: None,
            #[cfg(target_os = "android")]
            metered_audio_remaining: None,
        })
    }
    #[cfg(target_os = "android")]
    fn audio_network(&mut self, bytes: Option<usize>) -> Result<(), AppError> {
        if crate::device_sync::require_audio_network(self.metered_audio_remaining.is_some())? {
            if let Some(bytes)=bytes {
                let remaining=self.metered_audio_remaining.as_mut().unwrap();
                if bytes>*remaining {return Err(failure("audio_budget_exceeded","本次计费网络原声额度已用完；已保存内容和待同步进度保留，可改用不计费网络或重新授权一次。"));}
                *remaining-=bytes;
            }
        }
        Ok(())
    }
    pub(crate) fn scope(&self) -> String {
        format!(
            "{}|{}",
            self.site,
            self.credential.account_scope.as_deref().unwrap()
        )
    }
    fn check(&self) -> Result<(), AppError> {
        if self
            .cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(failure(
                "cancelled",
                "同步已取消，完整资料与待发送内容保留。",
            ));
        }
        let settings = self.app.settings()?;
        if settings.offline_mode {
            return Err(failure(
                "offline_mode",
                "已切换为手动离线，同步内容与进度保留。",
            ));
        }
        if site_origin(&settings.site_url)? != self.site
            || credentials::load(self.app.store.root(), &self.site)?.is_none_or(|c| {
                c.request_id != self.credential.request_id
                    || c.account_scope != self.credential.account_scope
                    || c.expires_at <= chrono::Utc::now().timestamp_millis()
            })
        {
            return Err(failure(
                "connection_changed",
                "站点账号连接已变化，请在当前连接下重新发起同步。",
            ));
        }
        Ok(())
    }
    pub(crate) fn account(&mut self) -> Result<(), AppError> {
        self.check()?;
        let (_, v) = read_json(
            self.client
                .get(format!("{}/api/svl/v1/account", self.site))
                .bearer_auth(&self.credential.token)
                .send()
                .map_err(|_| failure("network_unavailable", "无法检查同步账号，本地内容保留。"))?,
        )?;
        self.check()?;
        if v["accountScope"].as_str() != self.credential.account_scope.as_deref()
            || v["schemaVersion"].as_u64() != Some(1)
        {
            return Err(failure(
                "auth_required",
                "同步账号或协议与已批准连接不一致。",
            ));
        }
        self.supports_chunks = v["packetTransport"].as_str() == Some("chunks-1")
            && v["packetChunkBytes"].as_u64() == Some(CHUNK_BYTES as u64)
            && v["maxPacketBytes"].as_u64() == Some(MAX_PACKET_BYTES as u64);
        self.supports_assessments = v["assessmentSync"].as_str() == Some("assessment-1");
        self.supports_delta = v["bundleDelta"].as_str() == Some(crate::sync_delta::PROTOCOL);
        self.supports_delta_download = v["bundleDeltaDownload"].as_str() == Some(crate::sync_delta::PROTOCOL) && self.supports_chunks;
        self.supports_prepared_episodes=v["preparedEpisodes"].as_str()==Some(crate::episode_sharing::PROTOCOL);
        self.app
            .store
            .assessment_sync_capability(&self.scope(), v["assessmentSync"].as_str())?;
        Ok(())
    }
    pub(crate) fn follow_cancellation(&mut self,context:&crate::tasks::TaskContext) {self.cancelled=Some(Arc::clone(&context.cancelled));}
    fn require_prepared(&self)->Result<(),AppError> {self.check()?;if !self.supports_prepared_episodes{return Err(failure("capability_missing","当前站点尚不支持剧集准备资料，请完成服务器更新后重试。"));}Ok(())}
    pub(crate) fn prepared_info(&mut self,hash:&str)->Result<Option<crate::episode_sharing::PreparedEpisodeMeta>,AppError> {
        self.require_prepared()?;
        let response=self.client.get(format!("{}/api/svl/v1/prepared-episodes/{hash}/info",self.site)).bearer_auth(&self.credential.token).send().map_err(|_|failure("network_unavailable","无法检查同账号的剧集准备资料。"))?;
        self.check()?;if response.status().as_u16()==404{return Ok(None);}
        let (_,value)=read_json(response)?;let meta:crate::episode_sharing::PreparedEpisodeMeta=serde_json::from_value(value)?;meta.validate(hash)?;Ok(Some(meta))
    }
    pub(crate) fn prepared_publish(&mut self,packet:&crate::episode_sharing::PreparedEpisode,base_revision:i64)->Result<crate::episode_sharing::PreparedEpisodeMeta,AppError> {
        self.require_prepared()?;packet.validate()?;
        let (status,value)=read_json(self.client.put(format!("{}/api/svl/v1/prepared-episodes/{}",self.site,packet.media_sha256)).bearer_auth(&self.credential.token).json(&json!({"baseRevision":base_revision,"package":packet})).send().map_err(|_|failure("network_unavailable","准备资料发布中断，本机原资料保留。"))?)?;
        self.check()?;if status==409{return Err(failure("conflict","服务器准备资料已变化，请检查服务器版本并核对后再发布。"));}
        let metadata:crate::episode_sharing::PreparedEpisodeMeta=serde_json::from_value(value)?;
        crate::episode_sharing::PreparedDocument{metadata:metadata.clone(),package:packet.clone()}.validate(&packet.media_sha256)?;Ok(metadata)
    }
    pub(crate) fn prepared_download(&mut self,hash:&str)->Result<crate::episode_sharing::PreparedDocument,AppError> {
        self.require_prepared()?;
        let response=self.client.get(format!("{}/api/svl/v1/prepared-episodes/{hash}",self.site)).bearer_auth(&self.credential.token).send().map_err(|_|failure("network_unavailable","剧集准备资料下载中断，本机内容保留。"))?;
        if !response.status().is_success(){read_json(response)?;return Err(failure("resource_missing","同账号尚无此视频的完整准备资料。"));}
        let mut bytes=Vec::new();response.take((MAX_PACKET_BYTES+1) as u64).read_to_end(&mut bytes).map_err(|_|failure("network_unavailable","准备资料下载中断，请重试。"))?;
        self.check()?;if bytes.len()>MAX_PACKET_BYTES{return Err(failure("capacity_exceeded","完整剧集准备资料超过容量，本机原资料保持。"));}
        let document:crate::episode_sharing::PreparedDocument=serde_json::from_slice(&bytes)?;document.validate(hash)?;Ok(document)
    }
    fn get(&mut self, path: &str) -> Result<Value, AppError> {
        Transport::new(&self.client, &self.site, &self.credential.token)
            .fetch(path, || self.check())
    }
    fn get_document(&mut self, path: &str, id: &str, assessment: bool) -> Result<Value, AppError> {
        let base = if self.supports_delta_download {
            self.app.store.sync_download_base(&self.scope(), id, assessment)?
        } else { None };
        Transport::new(&self.client, &self.site, &self.credential.token)
            .fetch_since(path, base.as_ref().map(|(revision,bundle)|(*revision,bundle)), || self.check())
    }
    fn receive_assessment(
        &mut self,
        scope: &str,
        document: &crate::assessment_sync::AssessmentDocument,
    ) -> Result<(), AppError> {
        document.bundle.validate()?;
        for media in &document.bundle.media {
            self.check()?;
            if let Ok(bytes) = self
                .app
                .store
                .assessment_sync_media(&media.digest, &media.format)
            {
                self.app.store.assessment_save_remote_media(media, &bytes)?;
                continue;
            }
            let bytes = self.download(&media.digest, &media.format)?;
            self.app.store.assessment_save_remote_media(media, &bytes)?;
        }
        self.app.store.assessment_sync_receive(scope, document)?;
        Ok(())
    }
    fn sync_assessments(
        &mut self,
        scope: &str,
        context: &crate::tasks::TaskContext,
    ) -> Result<crate::assessment_sync::AssessmentSyncStatus, AppError> {
        if !self.supports_assessments {
            return self.app.store.assessment_sync_status(scope);
        }
        let mut cursor = self.app.store.assessment_sync_status(scope)?.cursor;
        loop {
            context.check_cancelled()?;
            let page = self.get(&format!("/api/svl/v1/assessments/changes?after={cursor}"))?;
            let changes = page["changes"]
                .as_array()
                .ok_or_else(|| failure("invalid_data", "测试历史游标响应无效。"))?;
            for change in changes {
                let next = change["cursor"]
                    .as_i64()
                    .filter(|v| *v > cursor)
                    .ok_or_else(|| failure("invalid_data", "测试历史游标没有推进。"))?;
                let session_id = change["sessionId"].as_str()
                    .ok_or_else(|| failure("invalid_data", "测试历史缺少会话标识。"))?;
                let revision = change["revision"].as_i64().filter(|v| *v > 0)
                    .ok_or_else(|| failure("invalid_data", "测试历史版本无效。"))?;
                if self.app.store.assessment_revision_known(scope, session_id, revision)? {
                    self.app.store.assessment_sync_cursor(scope, next)?;
                    cursor = next;
                    continue;
                }
                context.progress(
                    "assessment_pull",
                    cursor as usize,
                    0,
                    "正在保存完整水平测试历史",
                );
                let document: crate::assessment_sync::AssessmentDocument = serde_json::from_value(
                    self.get_document(&format!("/api/svl/v1/assessments/changes/{next}"), session_id, true)?,
                )?;
                if change["sessionId"].as_str() != Some(document.session_id.as_str())
                    || change["revision"].as_i64() != Some(document.revision)
                {
                    return Err(failure("invalid_data", "测试历史变更关联无效。"));
                }
                self.receive_assessment(scope, &document)?;
                self.app.store.assessment_sync_cursor(scope, next)?;
                cursor = next;
            }
            if changes.len() < 50 {
                break;
            }
        }
        // A confirmed version conflict may create one merged append. Unknown responses keep the same frozen push.
        for _ in 0..2 {
            let sessions = self.app.store.assessment_push_candidates(scope)?;
            if sessions.is_empty() {
                break;
            }
            for session in sessions {
                context.check_cancelled()?;
                let Some(push) = self.app.store.assessment_freeze_push(scope, &session)? else { continue; };
                context.progress(
                    "assessment_push",
                    0,
                    0,
                    "正在发送完整水平测试会话、作答和报告",
                );
                for media in &push.bundle.media {
                    if self.confirmed_media.contains(&(media.digest.clone(),media.format.clone())) {
                        continue;
                    }
                    let bytes = self
                        .app
                        .store
                        .assessment_sync_media(&media.digest, &media.format)?;
                    self.upload(&media.digest, &media.format, bytes)?;
                }
                let base = if self.supports_delta {
                    self.app.store.sync_upload_base(scope, &session, push.base_revision, true)?
                } else { None };
                let (status, value) =
                    Transport::new(&self.client, &self.site, &self.credential.token).push_changes(
                        &format!("/api/svl/v1/assessments/{}", push.bundle.session.id),
                        &serde_json::to_value(&push)?,
                        base.as_ref(),
                        "sessionId",
                        self.supports_chunks,
                        || self.check(),
                    )?;
                self.check()?;
                if value["sessionId"].as_str() != Some(push.bundle.session.id.as_str()) {
                    return Err(failure("invalid_data", "测试回执会话标识不一致。"));
                }
                if status == 409 {
                    let document: crate::assessment_sync::AssessmentDocument =
                        serde_json::from_value(value)?;
                    self.receive_assessment(scope, &document)?;
                } else {
                    let revision = value["revision"]
                        .as_i64()
                        .ok_or_else(|| failure("invalid_data", "测试回执缺少版本。"))?;
                    self.app
                        .store
                        .assessment_sync_saved(scope, &push, revision)?;
                }
            }
        }
        self.app.store.assessment_sync_status(scope)
    }
}
impl Remote for HttpRemote {
    fn changes(&mut self, after: i64) -> Result<ChangePage, AppError> {
        Ok(serde_json::from_value(
            self.get(&format!("/api/svl/v1/changes?after={after}"))?,
        )?)
    }
    fn change(&mut self, change: &RemoteChange) -> Result<RemoteDocument, AppError> {
        Ok(serde_json::from_value(
            self.get_document(&format!("/api/svl/v1/changes/{}", change.cursor), &change.entry_id, false)?,
        )?)
    }
    fn push(&mut self, request: &Push) -> Result<PushReply, AppError> {
        let base = if self.supports_delta {
            self.app.store.sync_upload_base(&self.scope(), request.bundle.entry["id"].as_str().unwrap(), request.base_revision, false)?
        } else { None };
        let (status, v) = Transport::new(&self.client, &self.site, &self.credential.token).push(
            request,
            base.as_ref(),
            self.supports_chunks,
            || self.check(),
        )?;
        self.check()?;
        if v["entryId"] != request.bundle.entry["id"] {
            return Err(failure("invalid_data", "站点回执的词条标识无效。"));
        }
        if status == 409 {
            return Ok(PushReply::Conflict(serde_json::from_value(v)?));
        }
        Ok(PushReply::Saved {
            i: v["revision"]
                .as_i64()
                .ok_or_else(|| failure("invalid_data", "站点回执版本无效。"))?,
        })
    }
    fn upload(&mut self, hash: &str, format: &str, bytes: Vec<u8>) -> Result<(), AppError> {
        self.check()?;
        let key = (hash.to_owned(), format.to_owned());
        if self.confirmed_media.contains(&key) { return Ok(()); }
        if bytes.len() > crate::sync_limits::MAX_AUDIO_BYTES {
            return Err(crate::sync_limits::capacity_error("原声音频", bytes.len(), crate::sync_limits::MAX_AUDIO_BYTES));
        }
        if digest(&bytes) != hash {
            return Err(failure(
                "capacity_exceeded",
                "原声过大或摘要不匹配，原文件与待同步记录保留。",
            ));
        }
        let url = format!("{}/api/svl/v1/media/{hash}", self.site);
        let head = self
            .client
            .head(&url)
            .bearer_auth(&self.credential.token)
            .send()
            .map_err(|_| failure("network_unavailable", "无法检查站点原声，待同步记录保留。"))?;
        if head.status().is_success() {
            if head
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                != Some(mime(format))
            {
                return Err(failure("invalid_data", "已保存原声的格式不一致。"));
            }
            self.check()?;
            self.confirmed_media.insert(key);
            return Ok(());
        }
        if head.status().as_u16() != 404 {
            read_json(head)?;
            return Err(failure("network_error", "站点原声状态无效。"));
        }
        #[cfg(target_os = "android")]
        self.audio_network(Some(bytes.len()))?;
        let size = bytes.len();
        let (_, v) = read_json(
            self.client
                .put(format!("{url}?format={format}"))
                .bearer_auth(&self.credential.token)
                .header("content-type", mime(format))
                .body(bytes)
                .send()
                .map_err(|_| failure("network_unavailable", "原声上传中断，待同步内容保留。"))?,
        )?;
        self.check()?;
        if v["digest"].as_str() != Some(hash)
            || v["size"].as_u64() != Some(size as u64)
            || v["format"].as_str() != Some(format)
        {
            return Err(failure("invalid_data", "原声上传回执无效。"));
        }
        self.confirmed_media.insert(key);
        Ok(())
    }
    fn download(&mut self, hash: &str, format: &str) -> Result<Vec<u8>, AppError> {
        self.check()?;
        #[cfg(target_os = "android")]
        self.audio_network(None)?;
        let response = self
            .client
            .get(format!("{}/api/svl/v1/media/{hash}", self.site))
            .bearer_auth(&self.credential.token)
            .send()
            .map_err(|_| failure("network_unavailable", "原声下载中断，游标保留。"))?;
        if !response.status().is_success() {
            read_json(response)?;
            return Err(failure("resource_missing", "原声不可用，游标保留。"));
        }
        if response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            != Some(mime(format))
            || response
                .headers()
                .get("x-content-sha256")
                .and_then(|h| h.to_str().ok())
                != Some(hash)
        {
            return Err(failure("invalid_data", "原声下载的类型或摘要标记不匹配。"));
        }
        let mut bytes = vec![];
        #[cfg(target_os = "android")]
        if self.metered_audio_remaining.is_some() {
            let size=response.content_length().filter(|n|*n<=crate::sync_limits::MAX_AUDIO_BYTES as u64)
                .ok_or_else(||failure("audio_size_unknown","无法确认原声大小，未授权继续传输；请改用不计费网络。"))?;
            self.audio_network(Some(size as usize))?;
        }
        response
            .take((crate::sync_limits::MAX_AUDIO_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| failure("network_unavailable", "原声下载中断，游标保留。"))?;
        self.check()?;
        Ok(bytes)
    }
}
impl Application {
    pub fn assessment_sync_keep_both(
        &self,
        input: &crate::assessment_sync::KeepAssessmentBoth,
    ) -> Result<Value, AppError> {
        let _guard = self
            .sync_guard
            .try_lock()
            .map_err(|_| failure("busy", "同步正在进行，请稍后处理冲突。"))?;
        let site = site_origin(&self.settings()?.site_url)?;
        let c = credentials::load(self.store.root(), &site)?
            .filter(|c| c.account_scope.is_some())
            .ok_or_else(|| failure("not_connected", "请先连接原账号。"))?;
        self.store
            .assessment_sync_keep_both(&format!("{site}|{}", c.account_scope.unwrap()), input)
    }
    pub fn assessment_sync_conflicts(&self) -> Result<Vec<Value>, AppError> {
        let settings = self.settings()?;
        let site = site_origin(&settings.site_url)?;
        if let Some(c) =
            credentials::load(self.store.root(), &site)?.filter(|c| c.account_scope.is_some())
        {
            self.store
                .assessment_sync_conflicts(&format!("{site}|{}", c.account_scope.unwrap()))
        } else {
            Ok(vec![])
        }
    }
    pub fn assessment_sync_status(
        &self,
    ) -> Result<crate::assessment_sync::AssessmentSyncStatus, AppError> {
        let settings = self.settings()?;
        let site = site_origin(&settings.site_url)?;
        if let Some(c) =
            credentials::load(self.store.root(), &site)?.filter(|c| c.account_scope.is_some())
        {
            self.store
                .assessment_sync_status(&format!("{site}|{}", c.account_scope.unwrap()))
        } else {
            let pending = self.store.connection()?.query_row(
                "SELECT COUNT(*) FROM assessment_sessions",
                [],
                |r| r.get(0),
            )?;
            Ok(crate::assessment_sync::AssessmentSyncStatus {
                pending,
                ..Default::default()
            })
        }
    }
    pub fn sync_status(&self) -> Result<SyncStatus, AppError> {
        let settings = self.settings()?;
        let site = site_origin(&settings.site_url)?;
        if let Some(c) =
            credentials::load(self.store.root(), &site)?.filter(|c| c.account_scope.is_some())
        {
            self.store
                .synchronization_status(&format!("{site}|{}", c.account_scope.unwrap()))
        } else {
            Ok(SyncStatus::default())
        }
    }
    pub fn sync_conflicts(&self) -> Result<Vec<SyncConflict>, AppError> {
        let settings = self.settings()?;
        let site = site_origin(&settings.site_url)?;
        if let Some(c) =
            credentials::load(self.store.root(), &site)?.filter(|c| c.account_scope.is_some())
        {
            self.store
                .synchronization_conflicts(&format!("{site}|{}", c.account_scope.unwrap()))
        } else {
            Ok(vec![])
        }
    }
    pub fn sync_start(self: &Arc<Self>, operation: String) -> Result<TaskHandle<SyncTaskResult>, AppError> {
        self.sync_start_with_metered(operation,false)
    }
    pub(crate) fn sync_start_with_metered(self: &Arc<Self>, operation: String, allow_metered:bool) -> Result<TaskHandle<SyncTaskResult>, AppError> {
        let mut remote = HttpRemote::new(Arc::clone(self))?;
        #[cfg(target_os = "android")]
        if allow_metered {remote.metered_audio_remaining=Some(crate::sync_limits::MAX_AUDIO_BYTES);}
        let scope = remote.scope();
        let store = Arc::clone(&self.store);
        let guard = Arc::clone(&self.sync_guard);
        let hash = digest(format!("{scope}:{allow_metered}").as_bytes());
        self.start_network_task_typed("sync", &operation, &hash, move |context| {
            remote.cancelled = Some(Arc::clone(&context.cancelled));
            let _guard = guard
                .try_lock()
                .map_err(|_| failure("sync_busy", "已有同步正在进行，请等待完成或取消。"))?;
            context.check_cancelled()?;
            remote.account()?;
            let result =
                store.synchronize(&scope, &mut remote, &context.cancelled, |stage, n| {
                    context.progress(
                        stage,
                        n,
                        0,
                        if stage == "pull" {
                            "正在拉取并保存词条、原声与学习资料"
                        } else {
                            "正在发送待同步的完整资料"
                        },
                    )
                })?;
            let assessment = remote.sync_assessments(&scope, &context)?;
            #[cfg(target_os = "android")]
            if result.conflicts==0 && store.synchronization_status(&scope)?.pending==0 && assessment.conflicts==0 && assessment.pending==0 && assessment.pending_references==0 {
                crate::device_sync::completed(&store, &scope)?;
            }
            Ok(SyncTaskResult { entries: result, assessment })
        })
    }
    pub fn sync_resolve(
        self: &Arc<Self>,
        input: crate::synchronization::ResolutionInput,
        operation: String,
    ) -> Result<TaskHandle<SyncResolution>, AppError> {
        let mut remote = HttpRemote::new(Arc::clone(self))?;
        let scope = remote.scope();
        let store = Arc::clone(&self.store);
        let guard = Arc::clone(&self.sync_guard);
        let hash = digest(serde_json::to_vec(&json!([scope, input]))?.as_slice());
        self.start_network_task_typed("sync_resolve", &operation, &hash, move |context| {
            let _guard = guard
                .try_lock()
                .map_err(|_| failure("sync_busy", "请等待已有同步结束再处理。"))?;
            context.check_cancelled()?;
            remote.account()?;
            store.synchronization_resolve(&scope, &input, &mut remote, &context.cancelled)?;
            Ok(SyncResolution { resolved: true })
        })
    }
}

#[cfg(test)]
#[path = "sync_live_acceptance.rs"]
mod live_acceptance;

#[cfg(test)]
mod live_recovery_tests {
    use super::*;
    use crate::{
        application::Settings,
        reviews::AnswerInput,
        store::Store,
        synchronization::{Remote, ResolutionInput},
        tasks::TaskManager,
        vocabulary::{CollectionInput, ExampleInput},
    };
    use std::{path::PathBuf, sync::atomic::AtomicBool};
    use uuid::Uuid;

    #[test]
    #[ignore = "Requires a real approved connection, private audio, and the chunks-1 service"]
    fn actual_large_history_and_original_audio_roundtrip() {
        let credential_root = PathBuf::from(
            std::env::var_os("SVL_LIVE_CREDENTIAL_ROOT").expect("approved private credential root"),
        );
        let test_root = PathBuf::from(
            std::env::var_os("SVL_LIVE_TEST_ROOT").expect("dedicated private integration root"),
        );
        let audio = std::fs::read(
            std::env::var_os("SVL_LIVE_AUDIO_FIXTURE").expect("private original clip"),
        )
        .unwrap();
        assert!(!test_root.exists(), "Use a fresh dedicated test directory");
        let credential = credentials::load(
            &credential_root,
            "https://english-copy-practice.hxwjb.chatgpt.site",
        )
        .unwrap()
        .expect("real approval");
        let first = application(&test_root.join("first"), &credential);
        let entry = crate::sync_test_fixture::large_entry(&first.store, Some(&audio), 3000);
        let bundle = first.store.sync_export(&entry).unwrap();
        let size = serde_json::to_vec(&bundle).unwrap().len();
        assert!(size > 2_000_000 && size < MAX_PACKET_BYTES - 4096);
        let mut remote = InterruptedRemote {
            inner: HttpRemote::new(Arc::clone(&first)).unwrap(),
            lose_ack: true,
            lose_download: false,
        };
        remote.inner.account().unwrap();
        assert!(remote.inner.supports_chunks);
        let scope = remote.inner.scope();
        let cancel = AtomicBool::new(false);
        assert_eq!(
            first
                .store
                .synchronize(&scope, &mut remote, &cancel, |_, _| {})
                .unwrap_err()
                .code,
            "network_error"
        );
        assert_eq!(
            first.store.synchronization_status(&scope).unwrap().pending,
            1
        );
        first
            .store
            .synchronize(&scope, &mut remote, &cancel, |_, _| {})
            .unwrap();
        let saved: RemoteDocument = serde_json::from_value(
            remote
                .inner
                .get(&format!("/api/svl/v1/entries/{entry}"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.bundle.tables["review_attempts"].len(), 3000);
        assert_eq!(saved.bundle.tables["collection_actions"].len(), 1);
        let second = application(&test_root.join("second"), &credential);
        let mut receiver = HttpRemote::new(Arc::clone(&second)).unwrap();
        receiver.account().unwrap();
        second
            .store
            .synchronize(&scope, &mut receiver, &cancel, |_, _| {})
            .unwrap();
        let document = second.store.sync_export(&entry).unwrap();
        assert_eq!(document.tables["review_attempts"].len(), 3000);
        assert_eq!(document.tables["collection_actions"].len(), 1);
        for (old, new) in bundle.tables["review_attempts"]
            .iter()
            .zip(&document.tables["review_attempts"])
        {
            for field in [
                "id",
                "operation_id",
                "request_hash",
                "answer",
                "outcome",
                "created_at",
            ] {
                assert_eq!(old[field], new[field]);
            }
            let old_question: Value =
                serde_json::from_str(old["question_json"].as_str().unwrap()).unwrap();
            let new_question: Value =
                serde_json::from_str(new["question_json"].as_str().unwrap()).unwrap();
            for field in ["target", "dimension", "expected", "context", "audioKind"] {
                assert_eq!(old_question[field], new_question[field]);
            }
        }
        let asset = document.tables["media_assets"][0]["id"].as_str().unwrap();
        assert_eq!(
            digest(&std::fs::read(second.store.media_file(asset).unwrap()).unwrap()),
            digest(&audio)
        );
        let stable = first
            .store
            .synchronize(&scope, &mut remote, &cancel, |_, _| {})
            .unwrap();
        assert_eq!(stable.pushed, 0);
        let mut deleted = saved.bundle.clone();
        deleted.deleted = true;
        assert!(matches!(
            remote
                .inner
                .push(&Push {
                    change_id: Uuid::new_v4().to_string(),
                    base_revision: 1,
                    bundle: deleted
                })
                .unwrap(),
            PushReply::Saved { i: 2 }
        ));
        second
            .store
            .synchronize(&scope, &mut receiver, &cancel, |_, _| {})
            .unwrap();
        let conflict = second
            .store
            .synchronization_conflicts(&scope)
            .unwrap()
            .into_iter()
            .find(|c| c.remote_id == entry)
            .unwrap();
        second
            .store
            .synchronization_resolve(
                &scope,
                &ResolutionInput {
                    conflict_id: conflict.id,
                    expected_remote_revision: conflict.remote_revision,
                    expected_local_revision: conflict
                        .local
                        .as_ref()
                        .and_then(|b| b.entry["revision"].as_i64()),
                    choice: "archive".into(),
                    target_entry_id: None,
                    expected_target_revision: None,
                },
                &mut receiver,
                &cancel,
            )
            .unwrap();
        assert_eq!(
            second.store.sync_export(&entry).unwrap().tables["review_attempts"].len(),
            3000
        );
        println!(
            "{}",
            json!({"bundleBytes":size,"answers":3000,"collections":1,"audioDigestMatched":true,"lostReceiptRetried":true,"archivedFixture":true})
        );
    }

    struct InterruptedRemote {
        inner: HttpRemote,
        lose_ack: bool,
        lose_download: bool,
    }
    impl Remote for InterruptedRemote {
        fn changes(&mut self, after: i64) -> Result<ChangePage, AppError> {
            self.inner.changes(after)
        }
        fn change(&mut self, change: &RemoteChange) -> Result<RemoteDocument, AppError> {
            self.inner.change(change)
        }
        fn upload(&mut self, hash: &str, format: &str, bytes: Vec<u8>) -> Result<(), AppError> {
            self.inner.upload(hash, format, bytes)
        }
        fn push(&mut self, request: &Push) -> Result<PushReply, AppError> {
            let reply = self.inner.push(request)?;
            if self.lose_ack && matches!(reply, PushReply::Saved { .. }) {
                self.lose_ack = false;
                return Err(failure(
                    "network_error",
                    "Controlled lost acknowledgement after a real server commit",
                ));
            }
            Ok(reply)
        }
        fn download(&mut self, hash: &str, format: &str) -> Result<Vec<u8>, AppError> {
            let bytes = self.inner.download(hash, format)?;
            if self.lose_download {
                self.lose_download = false;
                return Err(failure(
                    "network_error",
                    "Controlled interrupted real media response",
                ));
            }
            Ok(bytes)
        }
    }
    fn application(root: &std::path::Path, credential: &SiteCredential) -> Arc<Application> {
        let store = Arc::new(Store::open(root).unwrap());
        credentials::save(
            root,
            "https://english-copy-practice.hxwjb.chatgpt.site",
            credential,
        )
        .unwrap();
        let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
        Arc::new(Application::new(
            store,
            tasks,
            Settings {
                offline_mode: false,
                ..Default::default()
            },
        ))
    }
    #[test]
    #[ignore = "Requires an explicitly approved real connection and private audio fixture"]
    fn actual_receipt_loss_media_interruption_and_tombstone_keep_history() {
        let credential_root = PathBuf::from(
            std::env::var_os("SVL_LIVE_CREDENTIAL_ROOT").expect("approved private credential root"),
        );
        let test_root = PathBuf::from(
            std::env::var_os("SVL_LIVE_TEST_ROOT").expect("dedicated private integration root"),
        );
        let audio = PathBuf::from(
            std::env::var_os("SVL_LIVE_AUDIO_FIXTURE").expect("private original clip"),
        );
        assert!(!test_root.exists(), "Use a fresh, dedicated test directory");
        let credential = credentials::load(
            &credential_root,
            "https://english-copy-practice.hxwjb.chatgpt.site",
        )
        .unwrap()
        .expect("real approval");
        let first = application(&test_root.join("first"), &credential);
        let bytes = std::fs::read(audio).unwrap();
        let hash = digest(&bytes);
        let asset = Uuid::new_v4().to_string();
        std::fs::write(first.store.root().join("media/live-clip.m4a"), bytes).unwrap();
        first.store.connection().unwrap().execute("INSERT INTO media_assets(id,recipe_key,digest,relative_path,kind,format,duration_ms,state,created_at) VALUES (?,? ,?,'media/live-clip.m4a','original','m4a',1710,'ready',?)",rusqlite::params![asset,hash,hash,chrono::Utc::now().timestamp_millis()]).unwrap();
        let entry = first
            .store
            .collect(&CollectionInput {
                operation_id: Uuid::new_v4().to_string(),
                kind: "word".into(),
                text: format!("svl-recovery-fixture-{}", Uuid::new_v4()),
                meaning: "恢复验证".into(),
                examples: vec![ExampleInput {
                    text: "Kids, breakfast!".into(),
                    media_asset_ids: vec![asset],
                    ..Default::default()
                }],
                target_entry_id: None,
                expected_revision: None,
            })
            .unwrap()
            .entry_id;
        let unit = first
            .store
            .review_units("all", "meaning", 0, 100)
            .unwrap()
            .into_iter()
            .find(|u| u.entry_id == entry)
            .unwrap();
        let question = first
            .store
            .review_question(&unit.id, unit.revision)
            .unwrap();
        first
            .store
            .review_submit(&AnswerInput {
                operation_id: Uuid::new_v4().to_string(),
                question_id: question.id,
                answer: "恢复验证".into(),
                unable: false,
            })
            .unwrap();
        let mut remote = InterruptedRemote {
            inner: HttpRemote::new(Arc::clone(&first)).unwrap(),
            lose_ack: true,
            lose_download: false,
        };
        remote.inner.account().unwrap();
        let scope = remote.inner.scope();
        let cancel = AtomicBool::new(false);
        assert_eq!(
            first
                .store
                .synchronize(&scope, &mut remote, &cancel, |_, _| {})
                .unwrap_err()
                .code,
            "network_error"
        );
        assert_eq!(
            first.store.synchronization_status(&scope).unwrap().pending,
            1
        );
        first
            .store
            .synchronize(&scope, &mut remote, &cancel, |_, _| {})
            .unwrap();
        let stable = first
            .store
            .synchronize(&scope, &mut remote, &cancel, |_, _| {})
            .unwrap();
        assert_eq!(stable.pushed, 0);
        let document: RemoteDocument = serde_json::from_value(
            remote
                .inner
                .get(&format!("/api/svl/v1/entries/{entry}"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(document.revision, 1);
        assert_eq!(document.bundle.tables["collection_actions"].len(), 1);
        assert_eq!(document.bundle.tables["review_attempts"].len(), 1);
        let second = application(&test_root.join("second"), &credential);
        let mut receiving = InterruptedRemote {
            inner: HttpRemote::new(Arc::clone(&second)).unwrap(),
            lose_ack: false,
            lose_download: true,
        };
        assert_eq!(
            second
                .store
                .synchronize(&scope, &mut receiving, &cancel, |_, _| {})
                .unwrap_err()
                .code,
            "network_error"
        );
        let cursor_before = second.store.synchronization_status(&scope).unwrap().cursor;
        second
            .store
            .synchronize(&scope, &mut receiving, &cancel, |_, _| {})
            .unwrap();
        assert!(second.store.synchronization_status(&scope).unwrap().cursor > cursor_before);
        let peer = second.store.get_entry(&entry).unwrap();
        assert_eq!(peer.collection_count, 1);
        assert_eq!(peer.examples[0].audio.len(), 1);
        assert_eq!(
            digest(
                &std::fs::read(
                    second
                        .store
                        .media_file(&peer.examples[0].audio[0].id)
                        .unwrap()
                )
                .unwrap()
            ),
            hash
        );
        let mut removed = document.bundle;
        removed.deleted = true;
        assert!(matches!(
            remote
                .inner
                .push(&Push {
                    change_id: Uuid::new_v4().to_string(),
                    base_revision: document.revision,
                    bundle: removed
                })
                .unwrap(),
            PushReply::Saved { .. }
        ));
        first
            .store
            .synchronize(&scope, &mut remote, &cancel, |_, _| {})
            .unwrap();
        let conflict = first
            .store
            .synchronization_conflicts(&scope)
            .unwrap()
            .into_iter()
            .find(|c| c.remote_id == entry)
            .unwrap();
        first
            .store
            .synchronization_resolve(
                &scope,
                &ResolutionInput {
                    conflict_id: conflict.id,
                    choice: "archive".into(),
                    target_entry_id: None,
                    expected_remote_revision: conflict.remote_revision,
                    expected_local_revision: conflict
                        .local
                        .as_ref()
                        .and_then(|b| b.entry["revision"].as_i64()),
                    expected_target_revision: None,
                },
                &mut remote,
                &cancel,
            )
            .unwrap();
        assert_eq!(first.store.get_entry(&entry).unwrap().collection_count, 1);
        assert!(
            !first
                .store
                .list_entries("", 0, 100)
                .unwrap()
                .iter()
                .any(|e| e.id == entry)
        );
        assert_eq!(
            first
                .store
                .review_history(Some(&unit.id), 0, 20)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    #[ignore = "Requires the approved real account, assessment-1 deployment and a private original clip"]
    fn actual_assessment_complete_history_roundtrip() {
        use crate::{assessment::*, assessment_bank::*, assessment_reports::*};
        use rusqlite::{Connection, OptionalExtension, params};
        let credential_root = PathBuf::from(
            std::env::var_os("SVL_LIVE_CREDENTIAL_ROOT").expect("approved credential directory"),
        );
        let root = PathBuf::from(
            std::env::var_os("SVL_ASSESSMENT_LIVE_ROOT")
                .expect("dedicated persistent test directory"),
        );
        let audio = std::fs::read(
            std::env::var_os("SVL_LIVE_AUDIO_FIXTURE").expect("previously confirmed original clip"),
        )
        .unwrap();
        let credential = credentials::load(
            &credential_root,
            "https://english-copy-practice.hxwjb.chatgpt.site",
        )
        .unwrap()
        .expect("existing real approval");
        std::fs::create_dir_all(root.join("source/dictionaries")).unwrap();
        let reference = root.join("source/dictionaries/base.sqlite3");
        if !reference.exists() {
            let c = Connection::open(reference).unwrap();
            c.execute_batch("CREATE TABLE dictionary_entries(word TEXT,meaning TEXT);CREATE TABLE dictionary_aliases(form TEXT,lemma TEXT);CREATE TABLE metadata(key TEXT,value TEXT);INSERT INTO metadata VALUES('source','TC-58 transport fixture');CREATE TABLE frequency_catalogs(id TEXT,kind TEXT,title TEXT,source_url TEXT,source_version TEXT,source_sha256 TEXT,rank_unit TEXT,raw_rows INTEGER,max_rank INTEGER,notice TEXT);INSERT INTO frequency_catalogs VALUES('tc58','phrase','TC-58 传输验收合成范围','https://github.com/ananba1985/SubtitleVocabularyList','fixture-1','fixture','fixture_rank',1,1,'仅用于真实传输技术验收，不代表词频排名或用户水平');CREATE TABLE frequency_targets(catalog_id TEXT,rank INTEGER,text TEXT,target_key TEXT,lexeme_key TEXT,mapping_basis TEXT,frequency INTEGER,eligible INTEGER,seed_json TEXT);INSERT INTO frequency_targets VALUES('tc58',1,'Kids, breakfast!','kids, breakfast!','kids, breakfast!','identity',NULL,1,NULL);").unwrap();
        }
        let source = application(&root.join("source"), &credential);
        let existing: Option<String> = source
            .store
            .connection()
            .unwrap()
            .query_row(
                "SELECT value_json FROM settings WHERE key='tc58_session'",
                [],
                |r| r.get(0),
            )
            .optional()
            .unwrap();
        let session_id = if let Some(id) = existing {
            serde_json::from_str::<String>(&id).unwrap()
        } else {
            std::fs::write(source.store.root().join("media/tc58-original.m4a"), &audio).unwrap();
            let bank = Uuid::new_v4().to_string();
            let sources = source.store.assessment_catalogs().unwrap();
            source.store.connection().unwrap().execute("INSERT INTO assessment_banks(id,state,sources_json,policy_version,created_at) VALUES (?,'draft',?,?,?)",params![bank,serde_json::to_string(&sources).unwrap(),POLICY,chrono::Utc::now().timestamp_millis()]).unwrap();
            let target = source
                .store
                .assessment_targets("tc58", 1)
                .unwrap()
                .remove(0);
            source
                .store
                .assessment_save_candidates(
                    &bank,
                    &target,
                    &[(
                        "cloze".into(),
                        QuestionDraft {
                            target: target.text.clone(),
                            meaning: "孩子们，吃早饭啦！".into(),
                            sentence: "[ Claire ]\nKids, breakfast!".into(),
                            translation: "（克莱尔）孩子们，吃早饭啦！".into(),
                            prompt: "[ Claire ]\n___".into(),
                            choices: vec![],
                            correct_option_id: None,
                            accepted_answers: vec!["Kids, breakfast!".into()],
                            explanation:
                                "TC-58 传输技术夹具，沿用之前已确认的剧集片段；不作为用户水平测量。"
                                    .into(),
                            origin: "TC-58 transport fixture, Modern Family S01E01".into(),
                            audio: Some(AssessmentAudio {
                                kind: "original".into(),
                                relative_path: "media/tc58-original.m4a".into(),
                                sha256: digest(&audio),
                                text: "Kids, breakfast!".into(),
                                voice_id: String::new(),
                                asset_id: None,
                            }),
                        },
                    )],
                )
                .unwrap();
            source.store.connection().unwrap().execute("UPDATE assessment_items SET state='approved',review_note='TC-58 pre-reviewed transport fixture' WHERE bank_id=?",[&bank]).unwrap();
            source
                .store
                .assessment_bank_review(&ReviewBank {
                    operation_id: Uuid::new_v4().to_string(),
                    bank_id: bank.clone(),
                    expected_revision: source
                        .store
                        .assessment_bank_summary(&bank)
                        .unwrap()
                        .revision,
                    action: "publish".into(),
                    item_id: None,
                    expected_item_revision: None,
                    content: None,
                    note: "仅作为技术传输夹具，原声来源与之前确认的例句一致。".into(),
                })
                .unwrap();
            let config = AssessmentConfig {
                content: "phrase".into(),
                phrase_top_n: 1,
                question_count: 1,
                question_types: vec!["cloze".into()],
                ..Default::default()
            };
            let preview = source.store.assessment_preview(&config).unwrap();
            let started = source
                .store
                .assessment_start(&StartAssessment {
                    config,
                    expected_inventory_revision: preview.inventory_revision,
                    operation_id: Uuid::new_v4().to_string(),
                })
                .unwrap();
            let session = source
                .store
                .assessment_answer_submit(&AssessmentAnswerInput {
                    session_id: started.id.clone(),
                    item_id: started.question.unwrap().id,
                    expected_revision: started.revision,
                    operation_id: Uuid::new_v4().to_string(),
                    action: "answer".into(),
                    option_id: None,
                    text: "Kids, breakfast".into(),
                    uncertain: true,
                })
                .unwrap();
            source.store.assessment_report(&session.id, None).unwrap();
            let feedback = session.last_feedback.as_ref().unwrap();
            source
                .store
                .assessment_correct(&CorrectAssessment {
                    answer_id: feedback.answer_id.clone(),
                    expected_revision: feedback.answer_revision,
                    operation_id: Uuid::new_v4().to_string(),
                    result: "correct".into(),
                    add_hint: false,
                    reason: "技术夹具：本题可接受省略句尾感叹号，保留原回答和不确定事实。".into(),
                })
                .unwrap();
            let draft = source
                .store
                .assessment_collection_prepare(&PrepareAssessmentCollection {
                    session_id: session.id.clone(),
                    item_ids: vec![feedback.session_item_id.clone()],
                    operation_id: Uuid::new_v4().to_string(),
                })
                .unwrap();
            let candidate = &draft.items[0];
            source
                .store
                .assessment_collect_item(
                    &draft.draft_id,
                    &CollectionSelection {
                        session_item_id: candidate.session_item_id.clone(),
                        target_entry_id: None,
                        expected_revision: None,
                        meaning: candidate.meaning.clone(),
                        translation: candidate.translation.clone(),
                        save_audio: true,
                    },
                )
                .unwrap();
            source
                .store
                .connection()
                .unwrap()
                .execute(
                    "INSERT INTO settings(key,value_json) VALUES ('tc58_session',?)",
                    [serde_json::to_string(&session.id).unwrap()],
                )
                .unwrap();
            session.id
        };
        fn sync(app: &Arc<Application>) {
            let task = app.sync_start(Uuid::new_v4().to_string()).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
            loop {
                let t = app.tasks.get(&task.id).unwrap();
                if t.terminal() {
                    assert_eq!(t.state, "succeeded", "sync error: {:?}", t.error);
                    let a = &t.result.as_ref().unwrap()["assessment"];
                    assert_eq!(a["capability"], "assessment-1");
                    assert_eq!(a["conflicts"], 0);
                    assert_eq!(a["pending"], 0);
                    assert_eq!(a["pendingReferences"], 0);
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "normal sync did not finish"
                );
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        sync(&source);
        let peer = application(&root.join("peer"), &credential);
        sync(&peer);
        let peer_session = peer.store.assessment_session(&session_id).unwrap();
        let feedback = peer_session.last_feedback.unwrap();
        let correction_done: bool = peer
            .store
            .connection()
            .unwrap()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM settings WHERE key='tc58_peer_corrected')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if !correction_done {
            peer.store.assessment_correct(&CorrectAssessment{answer_id:feedback.answer_id,expected_revision:feedback.answer_revision,operation_id:Uuid::new_v4().to_string(),result:"invalid".into(),add_hint:false,reason:"TC-58：此记录为同步技术夹具，不计入学习成绩；从第二客户端保存修正并经真实网站回传。".into()}).unwrap();
            peer.store.assessment_report(&session_id, None).unwrap();
            peer.store
                .connection()
                .unwrap()
                .execute(
                    "INSERT INTO settings(key,value_json) VALUES ('tc58_peer_corrected','true')",
                    [],
                )
                .unwrap();
        }
        sync(&peer);
        sync(&source);
        let report = source.store.assessment_report(&session_id, None).unwrap();
        assert_eq!(report["report"]["counts"]["unscored"], 1);
        assert_eq!(report["report"]["counts"]["valid"], 0);
        let source_answer = source
            .store
            .assessment_session(&session_id)
            .unwrap()
            .last_feedback
            .unwrap();
        assert_eq!(
            source_answer.unscored_reason.as_deref(),
            Some("invalid_item")
        );
        assert!(source_answer.uncertain);
        let c = source.store.connection().unwrap();
        let raw: String = c
            .query_row(
                "SELECT answer_json FROM assessment_answers WHERE id=?",
                [&source_answer.answer_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&raw).unwrap()["result"],
            "needs_confirmation"
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM assessment_corrections WHERE answer_id=?",
                [&source_answer.answer_id],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            2
        );
        drop(c);
        let receiver = application(&root.join("receiver"), &credential);
        sync(&receiver);
        let received = receiver.store.assessment_report(&session_id, None).unwrap();
        assert_eq!(received["report"]["counts"], report["report"]["counts"]);
        let c = receiver.store.connection().unwrap();
        let links:usize=c.query_row("SELECT COUNT(*) FROM assessment_collection_links WHERE session_item_id IN (SELECT id FROM assessment_session_items WHERE session_id=?)",[&session_id],|r|r.get(0)).unwrap();
        assert_eq!(links, 1);
        let audio_path:String=c.query_row("SELECT json_extract(snapshot_json,'$.content.audio.relativePath') FROM assessment_session_items WHERE session_id=?",[&session_id],|r|r.get(0)).unwrap();
        drop(c);
        assert_eq!(
            digest(&std::fs::read(receiver.store.root().join(audio_path)).unwrap()),
            digest(&audio)
        );
        println!(
            "TC-58 real assessment roundtrip: {session_id}; immutable original answer, 2 corrections, versioned reports, collection reference and original audio retained. Test data: {}",
            root.display()
        );
    }
}
