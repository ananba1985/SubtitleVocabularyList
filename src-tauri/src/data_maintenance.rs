//! Cleanup of application-owned migration artifacts after a successful startup.
use crate::{error::AppError, store::Store};
use rusqlite::{Connection, MAIN_DB, OpenFlags};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

fn error(message: impl Into<String>) -> AppError {
    AppError::new("data_maintenance", message)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

// Every deletion is confined to the selected Data directory, including on Windows
// where junctions are reparse points rather than ordinary symlinks.
fn owned_path(root: &Path, path: &Path) -> Result<(), AppError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| error("清理路径超出资料目录。"))?;
    let mut current = root.to_path_buf();
    for component in std::iter::once(Component::CurDir).chain(relative.components()) {
        match component {
            Component::CurDir => (),
            Component::Normal(name) => current.push(name),
            _ => return Err(error("清理路径含非预期的上级目录。")),
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

fn plain_tree(root: &Path, path: &Path) -> Result<(), AppError> {
    owned_path(root, path)?;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|e| error(e.to_string()))?;
        if is_link(&fs::symlink_metadata(entry.path())?) {
            return Err(error(format!("目录中含链接，已保留：{}", path.display())));
        }
    }
    Ok(())
}

fn readonly(path: &Path) -> Result<Connection, AppError> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

fn verify(connection: &Connection) -> Result<(), AppError> {
    let check: String = connection.pragma_query_value(None, "quick_check", |row| row.get(0))?;
    if check != "ok" || connection.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err(error("数据库完整性检查未通过，已保留全部备份及旧目录。"));
    }
    Ok(())
}

fn automatic_directory(root: &Path) -> Result<PathBuf, AppError> {
    let path = root.join("backups/automatic");
    owned_path(root, &path)?;
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn snapshot(
    root: &Path,
    connection: &Connection,
    timestamp: u128,
    id: uuid::Uuid,
) -> Result<PathBuf, AppError> {
    let directory = automatic_directory(root)?;
    let path = directory.join(format!("snapshot-{timestamp}-{id}.sqlite3"));
    owned_path(root, &path)?;
    if path.exists() {
        verify(&readonly(&path)?)?;
        return Ok(path);
    }
    let temporary = path.with_extension("sqlite3.partial");
    owned_path(root, &temporary)?;
    // A failed copy is never published or counted as a recoverable backup.
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    connection.backup(MAIN_DB, &temporary, None)?;
    verify(&readonly(&temporary)?)?;
    fs::OpenOptions::new()
        .write(true)
        .open(&temporary)?
        .sync_all()?;
    fs::rename(&temporary, &path)?;
    Ok(path)
}

pub(crate) fn before_schema_upgrade(root: &Path, connection: &Connection) -> Result<(), AppError> {
    let root = dunce::canonicalize(root)?;
    snapshot(
        &root,
        connection,
        chrono::Utc::now().timestamp_millis() as u128,
        uuid::Uuid::new_v4(),
    )?;
    Ok(())
}

fn snapshot_time(name: &str) -> Option<u128> {
    let (time, id) = name
        .strip_prefix("snapshot-")?
        .strip_suffix(".sqlite3")?
        .split_once('-')?;
    uuid::Uuid::parse_str(id).ok()?;
    time.parse().ok()
}

fn migration_directory(name: &str) -> Option<uuid::Uuid> {
    uuid::Uuid::parse_str(name.strip_prefix("migration-")?).ok()
}

fn only_database_files(path: &Path, database: &str, receipt: bool) -> Result<bool, AppError> {
    let allowed = [
        database.to_owned(),
        format!("{database}-shm"),
        format!("{database}-wal"),
    ];
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type()?.is_file()
            || (!allowed.contains(&name) && !(receipt && name == "migration-verified.json"))
        {
            return Ok(false);
        }
    }
    Ok(path.join(database).is_file())
}

fn import_migration(root: &Path, path: &Path, id: uuid::Uuid) -> Result<bool, AppError> {
    plain_tree(root, path)?;
    if !only_database_files(path, "before.sqlite3", true)? {
        return Ok(false);
    }
    let receipt = path.join("migration-verified.json");
    if receipt.exists() {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(receipt)?)?;
        if value["verified"] != true
            || value["targetRoot"]
                .as_str()
                .and_then(|p| dunce::canonicalize(p).ok())
                .as_deref()
                != Some(root)
        {
            return Ok(false);
        }
    }
    let before = path.join("before.sqlite3");
    let time = fs::metadata(&before)?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| error(e.to_string()))?
        .as_millis();
    let connection = readonly(&before)?;
    verify(&connection)?;
    snapshot(root, &connection, time, id)?;
    drop(connection);
    // The database was copied consistently (including WAL) before its old wrapper
    // and SQLite sidecars are removed. Unknown files keep the entire directory.
    plain_tree(root, path)?;
    fs::remove_dir_all(path)?;
    Ok(true)
}

fn remove_empty_legacy_data(root: &Path, path: &Path) -> Result<bool, AppError> {
    plain_tree(root, path)?;
    if !only_database_files(path, "vocabulary.sqlite3", false)? {
        return Ok(false);
    }
    let connection = readonly(&path.join("vocabulary.sqlite3"))?;
    verify(&connection)?;
    let names = connection
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for table in names {
        if connection
            .prepare(&format!(
                "SELECT 1 FROM \"{}\" LIMIT 1",
                table.replace('"', "\"\"")
            ))?
            .exists([])?
        {
            return Ok(false);
        }
    }
    drop(connection);
    plain_tree(root, path)?;
    fs::remove_dir_all(path)?;
    Ok(true)
}

pub(crate) fn after_startup(store: &Store) -> Result<usize, AppError> {
    if is_link(&fs::symlink_metadata(store.root())?) {
        return Err(error("资料目录是链接，本次跳过自动清理。"));
    }
    let root = dunce::canonicalize(store.root())?;
    let backups = root.join("backups");
    owned_path(&root, &backups)?;
    if !backups.exists() {
        return Ok(0);
    }
    // Store::open has committed all schema migrations. This check runs only after
    // the main UI loaded its initial data, and holds the DB lock only to validate.
    {
        let connection = store.connection()?;
        verify(&connection)?;
    }
    let mut removed = 0;
    for entry in fs::read_dir(&backups)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name == "legacy-installed-cache" {
            plain_tree(&root, &path)?;
            fs::remove_dir_all(&path)?;
            removed += 1;
        } else if name == "legacy-installed-data" {
            removed += usize::from(remove_empty_legacy_data(&root, &path)?);
        } else if let Some(id) = migration_directory(&name) {
            removed += usize::from(import_migration(&root, &path, id)?);
        }
    }
    let automatic = backups.join("automatic");
    owned_path(&root, &automatic)?;
    if automatic.exists() {
        let mut snapshots = Vec::new();
        for entry in fs::read_dir(&automatic)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(time) = snapshot_time(&name) {
                owned_path(&root, &entry.path())?;
                if entry.file_type()?.is_file() {
                    snapshots.push((time, entry.path()));
                }
            } else {
                let partial = name
                    .strip_suffix(".partial")
                    .or_else(|| name.strip_suffix(".partial-wal"))
                    .or_else(|| name.strip_suffix(".partial-shm"));
                let orphan = name
                    .strip_suffix("-wal")
                    .or_else(|| name.strip_suffix("-shm"))
                    .filter(|main| !automatic.join(main).exists());
                if partial
                    .or(orphan)
                    .is_some_and(|main| snapshot_time(main).is_some())
                {
                    owned_path(&root, &entry.path())?;
                    if entry.file_type()?.is_file() {
                        fs::remove_file(entry.path())?;
                        removed += 1;
                    }
                }
            }
        }
        snapshots.sort_by(|a, b| b.cmp(a));
        for (_, path) in snapshots.into_iter().skip(2) {
            for file in [
                path.clone(),
                PathBuf::from(format!("{}-wal", path.display())),
                PathBuf::from(format!("{}-shm", path.display())),
            ] {
                owned_path(&root, &file)?;
                if file.exists() {
                    fs::remove_file(file)?;
                }
            }
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_startup_retires_owned_artifacts_and_keeps_two_backups_and_learning_data() {
        let directory = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        let old = Connection::open(root.join("vocabulary.sqlite3")).unwrap();
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
            old.execute_batch(migration).unwrap();
        }
        old.pragma_update(None, "user_version", 9).unwrap();
        old.execute("INSERT INTO entries(id,kind,text,match_key,created_at,updated_at) VALUES ('legacy','word','legacy','legacy',1,1)", []).unwrap();
        drop(old);

        let store = Store::open(&root).unwrap();
        assert_eq!(store.schema_version().unwrap(), 10);
        assert_eq!(store.get_entry("legacy").unwrap().text, "legacy");
        let saved = store
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
        let unit = store
            .review_units("all", "meaning", 0, 100)
            .unwrap()
            .into_iter()
            .find(|unit| unit.entry_id == saved.entry_id)
            .unwrap();
        let question = store.review_question(&unit.id, unit.revision).unwrap();
        store
            .review_submit(&crate::reviews::AnswerInput {
                operation_id: uuid::Uuid::new_v4().to_string(),
                question_id: question.id,
                answer: "增长".into(),
                unable: false,
            })
            .unwrap();
        let before = serde_json::to_value(store.sync_export(&saved.entry_id).unwrap()).unwrap();
        let audio = root.join("media/fixture.wav");
        fs::write(&audio, b"synthetic original audio").unwrap();
        fs::create_dir_all(root.join("cache/webview")).unwrap();
        fs::write(root.join("cache/webview/current-cache"), b"active cache").unwrap();

        let backups = root.join("backups");
        let legacy_cache = backups.join("legacy-installed-cache");
        fs::create_dir_all(&legacy_cache).unwrap();
        fs::write(legacy_cache.join("old-cache"), b"obsolete").unwrap();
        let legacy_data = backups.join("legacy-installed-data");
        fs::create_dir_all(&legacy_data).unwrap();
        Connection::open(legacy_data.join("vocabulary.sqlite3"))
            .unwrap()
            .execute_batch("CREATE TABLE entries(id TEXT)")
            .unwrap();
        let migration = backups.join(format!("migration-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&migration).unwrap();
        let time = chrono::Utc::now().timestamp_millis() as u128;
        let (newest, previous);
        {
            let connection = store.connection().unwrap();
            connection
                .backup(MAIN_DB, migration.join("before.sqlite3"), None)
                .unwrap();
            fs::write(
                migration.join("migration-verified.json"),
                serde_json::to_vec(&serde_json::json!({
                    "verified": true, "targetRoot": root, "formatVersion": 1,
                }))
                .unwrap(),
            )
            .unwrap();
            previous = snapshot(&root, &connection, time + 60_000, uuid::Uuid::new_v4()).unwrap();
            newest = snapshot(&root, &connection, time + 120_000, uuid::Uuid::new_v4()).unwrap();
        }
        let manual = backups.join("manual-backup.sqlite3");
        fs::write(&manual, b"manual backup preserved byte for byte").unwrap();
        fs::create_dir_all(backups.join("manual-folder")).unwrap();
        fs::write(backups.join("manual-folder/notes.txt"), b"manual notes").unwrap();

        store.maintain_after_startup().unwrap();

        assert!(!legacy_cache.exists() && !legacy_data.exists() && !migration.exists());
        let retained = fs::read_dir(backups.join("automatic"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .and_then(snapshot_time)
                    .is_some()
            })
            .collect::<Vec<_>>();
        assert_eq!(retained.len(), 2);
        assert!(retained.contains(&newest) && retained.contains(&previous));
        assert_eq!(
            fs::read(manual).unwrap(),
            b"manual backup preserved byte for byte"
        );
        assert_eq!(
            fs::read(backups.join("manual-folder/notes.txt")).unwrap(),
            b"manual notes"
        );
        assert_eq!(fs::read(audio).unwrap(), b"synthetic original audio");
        assert_eq!(
            fs::read(root.join("cache/webview/current-cache")).unwrap(),
            b"active cache"
        );
        assert_eq!(
            serde_json::to_value(store.sync_export(&saved.entry_id).unwrap()).unwrap(),
            before
        );
        assert_eq!(store.get_entry("legacy").unwrap().text, "legacy");
        assert_eq!(
            store.sync_export(&saved.entry_id).unwrap().tables["review_attempts"].len(),
            1
        );
    }
}
