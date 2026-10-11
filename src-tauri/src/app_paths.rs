use std::{io, path::PathBuf};

/// Test builds share this machine's chosen installation data by default.
pub fn data_directory() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("SVL_DATA_DIR").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(path);
        return if path.is_absolute() {
            Ok(path)
        } else {
            Ok(std::env::current_dir()?.join(path))
        };
    }
    #[cfg(debug_assertions)]
    {
        Ok(PathBuf::from(r"D:\SubtitleVocabularyList\Data"))
    }
    #[cfg(not(debug_assertions))]
    {
        Ok(std::env::current_exe()?
            .parent()
            .ok_or_else(|| io::Error::other("Cannot locate the application directory"))?
            .join("Data"))
    }
}

pub fn webview_directory(root: &std::path::Path) -> PathBuf {
    root.join("cache").join("webview")
}

pub fn prepare_process(root: &std::path::Path) -> io::Result<()> {
    let temporary = root.join("cache").join("temp");
    std::fs::create_dir_all(&temporary)?;
    std::fs::create_dir_all(webview_directory(root))?;
    // Called before Tauri creates windows or starts application worker threads.
    // Only this process and its child tools inherit these temporary directories.
    unsafe {
        std::env::set_var("TEMP", &temporary);
        std::env::set_var("TMP", &temporary);
    }
    Ok(())
}
