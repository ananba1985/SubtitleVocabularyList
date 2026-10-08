use crate::error::AppError;

pub fn site_origin(value: &str) -> Result<String, AppError> {
    let url =
        reqwest::Url::parse(value).map_err(|_| AppError::new("invalid_input", "站点地址无效。"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(AppError::new(
            "invalid_input",
            "请输入站点的 HTTPS 首页地址，不包含路径、查询或凭据。",
        ));
    }
    Ok(url.origin().ascii_serialization())
}

#[cfg(all(windows, feature = "desktop"))]
mod desktop {
    use super::*;
    use crate::{
        application::Application,
        credentials::{self, SiteCredential},
        tasks::TaskSnapshot,
        vocabulary::digest,
    };
    use chrono::Utc;
    use reqwest::blocking::Client;
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use std::{io::Read, sync::Arc, time::Duration};
    use uuid::Uuid;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ConnectionStatus {
        pub site_url: String,
        pub state: String,
        pub offline: bool,
        pub device_name: String,
        pub request_id: Option<String>,
        pub display_code: Option<String>,
        pub authorization_url: Option<String>,
        pub account_scope: Option<String>,
        pub expires_at: i64,
    }

    fn status(site: &str, offline: bool, credential: Option<&SiteCredential>) -> ConnectionStatus {
        let now = Utc::now().timestamp_millis();
        ConnectionStatus {
            site_url: site.into(),
            offline,
            device_name: credential
                .map(|c| c.device_name.clone())
                .unwrap_or_default(),
            state: credential
                .map(|c| {
                    if c.expires_at <= now {
                        "expired"
                    } else if c.account_scope.is_some() {
                        "connected"
                    } else {
                        "pending"
                    }
                })
                .unwrap_or("disconnected")
                .into(),
            request_id: credential.map(|c| c.request_id.clone()),
            display_code: credential.and_then(|c| c.display_code.clone()),
            authorization_url: credential
                .filter(|c| c.account_scope.is_none())
                .map(|c| format!("{site}/desktop-connect?request={}", c.request_id)),
            account_scope: credential.and_then(|c| c.account_scope.clone()),
            expires_at: credential.map(|c| c.expires_at).unwrap_or(0),
        }
    }
    pub(crate) fn client() -> Result<Client, AppError> {
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| AppError::new("network_error", "无法准备站点连接。"))
    }
    pub(crate) fn read_response(response: reqwest::blocking::Response) -> Result<Value, AppError> {
        let status = response.status();
        let mut bytes = vec![];
        response
            .take(1_000_001)
            .read_to_end(&mut bytes)
            .map_err(|_| AppError::new("network_unavailable", "站点响应中断，本地内容保留。"))?;
        if bytes.len() > 1_000_000 {
            return Err(AppError::new(
                "invalid_data",
                "站点响应过大，请重试或检查站点版本。",
            ));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status.is_redirection() {
            return Err(AppError::new(
                "auth_required",
                "站点需要重新登录或批准连接，本地学习仍可使用。",
            ));
        }
        if !status.is_success() {
            return Err(AppError::new(
                if status == reqwest::StatusCode::CONFLICT {
                    "conflict"
                } else {
                    "network_error"
                },
                format!(
                    "站点操作未完成（HTTP {}），请重试；本地内容保留。",
                    status.as_u16()
                ),
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| AppError::new("invalid_data", "站点没有返回有效数据，本地内容保留。"))
    }
    fn require_online(app: &Application) -> Result<String, AppError> {
        let settings = app.settings()?;
        if settings.offline_mode {
            return Err(AppError::new(
                "offline_mode",
                "已启用手动离线，请在本地设置取消手动离线并保存后再连接站点。",
            ));
        }
        site_origin(&settings.site_url)
    }

    impl Application {
        pub fn connection_status(&self) -> Result<ConnectionStatus, AppError> {
            let settings = self.settings()?;
            let site = site_origin(&settings.site_url)?;
            let credential = credentials::load(self.store.root(), &site)?;
            Ok(status(&site, settings.offline_mode, credential.as_ref()))
        }
        pub fn connection_start(
            &self,
            operation_id: String,
            replace: bool,
        ) -> Result<TaskSnapshot, AppError> {
            if Uuid::parse_str(&operation_id).is_err() {
                return Err(AppError::new("invalid_input", "连接操作标识无效。"));
            }
            let _guard = self
                .connection_guard
                .lock()
                .map_err(|_| AppError::new("internal_error", "站点连接状态不可用。"))?;
            let site = require_online(self)?;
            let previous = if replace {
                None
            } else {
                credentials::load(self.store.root(), &site)?
            };
            if previous.as_ref().is_some_and(|c| {
                c.account_scope.is_some() && c.expires_at > Utc::now().timestamp_millis()
            }) {
                return Err(AppError::new(
                    "conflict",
                    "已有连接。可以检查连接，或明确选择重新连接账号。",
                ));
            }
            let credential = previous
                .filter(|c| c.expires_at > Utc::now().timestamp_millis())
                .unwrap_or_else(|| SiteCredential {
                    request_id: Uuid::new_v4().to_string(),
                    token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
                    device_name: std::env::var("COMPUTERNAME")
                        .unwrap_or_else(|_| "Windows desktop".into()),
                    account_scope: None,
                    display_code: None,
                    expires_at: Utc::now().timestamp_millis() + 600_000,
                });
            // Save before the request so a lost response can retry the same request and secret.
            credentials::save(self.store.root(), &site, &credential)?;
            let hash = digest(format!("{site}:{}", credential.request_id).as_bytes());
            let store = Arc::clone(&self.store);
            let guard = Arc::clone(&self.connection_guard);
            self.start_network_task("site_connection",&operation_id,&hash,move|context| {
                context.check_cancelled()?;context.progress("connection",0,1,"正在准备真实站点账号连接");
                let response=client()?.post(format!("{site}/api/desktop/requests")).json(&json!({"requestId":credential.request_id,"tokenHash":digest(credential.token.as_bytes()),"deviceName":credential.device_name})).send().map_err(|_|AppError::new("network_unavailable","无法连接站点，请稍后重试；连接请求与本地内容已保留。"))?;
                let value=read_response(response)?;context.check_cancelled()?;
                let mut credential=credential;
                credential.display_code=Some(value["displayCode"].as_str().filter(|code|code.len()==8 && code.is_ascii()).ok_or_else(||AppError::new("invalid_data","站点连接码无效。"))?.into());
                credential.expires_at=value["expiresAt"].as_i64().ok_or_else(||AppError::new("invalid_data","站点连接日期无效。"))?;
                let _guard=guard.lock().map_err(|_|AppError::new("internal_error","站点连接状态不可用。"))?;
                if credentials::load(store.root(),&site)?.is_none_or(|current|current.request_id!=credential.request_id){return Err(AppError::new("conflict","连接请求已变化，请检查当前请求。"));}
                credentials::save(store.root(),&site,&credential)?;
                Ok(serde_json::to_value(status(&site,false,Some(&credential)))?)
            })
        }
        pub fn connection_check(&self, operation_id: String) -> Result<TaskSnapshot, AppError> {
            let site = require_online(self)?;
            let credential = credentials::load(self.store.root(), &site)?
                .ok_or_else(|| AppError::new("auth_required", "请先发起站点连接。"))?;
            let store = Arc::clone(&self.store);
            let guard = Arc::clone(&self.connection_guard);
            let hash = digest(format!("{site}:{}:check", credential.request_id).as_bytes());
            self.start_network_task(
                "site_connection_check",
                &operation_id,
                &hash,
                move |context| {
                    context.check_cancelled()?;
                    context.progress("authentication", 0, 1, "正在检查站点连接批准状态");
                    let value = read_response(
                        client()?
                            .get(format!("{site}/api/desktop/status"))
                            .bearer_auth(&credential.token)
                            .send()
                            .map_err(|_| {
                                AppError::new(
                                    "network_unavailable",
                                    "无法检查站点连接，本地内容保留。",
                                )
                            })?,
                    )?;
                    context.check_cancelled()?;
                    let mut credential = credential;
                    let server_state = value["status"]
                        .as_str()
                        .ok_or_else(|| AppError::new("invalid_data", "站点连接状态无效。"))?;
                    if server_state == "approved" {
                        credential.account_scope = Some(
                            value["accountScope"]
                                .as_str()
                                .filter(|scope| !scope.is_empty() && scope.len() <= 200)
                                .ok_or_else(|| {
                                    AppError::new("invalid_data", "站点未返回已认证账号范围。")
                                })?
                                .into(),
                        );
                        credential.expires_at = value["expiresAt"]
                            .as_i64()
                            .ok_or_else(|| AppError::new("invalid_data", "站点连接日期无效。"))?;
                        credential.display_code = None;
                    } else if matches!(server_state, "expired" | "rejected" | "revoked") {
                        credential.expires_at = 0;
                        credential.account_scope = None;
                    } else if server_state != "pending" {
                        return Err(AppError::new("invalid_data", "站点返回未知连接状态。"));
                    }
                    let _guard = guard
                        .lock()
                        .map_err(|_| AppError::new("internal_error", "站点连接状态不可用。"))?;
                    if credentials::load(store.root(), &site)?
                        .is_none_or(|current| current.request_id != credential.request_id)
                    {
                        return Err(AppError::new(
                            "conflict",
                            "连接请求已变化，请检查当前请求。",
                        ));
                    }
                    credentials::save(store.root(), &site, &credential)?;
                    let mut result = status(&site, false, Some(&credential));
                    if server_state != "approved" {
                        result.state = server_state.into();
                    }
                    Ok(serde_json::to_value(result)?)
                },
            )
        }
        pub fn connection_open(&self) -> Result<(), AppError> {
            let site = require_online(self)?;
            let credential = credentials::load(self.store.root(), &site)?
                .ok_or_else(|| AppError::new("auth_required", "请先发起站点连接。"))?;
            credentials::open_authorization(&site, &credential.request_id)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{application::Settings, store::Store, tasks::TaskManager};
        #[test]
        fn offline_connection_commands_make_no_task_or_credential() {
            let directory = tempfile::tempdir().unwrap();
            let store = Arc::new(Store::open(directory.path()).unwrap());
            let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
            let app = Application::new(
                store,
                Arc::clone(&tasks),
                Settings {
                    offline_mode: true,
                    ..Default::default()
                },
            );
            assert_eq!(
                app.connection_start(Uuid::new_v4().to_string(), false)
                    .unwrap_err()
                    .code,
                "offline_mode"
            );
            assert_eq!(
                app.connection_check(Uuid::new_v4().to_string())
                    .unwrap_err()
                    .code,
                "offline_mode"
            );
            assert_eq!(app.connection_open().unwrap_err().code, "offline_mode");
            assert!(tasks.list().unwrap().is_empty());
            assert!(!directory.path().join("credentials").exists());
        }
        #[test]
        fn public_connection_status_never_returns_the_secret() {
            let directory = tempfile::tempdir().unwrap();
            let store = Arc::new(Store::open(directory.path()).unwrap());
            let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
            let app = Application::new(store, tasks, Settings::default());
            let site = site_origin(&app.settings().unwrap().site_url).unwrap();
            let credential = SiteCredential {
                request_id: Uuid::new_v4().to_string(),
                token: "synthetic-private-secret".into(),
                device_name: "fixture".into(),
                account_scope: Some("fixture-user".into()),
                display_code: None,
                expires_at: Utc::now().timestamp_millis() + 600000,
            };
            credentials::save(directory.path(), &site, &credential).unwrap();
            let status = serde_json::to_string(&app.connection_status().unwrap()).unwrap();
            assert!(!status.contains(&credential.token));
            assert!(status.contains("fixture-user"));
        }
    }
}
#[cfg(all(windows, feature = "desktop"))]
pub use desktop::ConnectionStatus;
#[cfg(all(windows, feature = "desktop"))]
pub(crate) use desktop::client as desktop_client;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connection_origin_rejects_credentials_paths_and_insecure_urls() {
        assert_eq!(
            site_origin("https://fixture.example/").unwrap(),
            "https://fixture.example"
        );
        for url in [
            "http://fixture.example",
            "https://secret@fixture.example",
            "https://fixture.example/data",
            "https://fixture.example/?token=x",
            "https://fixture.example/#x",
            "file:///fixture",
        ] {
            assert!(site_origin(url).is_err(), "{url}");
        }
    }
}
