use crate::{error::AppError, store::Store, vocabulary::digest};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

const KEY: &str = "query_providers_v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginField {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol_version: usize,
    pub min_host_version: String,
    pub license: String,
    pub capabilities: Vec<String>,
    pub allowed_hosts: Vec<String>,
    pub endpoint: String,
    pub method: String,
    #[serde(default)]
    pub fields: Vec<PluginField>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub body: Value,
    #[serde(default)]
    pub meaning_pointer: String,
    #[serde(default)]
    pub translation_pointer: String,
    #[serde(default)]
    pub notes_pointer: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub auto_query: bool,
    pub send_context: bool,
    pub order: usize,
    pub max_concurrency: usize,
    pub network_scope: String,
    #[serde(default)]
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub credential_fields: Vec<String>,
    pub manifest: Option<PluginManifest>,
}

impl Provider {
    pub fn supports_translation(&self) -> bool {
        matches!(self.kind.as_str(), "qwen" | "translation")
            || self
                .manifest
                .as_ref()
                .is_some_and(|m| m.capabilities.iter().any(|c| c == "translation"))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUpdate {
    pub id: String,
    pub enabled: bool,
    pub auto_query: bool,
    pub send_context: bool,
    pub order: usize,
    pub max_concurrency: usize,
    #[serde(default)]
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub secrets: HashMap<String, String>,
}

pub fn builtin() -> Vec<Provider> {
    [
        ("qwen", "本地模型", "loopback"),
        ("dictionary", "Wiktionary 英语词典", "external"),
        ("translation", "MyMemory 中文翻译", "external"),
    ]
    .iter()
    .enumerate()
    .map(|(order, (id, name, scope))| Provider {
        id: (*id).into(),
        name: (*name).into(),
        kind: (*id).into(),
        enabled: *id == "qwen",
        auto_query: *id == "qwen",
        send_context: *id == "qwen",
        order,
        max_concurrency: 1,
        network_scope: (*scope).into(),
        values: HashMap::new(),
        credential_fields: vec![],
        manifest: None,
    })
    .collect()
}

fn version(value: &str) -> Option<(u32, u32, u32)> {
    let parts = value
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (parts.len() == 3).then(|| (parts[0], parts[1], parts[2]))
}

pub fn validate_manifest(manifest: &PluginManifest) -> Result<String, AppError> {
    let valid_id = |value: &str| {
        !value.is_empty()
            && value.len() <= 80
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
    };
    if !valid_id(&manifest.id)
        || ["qwen", "dictionary", "translation"].contains(&manifest.id.as_str())
        || manifest.name.trim().is_empty()
        || manifest.name.chars().count() > 100
        || manifest.protocol_version != 1
        || version(&manifest.version).is_none()
        || version(&manifest.min_host_version)
            .is_none_or(|v| v > version(env!("CARGO_PKG_VERSION")).unwrap())
        || manifest.license.trim().is_empty()
        || !matches!(manifest.method.as_str(), "GET" | "POST")
        || manifest.capabilities.is_empty()
        || manifest
            .capabilities
            .iter()
            .any(|v| !matches!(v.as_str(), "translation" | "dictionary"))
        || manifest.fields.len() > 20
        || (manifest.meaning_pointer.is_empty() && manifest.translation_pointer.is_empty())
    {
        return Err(AppError::new(
            "invalid_input",
            "插件版本、标识、能力或响应映射不受支持。",
        ));
    }
    let mut fields = HashSet::new();
    if manifest
        .fields
        .iter()
        .any(|f| !valid_id(&f.id) || !fields.insert(&f.id) || f.label.is_empty())
        || [
            &manifest.meaning_pointer,
            &manifest.translation_pointer,
            &manifest.notes_pointer,
        ]
        .iter()
        .any(|p| !p.is_empty() && !p.starts_with('/'))
    {
        return Err(AppError::new(
            "invalid_input",
            "插件配置字段或 JSON Pointer 无效。",
        ));
    }
    let url = reqwest::Url::parse(&manifest.endpoint)
        .map_err(|_| AppError::new("invalid_input", "插件服务地址无效。"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || manifest.endpoint.contains("{{")
        || url.fragment().is_some()
        || !manifest
            .allowed_hosts
            .iter()
            .any(|host| Some(host.as_str()) == url.host_str())
    {
        return Err(AppError::new(
            "invalid_input",
            "插件地址必须属于声明的主机，且不能包含凭据或模板。",
        ));
    }
    let local = matches!(
        url.host_str(),
        Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
    );
    if url.scheme() != "https" && !(local && url.scheme() == "http") {
        return Err(AppError::new(
            "invalid_input",
            "外部插件服务必须使用 HTTPS，本机服务可使用 HTTP。",
        ));
    }
    Ok(if local { "loopback" } else { "external" }.into())
}

impl Store {
    pub fn providers(&self) -> Result<Vec<Provider>, AppError> {
        let value: Option<String> = self
            .connection()?
            .query_row("SELECT value_json FROM settings WHERE key=?", [KEY], |r| {
                r.get(0)
            })
            .optional()?;
        let mut providers = value
            .map(|v| serde_json::from_str(&v))
            .transpose()?
            .unwrap_or_else(builtin);
        providers.sort_by_key(|p| p.order);
        Ok(providers)
    }
    fn save_providers(&self, providers: &[Provider]) -> Result<(), AppError> {
        self.connection()?.execute("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",params![KEY,serde_json::to_string(providers)?])?;
        Ok(())
    }
    pub fn provider_update(&self, update: ProviderUpdate) -> Result<Vec<Provider>, AppError> {
        if !(1..=16).contains(&update.max_concurrency) || update.order > 1000 {
            return Err(AppError::new(
                "invalid_input",
                "服务并发数应为 1～16，顺序应在 0～1000 内。",
            ));
        }
        let mut providers = self.providers()?;
        let provider = providers
            .iter_mut()
            .find(|p| p.id == update.id)
            .ok_or_else(|| AppError::new("not_found", "服务不存在。"))?;
        if let Some(manifest) = &provider.manifest {
            validate_manifest(manifest)?;
            if update
                .values
                .keys()
                .any(|key| !manifest.fields.iter().any(|f| !f.secret && &f.id == key))
                || update
                    .secrets
                    .keys()
                    .any(|key| !manifest.fields.iter().any(|f| f.secret && &f.id == key))
            {
                return Err(AppError::new("invalid_input", "配置包含插件未声明的字段。"));
            }
            let mut secrets = load_secrets(self.root(), &provider.id)?;
            for (key, value) in update.secrets {
                if value.is_empty() {
                    secrets.remove(&key);
                } else {
                    secrets.insert(key, value);
                }
            }
            if update.enabled
                && manifest.fields.iter().any(|f| {
                    f.required
                        && if f.secret {
                            secrets.get(&f.id).is_none_or(|v| v.trim().is_empty())
                        } else {
                            update.values.get(&f.id).is_none_or(|v| v.trim().is_empty())
                        }
                })
            {
                return Err(AppError::new("invalid_input", "请先填写插件必填配置。"));
            }
            save_secrets(self.root(), &provider.id, &secrets)?;
            provider.credential_fields = secrets.keys().cloned().collect();
        }
        provider.enabled = update.enabled;
        provider.auto_query = update.auto_query;
        provider.send_context = update.send_context;
        provider.order = update.order;
        provider.max_concurrency = update.max_concurrency;
        provider.values = update.values;
        self.save_providers(&providers)?;
        self.providers()
    }
    pub fn plugin_install_manifest(&self, manifest: PluginManifest) -> Result<Provider, AppError> {
        let scope = validate_manifest(&manifest)?;
        let mut providers = self.providers()?;
        let existing = providers.iter().find(|p| p.id == manifest.id).cloned();
        let mut provider = Provider {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            kind: "plugin".into(),
            enabled: false,
            auto_query: false,
            send_context: false,
            order: providers.len(),
            max_concurrency: 1,
            network_scope: scope,
            values: HashMap::new(),
            credential_fields: vec![],
            manifest: Some(manifest),
        };
        if let Some(old) = existing {
            provider.order = old.order;
            provider.values = old.values;
            provider.credential_fields = old.credential_fields;
        }
        providers.retain(|p| p.id != provider.id);
        providers.push(provider.clone());
        self.save_providers(&providers)?;
        Ok(provider)
    }
    pub fn plugin_remove(&self, id: &str) -> Result<Vec<Provider>, AppError> {
        let mut providers = self.providers()?;
        if !providers.iter().any(|p| p.id == id && p.kind == "plugin") {
            return Err(AppError::new("invalid_input", "仅可卸载已安装插件。"));
        }
        providers.retain(|p| p.id != id);
        self.save_providers(&providers)?;
        let path = secret_path(self.root(), id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(providers)
    }
}

fn secret_path(root: &Path, id: &str) -> std::path::PathBuf {
    root.join("credentials")
        .join(format!("query-{}.bin", digest(id.as_bytes())))
}

pub fn load_secrets(root: &Path, id: &str) -> Result<HashMap<String, String>, AppError> {
    let path = secret_path(root, id);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    #[cfg(all(windows, feature = "desktop"))]
    {
        let bytes = std::fs::read(path)?;
        return Ok(serde_json::from_slice(&crate::credentials::transform(
            &bytes, false,
        )?)?);
    }
    #[cfg(not(all(windows, feature = "desktop")))]
    {
        Err(AppError::new(
            "credentials_unavailable",
            "服务凭据需要 Windows 本机运行环境。",
        ))
    }
}
fn save_secrets(root: &Path, id: &str, secrets: &HashMap<String, String>) -> Result<(), AppError> {
    if secrets.is_empty() {
        let path = secret_path(root, id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    #[cfg(all(windows, feature = "desktop"))]
    {
        let path = secret_path(root, id);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let data = crate::credentials::transform(&serde_json::to_vec(secrets)?, true)?;
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temporary, data)?;
        if let Err(error) = std::fs::rename(&temporary, &path) {
            let _ = std::fs::remove_file(temporary);
            return Err(error.into());
        }
        Ok(())
    }
    #[cfg(not(all(windows, feature = "desktop")))]
    {
        Err(AppError::new(
            "credentials_unavailable",
            "服务凭据需要 Windows 本机运行环境。",
        ))
    }
}

pub fn substitute(value: &Value, fields: &HashMap<String, String>) -> Result<Value, AppError> {
    Ok(match value {
        Value::String(s) => {
            let pattern = regex::Regex::new(r"\{\{([A-Za-z0-9_.:-]+)\}\}").unwrap();
            let mut missing = false;
            let replaced = pattern
                .replace_all(s, |capture: &regex::Captures<'_>| {
                    fields.get(&capture[1]).cloned().unwrap_or_else(|| {
                        missing = true;
                        String::new()
                    })
                })
                .into_owned();
            if missing {
                return Err(AppError::new("invalid_input", "插件模板包含未配置的字段。"));
            }
            json!(replaced)
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|v| substitute(v, fields))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(k, v)| Ok((k.clone(), substitute(v, fields)?)))
                .collect::<Result<_, AppError>>()?,
        ),
        _ => value.clone(),
    })
}
