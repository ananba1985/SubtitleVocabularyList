use crate::{
    application::Application,
    credentials::{self, SiteCredential},
    error::AppError,
    site_connection::site_origin,
    synchronization::{
        ChangePage, Push, PushReply, Remote, RemoteDocument, SyncConflict, SyncStatus,
    },
    tasks::TaskSnapshot,
    vocabulary::digest,
};
use reqwest::blocking::{Client, Response};
use serde_json::{Value, json};
use std::{io::Read, sync::Arc};

struct HttpRemote {
    app: Arc<Application>,
    site: String,
    credential: SiteCredential,
    client: Client,
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
fn read_json(response: Response) -> Result<(u16, Value), AppError> {
    let status = response.status().as_u16();
    let mut bytes = vec![];
    response
        .take(1_010_001)
        .read_to_end(&mut bytes)
        .map_err(|_| failure("network_error", "站点响应中断，待同步内容保留。"))?;
    if bytes.len() > 1_010_000 {
        return Err(failure("invalid_data", "同步响应过大，已保留当前资料。"));
    }
    if status == 401 || status == 403 || (300..400).contains(&status) {
        return Err(failure(
            "auth_required",
            "站点账号连接需要重新批准，本地内容与同步进度保留。",
        ));
    }
    if !matches!(status, 200..=299 | 409) {
        return Err(failure(
            "network_error",
            &format!("同步未完成（HTTP {status}），本地资料与待处理内容保留。"),
        ));
    }
    let value = serde_json::from_slice(&bytes)
        .map_err(|_| failure("invalid_data", "站点同步响应不是有效资料。"))?;
    Ok((status, value))
}
impl HttpRemote {
    fn new(app: Arc<Application>) -> Result<Self, AppError> {
        let settings = app.settings()?;
        if settings.offline_mode {
            return Err(failure(
                "offline_mode",
                "当前为离线模式，已有资料仍可本地学习。",
            ));
        }
        let site = site_origin(&settings.site_url)?;
        let credential = credentials::load(app.store.root(), &site)?
            .filter(|c| {
                c.account_scope.is_some() && c.expires_at > chrono::Utc::now().timestamp_millis()
            })
            .ok_or_else(|| failure("auth_required", "请先由浏览器批准本机的站点账号连接。"))?;
        Ok(Self {
            app,
            site,
            credential,
            client: crate::site_connection::desktop_client()?,
        })
    }
    fn scope(&self) -> String {
        format!(
            "{}|{}",
            self.site,
            self.credential.account_scope.as_deref().unwrap()
        )
    }
    fn check(&self) -> Result<(), AppError> {
        let settings = self.app.settings()?;
        if settings.offline_mode {
            return Err(failure(
                "offline_mode",
                "已切换为离线模式，同步内容与进度保留。",
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
    fn account(&mut self) -> Result<(), AppError> {
        self.check()?;
        let (_, v) = read_json(
            self.client
                .get(format!("{}/api/svl/v1/account", self.site))
                .bearer_auth(&self.credential.token)
                .send()
                .map_err(|_| failure("network_error", "无法检查同步账号，本地内容保留。"))?,
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
        Ok(())
    }
    fn get(&mut self, path: &str) -> Result<Value, AppError> {
        self.check()?;
        let (_, v) = read_json(
            self.client
                .get(format!("{}{path}", self.site))
                .bearer_auth(&self.credential.token)
                .send()
                .map_err(|_| failure("network_error", "拉取未完成，本地资料与游标保留。"))?,
        )?;
        self.check()?;
        Ok(v)
    }
}
impl Remote for HttpRemote {
    fn changes(&mut self, after: i64) -> Result<ChangePage, AppError> {
        Ok(serde_json::from_value(
            self.get(&format!("/api/svl/v1/changes?after={after}"))?,
        )?)
    }
    fn change(&mut self, cursor: i64) -> Result<RemoteDocument, AppError> {
        Ok(serde_json::from_value(
            self.get(&format!("/api/svl/v1/changes/{cursor}"))?,
        )?)
    }
    fn push(&mut self, request: &Push) -> Result<PushReply, AppError> {
        self.check()?;
        if serde_json::to_vec(request)?.len() > 996_000 {
            return Err(failure(
                "capacity_exceeded",
                "此词条完整资料超过站点当前容量，未删减任何例句、原声或历史。",
            ));
        }
        let (status, v) = read_json(
            self.client
                .put(format!(
                    "{}/api/svl/v1/entries/{}",
                    self.site,
                    request.bundle.entry["id"].as_str().unwrap()
                ))
                .bearer_auth(&self.credential.token)
                .json(request)
                .send()
                .map_err(|_| {
                    failure(
                        "network_error",
                        "推送响应丢失或中断，已冻结的同一内容可安全重试。",
                    )
                })?,
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
        if bytes.len() > 20 * 1024 * 1024 || digest(&bytes) != hash {
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
            .map_err(|_| failure("network_error", "无法检查站点原声，待同步记录保留。"))?;
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
            return Ok(());
        }
        if head.status().as_u16() != 404 {
            read_json(head)?;
            return Err(failure("network_error", "站点原声状态无效。"));
        }
        let size = bytes.len();
        let (_, v) = read_json(
            self.client
                .put(format!("{url}?format={format}"))
                .bearer_auth(&self.credential.token)
                .header("content-type", mime(format))
                .body(bytes)
                .send()
                .map_err(|_| failure("network_error", "原声上传中断，待同步内容保留。"))?,
        )?;
        self.check()?;
        if v["digest"].as_str() != Some(hash)
            || v["size"].as_u64() != Some(size as u64)
            || v["format"].as_str() != Some(format)
        {
            return Err(failure("invalid_data", "原声上传回执无效。"));
        }
        Ok(())
    }
    fn download(&mut self, hash: &str, format: &str) -> Result<Vec<u8>, AppError> {
        self.check()?;
        let response = self
            .client
            .get(format!("{}/api/svl/v1/media/{hash}", self.site))
            .bearer_auth(&self.credential.token)
            .send()
            .map_err(|_| failure("network_error", "原声下载中断，游标保留。"))?;
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
        response
            .take(20 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| failure("network_error", "原声下载中断，游标保留。"))?;
        self.check()?;
        Ok(bytes)
    }
}
impl Application {
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
    pub fn sync_start(self: &Arc<Self>, operation: String) -> Result<TaskSnapshot, AppError> {
        let mut remote = HttpRemote::new(Arc::clone(self))?;
        let scope = remote.scope();
        let store = Arc::clone(&self.store);
        let guard = Arc::clone(&self.sync_guard);
        let hash = digest(scope.as_bytes());
        self.tasks.start("sync", &operation, &hash, move |context| {
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
            Ok(serde_json::to_value(result)?)
        })
    }
    pub fn sync_resolve(
        self: &Arc<Self>,
        input: crate::synchronization::ResolutionInput,
        operation: String,
    ) -> Result<TaskSnapshot, AppError> {
        let mut remote = HttpRemote::new(Arc::clone(self))?;
        let scope = remote.scope();
        let store = Arc::clone(&self.store);
        let guard = Arc::clone(&self.sync_guard);
        let hash = digest(serde_json::to_vec(&json!([scope, input]))?.as_slice());
        self.tasks
            .start("sync_resolve", &operation, &hash, move |context| {
                let _guard = guard
                    .try_lock()
                    .map_err(|_| failure("sync_busy", "请等待已有同步结束再处理。"))?;
                context.check_cancelled()?;
                remote.account()?;
                store.synchronization_resolve(&scope, &input, &mut remote, &context.cancelled)?;
                Ok(json!({"resolved":true}))
            })
    }
}
