//! Device preferences and explicit ownership of the Android local library.
//! The existing sync engine, account scopes and data model remain authoritative.
use crate::{application::Application, credentials, error::AppError, site_connection::site_origin, store::Store};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct DeviceSyncPreferences {
    pub device_name: String,
    pub auto_sync: bool,
}
impl Default for DeviceSyncPreferences {
    fn default() -> Self { Self { device_name: "Android 手机".into(), auto_sync: false } }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DeviceNetwork {
    pub connected: bool,
    pub metered: bool,
    pub foreground: bool,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DeviceSyncState {
    pub preferences: DeviceSyncPreferences,
    pub bound_account: Option<String>,
    pub current_account: Option<String>,
    pub binding_required: bool,
    pub network: DeviceNetwork,
    pub last_completed_at: Option<i64>,
    pub ready_audio: usize,
    pub missing_audio: usize,
}

fn read<T: DeserializeOwned>(store: &Store, key: &str) -> Result<Option<T>, AppError> {
    let value: Option<String> = store.connection()?.query_row(
        "SELECT value_json FROM settings WHERE key=?", [key], |row| row.get(0),
    ).optional()?;
    value.map(|value| Ok(serde_json::from_str(&value)?)).transpose()
}
fn write<T: Serialize>(store: &Store, key: &str, value: &T) -> Result<(), AppError> {
    store.connection()?.execute(
        "INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",
        rusqlite::params![key, serde_json::to_string(value)?],
    )?;
    Ok(())
}
pub(crate) fn preferences(store: &Store) -> Result<DeviceSyncPreferences, AppError> {
    Ok(read(store, "android.syncPreferences")?.unwrap_or_default())
}
pub(crate) fn device_name(app: &Application) -> Result<String, AppError> {
    #[cfg(target_os = "android")]
    { Ok(preferences(&app.store)?.device_name) }
    #[cfg(not(target_os = "android"))]
    { let _ = app; Ok(std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows desktop".into())) }
}
pub(crate) fn network() -> Result<DeviceNetwork, AppError> {
    #[cfg(target_os = "android")]
    { crate::mobile_platform::network() }
    #[cfg(not(target_os = "android"))]
    { Ok(DeviceNetwork { connected: true, metered: false, foreground: true }) }
}
#[cfg(target_os = "android")]
pub(crate) fn require_bound(store: &Store, account: &str) -> Result<(), AppError> {
    if read::<String>(store, "android.libraryAccount")?.as_deref() != Some(account) {
        return Err(AppError::new("account_binding_required", "请先在“我的”确认本机全部学习资料属于当前获批账号，再同步。"));
    }
    Ok(())
}
/// Checked immediately before each real audio request; never discard audio to advance a cursor.
#[cfg(target_os = "android")]
pub(crate) fn require_audio_network() -> Result<(), AppError> {
    let state = network()?;
    if !state.connected { return Err(AppError::new("network_unavailable", "当前没有可用网络，原声和待同步进度保留。")); }
    if state.metered { return Err(AppError::new("wifi_required", "原声等待 Wi-Fi 等不计费网络；本地资料和未完成同步进度保留，连接后可继续同步。")); }
    Ok(())
}
#[cfg(target_os = "android")]
pub(crate) fn completed(store: &Store, account: &str) -> Result<(), AppError> {
    write(store, &format!("android.syncCompleted:{account}"), &chrono::Utc::now().timestamp_millis())
}

impl Application {
    pub fn device_sync_state(&self) -> Result<DeviceSyncState, AppError> {
        let site = site_origin(&self.settings()?.site_url)?;
        let current_account = credentials::load(self.store.root(), &site)?
            .filter(|c| c.expires_at > chrono::Utc::now().timestamp_millis())
            .and_then(|c| c.account_scope.map(|scope| format!("{site}|{scope}")));
        let bound_account: Option<String> = read(&self.store, "android.libraryAccount")?;
        let last_completed_at = current_account.as_ref()
            .map(|account| read(&self.store, &format!("android.syncCompleted:{account}")))
            .transpose()?.flatten();
        let (ready_audio, missing_audio) = self.store.connection()?.query_row(
            "SELECT COUNT(CASE WHEN state='ready' THEN 1 END), COUNT(CASE WHEN state='missing' THEN 1 END) FROM media_assets",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(DeviceSyncState {
            preferences: preferences(&self.store)?,
            binding_required: current_account.is_some() && current_account != bound_account,
            bound_account, current_account, network: network()?, last_completed_at, ready_audio, missing_audio,
        })
    }
    pub fn device_sync_update(&self, mut input: DeviceSyncPreferences) -> Result<DeviceSyncPreferences, AppError> {
        input.device_name = input.device_name.trim().into();
        if input.device_name.is_empty() || input.device_name.chars().count() > 80 || input.device_name.chars().any(char::is_control) {
            return Err(AppError::new("invalid_input", "请输入 1 至 80 字的设备名称。"));
        }
        write(&self.store, "android.syncPreferences", &input)?;
        Ok(input)
    }
    pub fn device_sync_bind(&self, expected_account: String) -> Result<DeviceSyncState, AppError> {
        let _guard = self.sync_guard.try_lock().map_err(|_| AppError::new("sync_busy", "请等当前同步结束再确认资料归属。"))?;
        let _connection = self.connection_guard.lock().map_err(|_| AppError::new("internal_error", "账号状态不可用。"))?;
        let state = self.device_sync_state()?;
        if state.current_account.as_deref() != Some(&expected_account) {
            return Err(AppError::new("connection_changed", "获批账号已变化，请刷新后核对。"));
        }
        write(&self.store, "android.libraryAccount", &expected_account)?;
        self.device_sync_state()
    }
    pub fn connection_disconnect(&self) -> Result<(), AppError> {
        let _guard = self.sync_guard.try_lock().map_err(|_| AppError::new("sync_busy", "请先结束当前同步，再断开账号。"))?;
        let _connection = self.connection_guard.lock().map_err(|_| AppError::new("internal_error", "账号状态不可用。"))?;
        let site = site_origin(&self.settings()?.site_url)?;
        credentials::remove(self.store.root(), &site)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::{application::Settings, credentials::SiteCredential, tasks::TaskManager, vocabulary::CollectionInput};
    use std::sync::Arc;
    #[test]
    fn device_sync_normal_binding_preferences_and_disconnect_preserve_library() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
        let app = Application::new(Arc::clone(&store), tasks, Settings::default());
        let saved = store.collect(&CollectionInput {
            operation_id: uuid::Uuid::new_v4().to_string(), kind: "word".into(), text: "reluctant".into(),
            meaning: "不情愿的".into(), examples: vec![], target_entry_id: None, expected_revision: None,
        }).unwrap();
        let site = site_origin(&app.settings().unwrap().site_url).unwrap();
        credentials::save(store.root(), &site, &SiteCredential {
            request_id: uuid::Uuid::new_v4().to_string(), token: "synthetic-device-token".into(),
            device_name: "fixture".into(), account_scope: Some("fixture-account".into()), display_code: None,
            expires_at: chrono::Utc::now().timestamp_millis() + 600_000,
        }).unwrap();
        let account = format!("{site}|fixture-account");
        assert!(app.device_sync_state().unwrap().binding_required);
        let state = app.device_sync_bind(account.clone()).unwrap();
        assert_eq!(state.bound_account.as_deref(), Some(account.as_str()));
        assert!(!state.binding_required);
        app.device_sync_update(DeviceSyncPreferences { device_name: "学习手机".into(), auto_sync: true }).unwrap();
        assert!(app.device_sync_state().unwrap().preferences.auto_sync);
        app.connection_disconnect().unwrap();
        assert!(app.device_sync_state().unwrap().current_account.is_none());
        assert_eq!(app.device_sync_state().unwrap().bound_account, Some(account));
        assert_eq!(store.get_entry(&saved.entry_id).unwrap().text, "reluctant");
        assert!(credentials::load(store.root(), &site).unwrap().is_none());
    }
}
