//! Per-user Windows logon registration. No preference is copied with the notebook.
use crate::error::AppError;
use std::path::Path;
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        System::Registry::*,
    },
    core::{PCWSTR, w},
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn failure(action: &str, error: WIN32_ERROR) -> AppError {
    AppError::new(
        "autostart_failed",
        format!(
            "{action}失败（Windows 错误 {}），请检查当前用户的启动项权限。",
            error.0
        ),
    )
}

fn read_at(path: &str) -> Result<Option<String>, AppError> {
    let path = wide(path);
    let mut data = [0u16; 261];
    let mut bytes = std::mem::size_of_val(&data) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            w!("SubtitleVocabularyList"),
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if status != ERROR_SUCCESS {
        return Err(failure("读取开机启动项", status));
    }
    let length = bytes as usize / 2;
    let data = &data[..length];
    let text = String::from_utf16(data.strip_suffix(&[0]).unwrap_or(data))
        .map_err(|_| AppError::new("autostart_failed", "开机启动项包含无法识别的路径。"))?;
    Ok(Some(text))
}

fn write_at(path: &str, command: Option<&str>) -> Result<(), AppError> {
    let path = wide(path);
    let mut key = HKEY::default();
    let status = unsafe {
        if command.is_some() {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        } else {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                KEY_SET_VALUE,
                &mut key,
            )
        }
    };
    if status == ERROR_FILE_NOT_FOUND && command.is_none() {
        return Ok(());
    }
    if status != ERROR_SUCCESS {
        return Err(failure("打开开机启动项", status));
    }
    let key = Key(key);
    let status = if let Some(command) = command {
        let data: Vec<u8> = wide(command)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        unsafe {
            RegSetValueExW(
                key.0,
                w!("SubtitleVocabularyList"),
                None,
                REG_SZ,
                Some(&data),
            )
        }
    } else {
        unsafe { RegDeleteValueW(key.0, w!("SubtitleVocabularyList")) }
    };
    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND && command.is_none() {
        Ok(())
    } else {
        Err(failure("更新开机启动项", status))
    }
}

fn quoted_path(path: &Path) -> Result<String, AppError> {
    let text = dunce::simplified(path)
        .to_str()
        .filter(|v| !v.contains(['"', '\0']))
        .ok_or_else(|| AppError::new("autostart_failed", "程序或资料目录路径无法用于开机启动。"))?;
    // Windows command-line parsing doubles trailing slashes before a closing quote.
    let trailing = text.chars().rev().take_while(|c| *c == '\\').count();
    Ok(format!("\"{text}{}\"", "\\".repeat(trailing)))
}

fn command(executable: &Path, data: &Path) -> Result<String, AppError> {
    let value = format!(
        "{} --data-dir {}",
        quoted_path(executable)?,
        quoted_path(data)?
    );
    if value.encode_utf16().count() > 260 {
        return Err(AppError::new(
            "autostart_failed",
            "程序与资料目录路径过长，超过 Windows 启动项的 260 字符限制，请使用较短路径。",
        ));
    }
    Ok(value)
}

pub(crate) fn enabled() -> Result<bool, AppError> {
    Ok(read_at(RUN_KEY)?.is_some_and(|value| !value.trim().is_empty()))
}

pub(crate) fn set_enabled(enabled: bool, data: &Path) -> Result<bool, AppError> {
    let value = enabled
        .then(|| command(&std::env::current_exe()?, data))
        .transpose()?;
    write_at(RUN_KEY, value.as_deref())?;
    self::enabled()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn logon_registration_keeps_data_path_and_can_be_disabled() {
        let path = format!(
            r"Software\SubtitleVocabularyList.AutostartTest-{}",
            uuid::Uuid::new_v4()
        );
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let path = wide(&self.0);
                let _ = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr())) };
            }
        }
        let _cleanup = Cleanup(path.clone());
        let data = Path::new(r"D:\学习资料\Notebook Data");
        let value = command(
            Path::new(r"C:\Program Files\SubtitleVocabularyList\SubtitleVocabularyList.exe"),
            data,
        )
        .unwrap();
        assert_eq!(
            value,
            "\"C:\\Program Files\\SubtitleVocabularyList\\SubtitleVocabularyList.exe\" --data-dir \"D:\\学习资料\\Notebook Data\""
        );
        assert!(read_at(&path).unwrap().is_none());
        write_at(&path, Some(&value)).unwrap();
        assert_eq!(read_at(&path).unwrap().as_deref(), Some(value.as_str()));
        assert_eq!(
            crate::app_paths::startup_data_directory([OsString::from("--data-dir"), data.into()])
                .unwrap()
                .as_deref(),
            Some(data)
        );
        write_at(&path, None).unwrap();
        assert!(read_at(&path).unwrap().is_none());
    }
}
