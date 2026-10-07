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
        if version > 3 {
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
        if version < 2 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/002_imports.sql"))?;
            transaction.pragma_update(None, "user_version", 2)?;
            transaction.commit()?;
        }
        if version < 3 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/003_reviews.sql"))?;
            transaction.pragma_update(None, "user_version", 3)?;
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
        assert_eq!(store.schema_version().unwrap(), 3);
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
            3
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

    #[test]
    fn version_one_upgrade_keeps_existing_vocabulary() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("vocabulary.sqlite3")).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        connection.execute("INSERT INTO entries(id,kind,text,match_key,created_at,updated_at) VALUES ('existing','word','reluctant','reluctant',1,1)",[]).unwrap();
        drop(connection);
        let store = Store::open(directory.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), 3);
        assert_eq!(store.get_entry("existing").unwrap().text, "reluctant");
    }

    #[test]
    fn version_two_upgrade_keeps_existing_units_examples_and_actions() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("vocabulary.sqlite3")).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/002_imports.sql"))
            .unwrap();
        connection.execute_batch("INSERT INTO entries(id,kind,text,match_key,created_at,updated_at) VALUES ('e','word','reluctant','reluctant',100,100);
            INSERT INTO meanings(id,entry_id,text,origin,created_at) VALUES ('m','e','不情愿的','user',100);
            INSERT INTO examples(id,location_key,identity_key,text,created_at) VALUES ('x','manual','x','She was reluctant.',100);
            INSERT INTO entry_examples(entry_id,example_id,meaning_id) VALUES ('e','x','m');
            INSERT INTO collection_actions(operation_id,request_hash,entry_id,result_json,created_at) VALUES ('op','hash','e','{}',100);
            INSERT INTO learning_units(id,entry_id,scope_key,dimension) VALUES ('u','e','m','meaning');
            INSERT INTO review_states(unit_id,state_json,due_at,relearn_at) VALUES ('u','{}',100,100);").unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        drop(connection);
        let store = Store::open(directory.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), 3);
        let entry = store.get_entry("e").unwrap();
        assert_eq!(entry.collection_count, 1);
        assert_eq!(entry.examples[0].text, "She was reluctant.");
        let units = store.review_units("all", "meaning", 0, 50).unwrap();
        assert_eq!(units[0].id, "u");
        assert_eq!(units[0].state.due_at, 100);
        assert!(store.review_question("u", units[0].revision).is_ok());
    }
}
