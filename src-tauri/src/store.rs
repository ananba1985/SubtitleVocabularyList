use crate::error::AppError;
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::Duration,
};

pub struct Store {
    root: PathBuf,
    connection: Mutex<Connection>,
}

impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, AppError> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join("media"))?;
        std::fs::create_dir_all(root.join("jobs"))?;
        let mut connection = Connection::open(root.join("vocabulary.sqlite3"))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > 1 {
            return Err(AppError::new(
                "unsupported_schema",
                "此词库由较新版本创建，请使用对应软件版本打开。",
            ));
        }
        if version == 0 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/001_initial.sql"))?;
            transaction.pragma_update(None, "user_version", 1)?;
            transaction.commit()?;
        }
        Ok(Self {
            root,
            connection: Mutex::new(connection),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn connection(&self) -> Result<MutexGuard<'_, Connection>, AppError> {
        self.connection.lock().map_err(|_| {
            AppError::new(
                "database_unavailable",
                "本地词库当前不可用，请重新打开应用。",
            )
        })
    }

    pub fn schema_version(&self) -> Result<i64, AppError> {
        Ok(self
            .connection()?
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_migrates_once_and_enforces_relationships() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), 1);
        assert_eq!(
            store
                .connection()
                .unwrap()
                .pragma_query_value(None, "foreign_keys", |row| row.get::<_, i32>(0))
                .unwrap(),
            1
        );
        let result = store.connection().unwrap().execute("INSERT INTO meanings (id, entry_id, text, origin, created_at) VALUES ('m', 'missing', 'meaning', 'user', 0)", []);
        assert!(result.is_err());
        drop(store);
        assert_eq!(
            Store::open(directory.path())
                .unwrap()
                .schema_version()
                .unwrap(),
            1
        );
    }

    #[test]
    fn newer_schema_is_not_silently_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        store
            .connection()
            .unwrap()
            .pragma_update(None, "user_version", 20)
            .unwrap();
        drop(store);
        assert!(
            matches!(Store::open(directory.path()), Err(error) if error.code == "unsupported_schema")
        );
    }
}
