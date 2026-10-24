//! Durable corpora and reference-based reclamation of app-owned task workspaces.
use crate::{
    error::AppError,
    managed_files::{is_link, owned_path, plain_tree},
    media::ImportedMedia,
    store::Store,
    tasks::TaskSnapshot,
    vocabulary,
};
use rusqlite::params;
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

fn error(message: impl Into<String>) -> AppError {
    AppError::new("storage_unavailable", message)
}
fn hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn root(store: &Store) -> Result<PathBuf, AppError> {
    if is_link(&fs::symlink_metadata(store.root())?) {
        return Err(error("资料目录包含链接，已延后文件维护。"));
    }
    Ok(dunce::canonicalize(store.root())?)
}

fn directory(root: &Path, path: PathBuf) -> Result<PathBuf, AppError> {
    owned_path(root, &path)?;
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub(crate) fn workspace(store: &Store, id: &str) -> Result<PathBuf, AppError> {
    uuid::Uuid::parse_str(id).map_err(|_| error("任务工作目录标识无效。"))?;
    let root = root(store)?;
    directory(&root, root.join("jobs").join(format!("task-{id}")))
}

pub(crate) fn speech_directory(store: &Store) -> Result<PathBuf, AppError> {
    let root = root(store)?;
    directory(&root, root.join("cache/speech"))
}

fn path_key(path: &Path) -> String {
    let value = dunce::simplified(path).to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    {
        value.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        value
    }
}

fn within(path: &Path, parent: &Path) -> bool {
    let path = path_key(path);
    let parent = path_key(parent);
    path == parent || path.starts_with(&(parent + "/"))
}

fn in_jobs(root: &Path, path: &Path) -> bool {
    path.is_absolute() && within(path, &root.join("jobs"))
}

fn publish_file(
    root: &Path,
    source: &Path,
    destination: &Path,
    digest: &str,
) -> Result<(), AppError> {
    owned_path(root, source)?;
    owned_path(root, destination)?;
    if destination.exists() {
        if crate::corpus::file_hash(destination)? == digest {
            return Ok(());
        }
        return Err(error("长期资料目标与内容摘要不一致，原文件已保留。"));
    }
    let temporary = destination.with_extension(format!("pending-{}", uuid::Uuid::new_v4()));
    owned_path(root, &temporary)?;
    let result = (|| {
        fs::copy(source, &temporary)?;
        if crate::corpus::file_hash(&temporary)? != digest {
            return Err(error("资料复制摘要不一致，原文件已保留。"));
        }
        fs::OpenOptions::new()
            .write(true)
            .open(&temporary)?
            .sync_all()?;
        if let Err(e) = fs::rename(&temporary, destination) {
            if !destination.is_file() || crate::corpus::file_hash(destination)? != digest {
                return Err(e.into());
            }
        }
        Ok(())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn publish_document(
    store: &Store,
    id: &str,
    mut value: Value,
    audio: &Path,
) -> Result<(Value, PathBuf, PathBuf), AppError> {
    let root = root(store)?;
    let directory = directory(
        &root,
        root.join("corpora")
            .join(format!("source-{}", vocabulary::digest(id.as_bytes()))),
    )?;
    let mut saved_audio = audio.to_path_buf();
    if in_jobs(&root, audio) {
        owned_path(&root, audio)?;
        let digest = crate::corpus::file_hash(audio)?;
        let extension = audio
            .extension()
            .and_then(|s| s.to_str())
            .filter(|s| s.bytes().all(|b| b.is_ascii_alphanumeric()))
            .unwrap_or("bin");
        saved_audio = directory.join(format!("audio-{digest}.{extension}"));
        publish_file(&root, audio, &saved_audio, &digest)?;
    }
    value["audioPath"] = serde_json::to_value(&saved_audio)?;
    let bytes = serde_json::to_vec_pretty(&value)?;
    let path = directory.join(format!("corpus-{}.json", vocabulary::digest(&bytes)));
    owned_path(&root, &path)?;
    if path.exists() {
        if fs::read(&path)? != bytes {
            return Err(error("语料目标与内容摘要不一致，原文件已保留。"));
        }
    } else {
        let temporary = path.with_extension(format!("pending-{}", uuid::Uuid::new_v4()));
        owned_path(&root, &temporary)?;
        fs::write(&temporary, &bytes)?;
        fs::OpenOptions::new()
            .write(true)
            .open(&temporary)?
            .sync_all()?;
        if let Err(e) = fs::rename(&temporary, &path) {
            if !path.is_file() || fs::read(&path)? != bytes {
                return Err(e.into());
            }
            fs::remove_file(&temporary)?;
        }
    }
    Ok((value, path, saved_audio))
}

pub(crate) fn publish_import(
    store: &Store,
    id: &str,
    imported: &ImportedMedia,
    corpus: &Path,
) -> Result<(ImportedMedia, PathBuf), AppError> {
    let root = root(store)?;
    if !in_jobs(&root, corpus) || !corpus.is_file() {
        return Ok((imported.clone(), corpus.into()));
    }
    owned_path(&root, corpus)?;
    let (value, path, _) = publish_document(
        store,
        id,
        serde_json::to_value(imported)?,
        &imported.audio_path,
    )?;
    Ok((serde_json::from_value(value)?, path))
}

#[derive(Default)]
struct References {
    paths: HashSet<String>,
}
impl References {
    fn add(&mut self, root: &Path, path: &Path) {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        if in_jobs(root, &path) {
            self.paths.insert(path_key(&path));
        }
    }
    fn json(&mut self, root: &Path, value: &Value) {
        match value {
            Value::String(value) => self.add(root, Path::new(value)),
            Value::Array(values) => {
                for value in values {
                    self.json(root, value);
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    self.json(root, value);
                }
            }
            _ => (),
        }
    }
    fn task(&mut self, root: &Path, id: &str, state: &str, json: &str) -> Result<(), AppError> {
        if !matches!(state, "succeeded" | "failed" | "cancelled") {
            self.add(root, &root.join("jobs").join(format!("task-{id}")));
        }
        self.json(root, &serde_json::from_str(json)?);
        Ok(())
    }
    fn protects(&self, path: &Path) -> bool {
        let path = path_key(path);
        self.paths.iter().any(|reference| {
            reference == &path
                || reference.starts_with(&(path.clone() + "/"))
                || path.starts_with(&(reference.clone() + "/"))
        })
    }
}

fn references(
    store: &Store,
    root: &Path,
    foreground: &[TaskSnapshot],
) -> Result<References, AppError> {
    let (paths, tasks, settings) = {
        let c = store.connection()?;
        let mut paths = Vec::new();
        for sql in [
            "SELECT corpus_path FROM sources WHERE corpus_path IS NOT NULL",
            "SELECT audio_path FROM sources WHERE audio_path IS NOT NULL",
            "SELECT path_hint FROM sources WHERE path_hint IS NOT NULL",
            "SELECT relative_path FROM media_assets",
        ] {
            paths.extend(
                c.prepare(sql)?
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        let tasks = c
            .prepare("SELECT id,state,snapshot_json FROM tasks")?
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let settings = c
            .prepare("SELECT value_json FROM settings WHERE value_json LIKE '%jobs%'")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        (paths, tasks, settings)
    };
    let mut result = References::default();
    for path in paths {
        result.add(root, Path::new(&path));
    }
    for (id, state, json) in tasks {
        result.task(root, &id, &state, &json)?;
    }
    for task in foreground {
        result.task(root, &task.id, &task.state, &serde_json::to_string(task)?)?;
    }
    for json in settings {
        result.json(root, &serde_json::from_str(&json)?);
    }
    Ok(result)
}

fn legacy_files(path: &Path) -> Result<bool, AppError> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let pgs = name
            .strip_prefix("pgs-")
            .and_then(|s| s.strip_suffix(".png"))
            .is_some_and(|s| s.parse::<usize>().is_ok());
        if !entry.file_type()?.is_file()
            || !(pgs
                || matches!(
                    name.as_str(),
                    "corpus.json"
                        | "audio.m4a"
                        | "original-subtitles.sup"
                        | "original-subtitles.srt"
                        | "speech-16k.wav"
                        | "transcribed.json"
                )
                || name.starts_with("external-input."))
        {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Default, Debug)]
pub(crate) struct Report {
    pub migrated_sources: usize,
    pub removed: usize,
}

/// Called with the TaskManager's exclusive file lease; no worker can use a stale
/// source path between reading the DB and opening the corresponding file.
pub(crate) fn maintain(store: &Store, foreground: &[TaskSnapshot]) -> Result<Report, AppError> {
    let root = root(store)?;
    let jobs = root.join("jobs");
    owned_path(&root, &jobs)?;
    if !jobs.exists() {
        return Ok(Report::default());
    }
    let candidates = fs::read_dir(&jobs)?.collect::<Result<Vec<_>, _>>()?;
    let sources = store.connection()?.prepare("SELECT id,corpus_path,audio_path FROM sources WHERE corpus_path IS NOT NULL AND audio_path IS NOT NULL")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?,r.get::<_, String>(1)?,r.get::<_, String>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
    let mut promoted = Vec::new();
    for (id, corpus, audio) in sources {
        let path = Path::new(&corpus);
        if !in_jobs(&root, path) || !path.is_file() {
            continue;
        }
        owned_path(&root, path)?;
        let value = serde_json::from_slice(&fs::read(path)?)?;
        let (_, saved_corpus, saved_audio) =
            publish_document(store, &id, value, Path::new(&audio))?;
        promoted.push((id, corpus, audio, saved_corpus, saved_audio));
    }
    let mut report = Report::default();
    if !promoted.is_empty() {
        let mut c = store.connection()?;
        let tx = c.transaction()?;
        for (id, old_corpus, old_audio, corpus, audio) in promoted {
            report.migrated_sources += tx.execute("UPDATE sources SET corpus_path=?,audio_path=? WHERE id=? AND corpus_path=? AND audio_path=?", params![corpus.to_string_lossy(),audio.to_string_lossy(),id,old_corpus,old_audio])?;
        }
        tx.commit()?;
    }
    let refs = references(store, &root, foreground)?;
    for entry in candidates {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if refs.protects(&path) {
            continue;
        }
        let task = name
            .strip_prefix("task-")
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok());
        let legacy = name.strip_prefix("video-").is_some_and(hex);
        if entry.file_type()?.is_dir() && (task || legacy) {
            plain_tree(&root, &path)?;
            if legacy && !legacy_files(&path)? {
                continue;
            }
            fs::remove_dir_all(&path)?;
            report.removed += 1;
        } else if entry.file_type()?.is_file()
            && (name
                .strip_prefix("system-")
                .and_then(|s| s.strip_suffix(".wav"))
                .is_some_and(|s| hex(s) || uuid::Uuid::parse_str(s).is_ok())
                || name
                    .strip_prefix("android-corpus-")
                    .and_then(|s| s.strip_suffix(".json"))
                    .is_some_and(hex))
        {
            owned_path(&root, &path)?;
            fs::remove_file(&path)?;
            report.removed += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        media::Segment,
        vocabulary::{CollectionInput, ExampleInput, SourceInput},
    };

    fn task(id: &str, state: &str, result: Option<Value>) -> TaskSnapshot {
        TaskSnapshot {
            id: id.into(),
            operation_id: uuid::Uuid::new_v4().to_string(),
            request_hash: "fixture".into(),
            kind: "import".into(),
            subject: None,
            stage: "fixture".into(),
            state: state.into(),
            current: 0,
            total: 1,
            message: "合成任务".into(),
            result,
            error: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn normal_publication_migration_and_reference_cleanup_keep_complete_learning_data() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open(temporary.path()).unwrap();
        let root = dunce::canonicalize(store.root()).unwrap();
        let fingerprint = vocabulary::digest(b"legacy episode");
        let legacy = root.join("jobs").join(format!("video-{fingerprint}"));
        fs::create_dir_all(&legacy).unwrap();
        let audio = legacy.join("audio.m4a");
        fs::write(&audio, b"complete episode audio bytes").unwrap();
        let corpus = legacy.join("corpus.json");
        let document = serde_json::json!({
            "title":"Legacy episode", "durationMs":2000, "audioPath":audio, "audioStream":0,
            "subtitleStream":1,"textSource":"embedded_text","warnings":[],
            "segments":[{"locationKey":"cue:0","text":"Steady growth.","startMs":0,"endMs":1000},
                        {"locationKey":"cue:1","text":"Keep the complete second sentence.","startMs":1000,"endMs":2000}],
            "extraProvenance":{"preserve":"original metadata"}
        });
        fs::write(&corpus, serde_json::to_vec(&document).unwrap()).unwrap();
        fs::write(legacy.join("original-subtitles.sup"), b"OCR input").unwrap();
        fs::write(legacy.join("pgs-0.png"), b"OCR intermediate").unwrap();
        let source = store
            .add_source(&SourceInput {
                kind: "video".into(),
                title: "Legacy episode".into(),
                fingerprint: fingerprint.clone(),
                path_hint: None,
                duration_ms: Some(2000),
            })
            .unwrap();
        store.connection().unwrap().execute("UPDATE sources SET corpus_path=?,audio_path=?,text_source='embedded_text',imported_at=1 WHERE id=?",params![corpus.to_string_lossy(),audio.to_string_lossy(),source]).unwrap();
        let asset = uuid::Uuid::new_v4().to_string();
        let saved_audio = root.join("media/original.m4a");
        fs::write(&saved_audio, b"saved original clip").unwrap();
        store.connection().unwrap().execute("INSERT INTO media_assets(id,recipe_key,digest,relative_path,kind,format,duration_ms,state,created_at) VALUES (?,?,?,'media/original.m4a','original','m4a',1000,'ready',1)",params![asset,asset,vocabulary::digest(b"saved original clip")]).unwrap();
        let entry = store
            .collect(&CollectionInput {
                operation_id: uuid::Uuid::new_v4().to_string(),
                kind: "word".into(),
                text: "growth".into(),
                meaning: "增长".into(),
                examples: vec![ExampleInput {
                    source_id: Some(source.clone()),
                    text: "Steady growth.".into(),
                    location_key: "cue:0".into(),
                    start_ms: Some(0),
                    end_ms: Some(1000),
                    media_asset_ids: vec![asset],
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
            .find(|u| u.entry_id == entry.entry_id)
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
        let before = serde_json::to_value(store.sync_export(&entry.entry_id).unwrap()).unwrap();

        let fresh_task = uuid::Uuid::new_v4().to_string();
        let fresh = workspace(&store, &fresh_task).unwrap();
        let fresh_audio = fresh.join("audio.m4a");
        fs::write(&fresh_audio, b"new complete episode").unwrap();
        let imported = ImportedMedia {
            title: "New episode".into(),
            duration_ms: 1000,
            audio_path: fresh_audio,
            audio_stream: 0,
            subtitle_stream: Some(1),
            text_source: "embedded_text".into(),
            warnings: vec![],
            segments: vec![Segment {
                location_key: "cue:0".into(),
                text: "A new complete sentence.".into(),
                start_ms: 0,
                end_ms: 1000,
            }],
        };
        let fresh_corpus = fresh.join("corpus.json");
        fs::write(&fresh_corpus, serde_json::to_vec(&imported).unwrap()).unwrap();
        let installed = store
            .install_corpus(
                Path::new("synthetic.mkv"),
                &vocabulary::digest(b"new episode"),
                &imported,
                &fresh_corpus,
            )
            .unwrap();
        assert!(
            store
                .cached_source(&vocabulary::digest(b"new episode"))
                .unwrap()
                .is_some()
        );

        let orphan = root
            .join("jobs")
            .join(format!("video-{}", vocabulary::digest(b"unused import")));
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("pgs-0.png"), b"unused OCR").unwrap();
        let active_id = uuid::Uuid::new_v4().to_string();
        let active = workspace(&store, &active_id).unwrap();
        fs::write(active.join("working.png"), b"active task").unwrap();
        let active_task = task(&active_id, "running", None);
        store.connection().unwrap().execute("INSERT INTO tasks(id,operation_id,kind,stage,state,snapshot_json,created_at,updated_at) VALUES (?,?,?,'fixture','running',?,1,1)",params![active_id,active_task.operation_id,active_task.kind,serde_json::to_string(&active_task).unwrap()]).unwrap();
        let receipt_id = uuid::Uuid::new_v4().to_string();
        let receipt = workspace(&store, &receipt_id).unwrap();
        let receipt_file = receipt.join("result.png");
        fs::write(&receipt_file, b"receipt still references result").unwrap();
        let foreground = task(
            &receipt_id,
            "succeeded",
            Some(serde_json::json!({"path":receipt_file})),
        );
        let manual = root.join("jobs/manual-notes");
        fs::create_dir_all(&manual).unwrap();
        fs::write(manual.join("notes.txt"), b"keep manual file").unwrap();
        let speech = speech_directory(&store).unwrap().join("system-fixture.wav");
        fs::write(&speech, b"cached speech").unwrap();

        let report = maintain(&store, &[foreground]).unwrap();
        assert_eq!(report.migrated_sources, 1);
        assert!(!legacy.exists() && !orphan.exists() && !fresh.exists());
        assert_eq!(
            fs::read(active.join("working.png")).unwrap(),
            b"active task"
        );
        assert_eq!(
            fs::read(receipt_file).unwrap(),
            b"receipt still references result"
        );
        assert_eq!(
            fs::read(manual.join("notes.txt")).unwrap(),
            b"keep manual file"
        );
        assert_eq!(fs::read(speech).unwrap(), b"cached speech");
        for id in [&source, &installed.id] {
            let (corpus, audio): (String, String) = store
                .connection()
                .unwrap()
                .query_row(
                    "SELECT corpus_path,audio_path FROM sources WHERE id=?",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert!(Path::new(&corpus).starts_with(root.join("corpora")));
            assert!(Path::new(&audio).starts_with(root.join("corpora")));
            let value: Value = serde_json::from_slice(&fs::read(&corpus).unwrap()).unwrap();
            assert_eq!(value["audioPath"], serde_json::to_value(&audio).unwrap());
            if id == &source {
                assert_eq!(value["segments"], document["segments"]);
                assert_eq!(value["extraProvenance"], document["extraProvenance"]);
                assert_eq!(fs::read(audio).unwrap(), b"complete episode audio bytes");
            }
        }
        assert_eq!(fs::read(saved_audio).unwrap(), b"saved original clip");
        assert_eq!(
            serde_json::to_value(store.sync_export(&entry.entry_id).unwrap()).unwrap(),
            before
        );
        assert_eq!(
            store.sync_export(&entry.entry_id).unwrap().tables["review_attempts"].len(),
            1
        );
    }
}
