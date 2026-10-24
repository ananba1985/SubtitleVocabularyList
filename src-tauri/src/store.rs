use crate::error::AppError;
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard},
    time::Duration,
};

pub struct Store {
    root: PathBuf,
    connection: Mutex<Connection>,
    dictionary: RwLock<crate::base_dictionary::BaseDictionary>,
    pub(crate) assessment_packet: Mutex<Option<Arc<crate::assessment_sync::PreparedPacket>>>,
    pub(crate) subtitle_guard: Mutex<()>,
}

pub struct ConnectionGuard<'a> {
    connection: MutexGuard<'a, Connection>,
    _held: crate::local_profile::Timer,
}
impl Deref for ConnectionGuard<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.connection
    }
}
impl DerefMut for ConnectionGuard<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
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
        if version > 10 {
            return Err(AppError::new(
                "unsupported_schema",
                "此词库由较新版本创建，请使用对应软件版本打开。",
            ));
        }
        #[cfg(any(all(windows, feature = "desktop"), test))]
        if version > 0 && version < 10 {
            crate::data_maintenance::before_schema_upgrade(&root, &connection)?;
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
        if version < 4 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/004_example_contexts.sql"))?;
            transaction.pragma_update(None, "user_version", 4)?;
            transaction.commit()?;
        }
        if version < 5 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/005_sync.sql"))?;
            transaction.pragma_update(None, "user_version", 5)?;
            transaction.commit()?;
        }
        if version < 6 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/006_explanations.sql"))?;
            transaction.pragma_update(None, "user_version", 6)?;
            transaction.commit()?;
        }
        if version < 7 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/007_query_results.sql"))?;
            transaction.pragma_update(None, "user_version", 7)?;
            transaction.commit()?;
        }
        if version < 8 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/008_sentence_translations.sql"))?;
            transaction.pragma_update(None, "user_version", 8)?;
            transaction.commit()?;
        }
        if version < 9 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/009_assessments.sql"))?;
            transaction.pragma_update(None, "user_version", 9)?;
            transaction.commit()?;
        }
        if version < 10 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/010_sync_growth.sql"))?;
            transaction.pragma_update(None, "user_version", 10)?;
            transaction.commit()?;
        }
        Ok(Self {
            assessment_packet: Mutex::new(None),
            subtitle_guard: Mutex::new(()),
            dictionary: RwLock::new(crate::base_dictionary::BaseDictionary::load(&root)),
            root,
            connection: Mutex::new(connection),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    #[cfg(any(all(windows, feature = "desktop"), test))]
    pub(crate) fn maintain_after_startup(&self) -> Result<usize, AppError> {
        crate::data_maintenance::after_startup(self)
    }

    pub fn dictionary_warnings(&self) -> Vec<String> {
        self.dictionary.read().map(|dictionary| dictionary.warnings())
            .unwrap_or_else(|_| vec!["离线词库当前不可用，请重新打开应用。".into()])
    }

    pub(crate) fn dictionary_explanation(&self, text: &str) -> Result<Option<crate::explanations::Explanation>, AppError> {
        self.base_dictionary()?.explanation(text)
    }

    pub(crate) fn base_dictionary(&self) -> Result<RwLockReadGuard<'_, crate::base_dictionary::BaseDictionary>, AppError> {
        self.dictionary.read()
            .map_err(|_| AppError::new("resource_missing", "离线词库当前不可用，请重新打开应用。"))
    }

    #[cfg(all(target_os = "android", feature = "mobile"))]
    pub(crate) fn reload_base_dictionary(&self) -> Result<(), AppError> {
        let dictionary = crate::base_dictionary::BaseDictionary::load(&self.root);
        *self.dictionary.write()
            .map_err(|_| AppError::new("resource_missing", "离线词库当前不可用，请重新打开应用。"))? = dictionary;
        Ok(())
    }

    pub fn connection(&self) -> Result<ConnectionGuard<'_>, AppError> {
        let waiting = crate::local_profile::phase("db.lock_wait");
        let connection = self.connection.lock().map_err(|_| {
            AppError::new(
                "database_unavailable",
                "本地词库当前不可用，请重新打开应用。",
            )
        })?;
        drop(waiting);
        Ok(ConnectionGuard {
            connection,
            _held: crate::local_profile::phase("db.hold"),
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
    fn sync_growth_version_nine_upgrade_keeps_learning_history() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("media")).unwrap();
        let connection = Connection::open(directory.path().join("vocabulary.sqlite3")).unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        for migration in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_imports.sql"),
            include_str!("../migrations/003_reviews.sql"),
            include_str!("../migrations/004_example_contexts.sql"),
            include_str!("../migrations/005_sync.sql"),
            include_str!("../migrations/006_explanations.sql"),
            include_str!("../migrations/007_query_results.sql"),
            include_str!("../migrations/008_sentence_translations.sql"),
            include_str!("../migrations/009_assessments.sql"),
        ] {
            connection.execute_batch(migration).unwrap();
        }
        connection.pragma_update(None, "user_version", 9).unwrap();
        let legacy = Store {
            assessment_packet: Mutex::new(None),
            subtitle_guard: Mutex::new(()),
            root: directory.path().into(),
            connection: Mutex::new(connection),
            dictionary: RwLock::new(crate::base_dictionary::BaseDictionary::load(directory.path())),
        };
        let entry = legacy
            .collect(&crate::vocabulary::CollectionInput {
                operation_id: uuid::Uuid::new_v4().to_string(),
                kind: "word".into(),
                text: "growth".into(),
                meaning: "增长".into(),
                examples: vec![crate::vocabulary::ExampleInput {
                    text: "Steady growth.".into(),
                    ..Default::default()
                }],
                target_entry_id: None,
                expected_revision: None,
            })
            .unwrap();
        let unit = legacy
            .review_units("all", "meaning", 0, 1)
            .unwrap()
            .remove(0);
        let question = legacy.review_question(&unit.id, unit.revision).unwrap();
        legacy
            .review_submit(&crate::reviews::AnswerInput {
                operation_id: uuid::Uuid::new_v4().to_string(),
                question_id: question.id,
                answer: "增长".into(),
                unable: false,
            })
            .unwrap();
        drop(legacy);
        let upgraded = Store::open(directory.path()).unwrap();
        assert_eq!(upgraded.schema_version().unwrap(), 10);
        assert_eq!(
            upgraded.get_entry(&entry.entry_id).unwrap().examples[0].text,
            "Steady growth."
        );
        assert_eq!(
            upgraded.sync_export(&entry.entry_id).unwrap().tables["review_attempts"].len(),
            1
        );
        assert_eq!(
            upgraded
                .get_entry(&entry.entry_id)
                .unwrap()
                .collection_count,
            1
        );
    }

    #[test]
    fn database_migrates_once_and_enforces_relationships() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), 10);
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
            10
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
        assert_eq!(store.schema_version().unwrap(), 10);
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
        assert_eq!(store.schema_version().unwrap(), 10);
        let entry = store.get_entry("e").unwrap();
        assert_eq!(entry.collection_count, 1);
        assert_eq!(entry.examples[0].text, "She was reluctant.");
        let units = store.review_units("all", "meaning", 0, 50).unwrap();
        assert_eq!(units[0].id, "u");
        assert_eq!(units[0].state.due_at, 100);
        assert!(store.review_question("u", units[0].revision).is_ok());
    }

    #[test]
    fn version_five_upgrade_preserves_media_history_meanings_and_sync_state() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("vocabulary.sqlite3")).unwrap();
        for migration in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_imports.sql"),
            include_str!("../migrations/003_reviews.sql"),
            include_str!("../migrations/004_example_contexts.sql"),
            include_str!("../migrations/005_sync.sql"),
        ] {
            connection.execute_batch(migration).unwrap();
        }
        connection.execute_batch("INSERT INTO entries(id,kind,text,match_key,created_at,updated_at) VALUES ('e','word','breakfast','breakfast',100,100);
            INSERT INTO meanings(id,entry_id,text,origin,created_at) VALUES ('m','e','早餐','user',100);
            INSERT INTO examples(id,location_key,identity_key,text,created_at) VALUES ('x','manual','x','Kids, breakfast!',100);
            INSERT INTO entry_examples(entry_id,example_id,scope_key,meaning_id,context_meaning,created_at) VALUES ('e','x','m','m','早餐',100);
            INSERT INTO media_assets(id,recipe_key,digest,relative_path,format,duration_ms,state,created_at) VALUES ('a','recipe','digest','media/fixture.wav','wav',1000,'ready',100);
            INSERT INTO example_media(example_id,asset_id) VALUES ('x','a');
            INSERT INTO collection_actions(operation_id,request_hash,entry_id,result_json,created_at) VALUES ('collect','hash','e','{}',100);
            INSERT INTO learning_units(id,entry_id,scope_key,dimension,created_at) VALUES ('u','e','m','meaning',100);
            INSERT INTO review_states(unit_id,state_json,due_at,relearn_at) VALUES ('u','{}',100,100);
            INSERT INTO review_attempts(id,operation_id,request_hash,unit_id,question_json,answer,hinted,outcome,grader,policy_version,state_before_json,created_at) VALUES ('answer','submit','hash','u','{}','早餐',0,'correct','exact_or_confirm','svl-review-1','{}',101);
            INSERT INTO review_corrections(id,operation_id,attempt_id,expected_revision,outcome,reason,created_at,request_hash) VALUES ('correction','correct','answer',1,'incorrect','合成迁移检查',102,'hash');").unwrap();
        connection.pragma_update(None, "user_version", 5).unwrap();
        connection
            .execute(
                "INSERT INTO sync_dirty_entries(entry_id,epoch,updated_at) VALUES ('e',1,100)",
                [],
            )
            .unwrap();
        drop(connection);
        std::fs::create_dir_all(directory.path().join("media")).unwrap();
        std::fs::write(
            directory.path().join("media/fixture.wav"),
            b"private fixture audio",
        )
        .unwrap();
        let store = Store::open(directory.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), 10);
        let entry = store.get_entry("e").unwrap();
        assert_eq!(entry.collection_count, 1);
        assert_eq!(entry.examples[0].contexts[0].context_meaning, "早餐");
        assert_eq!(entry.meanings[0].text, "早餐");
        assert_eq!(entry.examples[0].audio[0].id, "a");
        let c = store.connection().unwrap();
        let answer: String = c
            .query_row(
                "SELECT answer FROM review_attempts WHERE id='answer'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let correction: String = c
            .query_row(
                "SELECT reason FROM review_corrections WHERE id='correction'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let epoch: i64 = c
            .query_row(
                "SELECT epoch FROM sync_dirty_entries WHERE entry_id='e'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            (answer.as_str(), correction.as_str(), epoch),
            ("早餐", "合成迁移检查", 1)
        );
        assert_eq!(
            std::fs::read(directory.path().join("media/fixture.wav")).unwrap(),
            b"private fixture audio"
        );
    }
}
