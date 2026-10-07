use crate::{error::AppError, vocabulary::digest};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    },
    core::{PCWSTR, w},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteCredential {
    pub request_id: String,
    pub token: String,
    pub device_name: String,
    pub account_scope: Option<String>,
    pub display_code: Option<String>,
    pub expires_at: i64,
}
impl std::fmt::Debug for SiteCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SiteCredential")
            .field("request_id", &self.request_id)
            .field("token", &"[redacted]")
            .finish_non_exhaustive()
    }
}

fn credential_path(root: &Path, site: &str) -> PathBuf {
    root.join("credentials")
        .join(format!("site-{}.bin", digest(site.as_bytes())))
}

fn transform(bytes: &[u8], protect: bool) -> Result<Vec<u8>, AppError> {
    if bytes.is_empty() || bytes.len() > 65536 {
        return Err(AppError::new("invalid_input", "连接凭据大小无效。"));
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // Current-user DPAPI, with no prompts and no machine-wide flag.
    let result = unsafe {
        if protect {
            CryptProtectData(
                &input,
                w!("SubtitleVocabularyList site connection"),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    result.map_err(|_| {
        AppError::new(
            "credentials_unavailable",
            "本机当前用户无法读取或保存站点连接，请重新连接账号。",
        )
    })?;
    let result = if output.pbData.is_null() || output.cbData == 0 {
        Err(AppError::new(
            "credentials_unavailable",
            "站点连接凭据无效。",
        ))
    } else {
        Ok(unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec())
    };
    if !output.pbData.is_null() {
        unsafe {
            let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        }
    }
    result
}

pub fn load(root: &Path, site: &str) -> Result<Option<SiteCredential>, AppError> {
    let path = credential_path(root, site);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() > 65536 {
        return Err(AppError::new(
            "credentials_unavailable",
            "站点连接文件无效，请重新连接。",
        ));
    }
    let plaintext = transform(&bytes, false)?;
    let credential: SiteCredential = serde_json::from_slice(&plaintext)
        .map_err(|_| AppError::new("credentials_unavailable", "站点连接文件无效，请重新连接。"))?;
    Ok(Some(credential))
}

pub fn save(root: &Path, site: &str, credential: &SiteCredential) -> Result<(), AppError> {
    let path = credential_path(root, site);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let encrypted = transform(&serde_json::to_vec(credential)?, true)?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&temporary, encrypted)?;
    let result = std::fs::rename(&temporary, &path);
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    Ok(())
}

pub fn open_authorization(site: &str, request_id: &str) -> Result<(), AppError> {
    if uuid::Uuid::parse_str(request_id).is_err() {
        return Err(AppError::new("invalid_input", "连接请求标识无效。"));
    }
    let origin =
        reqwest::Url::parse(site).map_err(|_| AppError::new("invalid_input", "站点地址无效。"))?;
    if origin.scheme() != "https" || origin.username() != "" || origin.password().is_some() {
        return Err(AppError::new(
            "invalid_input",
            "站点连接需要有效的 HTTPS 地址。",
        ));
    }
    let url = format!(
        "{}/desktop-connect?request={request_id}",
        origin.origin().ascii_serialization()
    );
    let wide: Vec<u16> = url.encode_utf16().chain([0]).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err(AppError::new(
            "provider_unavailable",
            "无法打开系统浏览器，请手动打开连接页。",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_current_user_dpapi_roundtrip_keeps_plaintext_off_disk() {
        let directory = tempfile::tempdir().unwrap();
        let site = "https://fixture.example";
        let credential = SiteCredential {
            request_id: uuid::Uuid::new_v4().to_string(),
            token: "synthetic-secret-not-a-real-credential".into(),
            device_name: "fixture".into(),
            account_scope: Some("fixture-user".into()),
            display_code: None,
            expires_at: 123,
        };
        save(directory.path(), site, &credential).unwrap();
        let file = std::fs::read(credential_path(directory.path(), site)).unwrap();
        assert!(
            !file
                .windows(credential.token.len())
                .any(|part| part == credential.token.as_bytes())
        );
        assert_eq!(
            load(directory.path(), site).unwrap().unwrap().token,
            credential.token
        );
        assert!(
            load(directory.path(), "https://other.example")
                .unwrap()
                .is_none()
        );
        save(directory.path(), site, &credential).unwrap();
        std::fs::write(credential_path(directory.path(), site), b"damaged").unwrap();
        assert_eq!(
            load(directory.path(), site).unwrap_err().code,
            "credentials_unavailable"
        );
    }
}
