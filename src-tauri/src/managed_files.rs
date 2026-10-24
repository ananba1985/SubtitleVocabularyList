use crate::error::AppError;
use std::{
    fs,
    path::{Component, Path},
};

fn error(message: impl Into<String>) -> AppError {
    AppError::new("storage_unavailable", message)
}

pub(crate) fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

/// The caller supplies the canonical, selected Data root, never a workspace root.
pub(crate) fn owned_path(root: &Path, path: &Path) -> Result<(), AppError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| error("资料路径超出选定目录。"))?;
    let mut current = root.to_path_buf();
    for component in std::iter::once(Component::CurDir).chain(relative.components()) {
        match component {
            Component::CurDir => (),
            Component::Normal(name) => current.push(name),
            _ => return Err(error("资料路径含非预期的上级目录。")),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if is_link(&metadata) || !dunce::canonicalize(&current)?.starts_with(root) {
                    return Err(error(format!(
                        "路径包含目录链接，已保留：{}",
                        current.display()
                    )));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

pub(crate) fn plain_tree(root: &Path, path: &Path) -> Result<(), AppError> {
    owned_path(root, path)?;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|e| error(e.to_string()))?;
        if is_link(&fs::symlink_metadata(entry.path())?) {
            return Err(error(format!("目录中含链接，已保留：{}", path.display())));
        }
    }
    Ok(())
}
