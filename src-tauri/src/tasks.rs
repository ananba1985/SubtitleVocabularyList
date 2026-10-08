use crate::{error::AppError, store::Store};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub id: String,
    pub operation_id: String,
    pub request_hash: String,
    pub kind: String,
    #[serde(default)]
    pub subject: Option<String>,
    pub stage: String,
    pub state: String,
    pub current: usize,
    pub total: usize,
    pub message: String,
    pub result: Option<Value>,
    pub error: Option<AppError>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl TaskSnapshot {
    pub fn terminal(&self) -> bool {
        matches!(self.state.as_str(), "succeeded" | "failed" | "cancelled")
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskHistoryPage {
    pub items: Vec<TaskSnapshot>,
    pub total: usize,
}

type Observer = Arc<dyn Fn(&TaskSnapshot) + Send + Sync>;
const FOREGROUND_KINDS: &str = "'preview','speech','review_audio','explanation'";
const FOREGROUND_RECEIPTS: usize = 64;

fn foreground(kind: &str) -> bool {
    matches!(kind, "preview" | "speech" | "review_audio" | "explanation")
}

fn trim_receipts(receipts: &mut HashMap<String, TaskSnapshot>, current: &str) {
    let mut ended = receipts
        .values()
        .filter(|task| task.terminal() && task.id != current)
        .map(|task| (task.updated_at, task.id.clone()))
        .collect::<Vec<_>>();
    ended.sort();
    let protected = usize::from(receipts.get(current).is_some_and(TaskSnapshot::terminal));
    let excess = (ended.len() + protected).saturating_sub(FOREGROUND_RECEIPTS);
    for (_, id) in ended.into_iter().take(excess) {
        receipts.remove(&id);
    }
}

pub struct TaskManager {
    store: Arc<Store>,
    controls: Mutex<HashMap<String, Arc<AtomicBool>>>,
    start_guard: Mutex<()>,
    observer: Observer,
    foreground: Mutex<HashMap<String, TaskSnapshot>>,
}

#[derive(Clone)]
pub struct TaskContext {
    manager: Arc<TaskManager>,
    pub task_id: String,
    pub cancelled: Arc<AtomicBool>,
}

impl TaskContext {
    pub fn subject(&self, value: &str) {
        let _ = self.manager.update(&self.task_id, |snapshot| {
            snapshot.subject = Some(value.chars().take(240).collect());
        });
    }

    pub fn check_cancelled(&self) -> Result<(), AppError> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(AppError::new("cancelled", "任务已取消。"))
        } else {
            Ok(())
        }
    }

    pub fn progress(&self, stage: &str, current: usize, total: usize, message: &str) {
        let _ = self.manager.update(&self.task_id, |snapshot| {
            if snapshot.terminal() {
                return;
            }
            if snapshot.state != "cancel_requested" {
                snapshot.state = "running".into();
            }
            snapshot.stage = stage.into();
            snapshot.current = current;
            snapshot.total = total;
            snapshot.message = message.into();
        });
    }

    pub fn partial_result(&self, value: Value) {
        let _ = self
            .manager
            .update(&self.task_id, |snapshot| snapshot.result = Some(value));
    }
}

impl TaskManager {
    pub fn new(store: Arc<Store>, observer: Observer) -> Arc<Self> {
        Arc::new(Self {
            store,
            controls: Mutex::new(HashMap::new()),
            start_guard: Mutex::new(()),
            observer,
            foreground: Mutex::new(HashMap::new()),
        })
    }

    // Call only during startup after the desktop single-instance guard is active.
    pub fn recover_interrupted(&self) -> Result<(), AppError> {
        let ids = {
            let connection = self.store.connection()?;
            let mut query = connection.prepare(
                "SELECT id FROM tasks WHERE state IN ('queued','running','cancel_requested')",
            )?;
            query
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for id in ids {
            self.update(&id, |snapshot| {
                snapshot.state = "failed".into();
                snapshot.error = Some(AppError::new(
                    "interrupted",
                    "上次运行已中断。已保存内容保留，可以重新执行。",
                ));
            })?;
        }
        Ok(())
    }

    pub fn start<F>(
        self: &Arc<Self>,
        kind: &str,
        operation_id: &str,
        request_hash: &str,
        worker: F,
    ) -> Result<TaskSnapshot, AppError>
    where
        F: FnOnce(TaskContext) -> Result<Value, AppError> + Send + 'static,
    {
        if Uuid::parse_str(operation_id).is_err() {
            return Err(AppError::new("invalid_input", "任务操作标识无效。"));
        }
        let _guard = self
            .start_guard
            .lock()
            .map_err(|_| AppError::new("internal_error", "任务启动状态不可用。"))?;
        let memory = self
            .foreground
            .lock()
            .map_err(|_| AppError::new("internal_error", "临时操作状态不可用。"))?
            .values()
            .filter(|task| task.operation_id == operation_id && task.kind == kind)
            .max_by_key(|task| {
                (
                    !matches!(task.state.as_str(), "failed" | "cancelled"),
                    task.updated_at,
                )
            })
            .cloned();
        let existing = if let Some(snapshot) = memory {
            Some(snapshot)
        } else {
            let connection = self.store.connection()?;
            let json = connection.query_row("SELECT snapshot_json FROM tasks WHERE operation_id=? AND kind=? ORDER BY created_at DESC,id LIMIT 1", params![operation_id,kind], |row| row.get::<_, String>(0)).optional()?;
            json.map(|json| serde_json::from_str::<TaskSnapshot>(&json))
                .transpose()?
        };
        if let Some(snapshot) = existing {
            if snapshot.request_hash != request_hash {
                return Err(AppError::new(
                    "conflict",
                    "同一任务标识对应不同请求，请重新确认。",
                ));
            }
            if !matches!(snapshot.state.as_str(), "failed" | "cancelled") {
                return Ok(snapshot);
            }
        }
        let now = Utc::now().timestamp_millis();
        let snapshot = TaskSnapshot {
            id: Uuid::new_v4().to_string(),
            operation_id: operation_id.into(),
            request_hash: request_hash.into(),
            kind: kind.into(),
            subject: None,
            stage: "queued".into(),
            state: "queued".into(),
            current: 0,
            total: 0,
            message: "等待处理".into(),
            result: None,
            error: None,
            created_at: now,
            updated_at: now,
        };
        self.save(&snapshot)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.controls
            .lock()
            .map_err(|_| AppError::new("internal_error", "任务取消状态不可用。"))?
            .insert(snapshot.id.clone(), Arc::clone(&cancelled));
        let context = TaskContext {
            manager: Arc::clone(self),
            task_id: snapshot.id.clone(),
            cancelled,
        };
        let manager = Arc::clone(self);
        std::thread::spawn(move || {
            context.progress("starting", 0, 0, "正在处理");
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| worker(context.clone())))
                    .unwrap_or_else(|_| {
                        Err(AppError::new(
                            "internal_error",
                            "本次后台处理发生异常，已保存内容保留。",
                        ))
                    });
            let _ = manager.update(&context.task_id, |snapshot| match result {
                Ok(value) => {
                    snapshot.state = "succeeded".into();
                    snapshot.stage = "done".into();
                    snapshot.current = snapshot.total;
                    snapshot.result = Some(value);
                    snapshot.message = "处理完成".into();
                }
                Err(error) => {
                    snapshot.state = if error.code == "cancelled" {
                        "cancelled"
                    } else {
                        "failed"
                    }
                    .into();
                    snapshot.message = error.message.clone();
                    snapshot.error = Some(error);
                }
            });
            if let Ok(mut controls) = manager.controls.lock() {
                controls.remove(&context.task_id);
            }
        });
        Ok(snapshot)
    }

    fn save(&self, snapshot: &TaskSnapshot) -> Result<(), AppError> {
        if !foreground(&snapshot.kind) || snapshot.state == "failed" {
            self.persist(snapshot)?;
        }
        if foreground(&snapshot.kind) {
            let mut receipts = self
                .foreground
                .lock()
                .map_err(|_| AppError::new("internal_error", "临时操作状态不可用。"))?;
            receipts.insert(snapshot.id.clone(), snapshot.clone());
            trim_receipts(&mut receipts, &snapshot.id);
        }
        (self.observer)(snapshot);
        Ok(())
    }

    fn persist(&self, snapshot: &TaskSnapshot) -> Result<(), AppError> {
        self.store.connection()?.execute("INSERT INTO tasks(id,operation_id,kind,stage,state,snapshot_json,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET stage=excluded.stage,state=excluded.state,snapshot_json=excluded.snapshot_json,updated_at=excluded.updated_at", params![snapshot.id,snapshot.operation_id,snapshot.kind,snapshot.stage,snapshot.state,serde_json::to_string(snapshot)?,snapshot.created_at,snapshot.updated_at])?;
        Ok(())
    }

    fn update(
        &self,
        id: &str,
        change: impl FnOnce(&mut TaskSnapshot),
    ) -> Result<TaskSnapshot, AppError> {
        let mut receipts = self
            .foreground
            .lock()
            .map_err(|_| AppError::new("internal_error", "临时操作状态不可用。"))?;
        if let Some(current) = receipts.get(id) {
            let mut snapshot = current.clone();
            change(&mut snapshot);
            snapshot.updated_at = Utc::now().timestamp_millis();
            if snapshot.state == "failed" {
                self.persist(&snapshot)?;
            }
            receipts.insert(id.into(), snapshot.clone());
            trim_receipts(&mut receipts, &snapshot.id);
            drop(receipts);
            (self.observer)(&snapshot);
            return Ok(snapshot);
        }
        drop(receipts);
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        let json: String = transaction
            .query_row("SELECT snapshot_json FROM tasks WHERE id=?", [id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| AppError::new("not_found", "任务不存在。"))?;
        let mut snapshot: TaskSnapshot = serde_json::from_str(&json)?;
        change(&mut snapshot);
        snapshot.updated_at = Utc::now().timestamp_millis();
        transaction.execute(
            "UPDATE tasks SET stage=?,state=?,snapshot_json=?,updated_at=? WHERE id=?",
            params![
                snapshot.stage,
                snapshot.state,
                serde_json::to_string(&snapshot)?,
                snapshot.updated_at,
                id
            ],
        )?;
        transaction.commit()?;
        drop(connection);
        (self.observer)(&snapshot);
        Ok(snapshot)
    }

    pub fn get(&self, id: &str) -> Result<TaskSnapshot, AppError> {
        if let Some(snapshot) = self
            .foreground
            .lock()
            .map_err(|_| AppError::new("internal_error", "临时操作状态不可用。"))?
            .get(id)
            .cloned()
        {
            return Ok(snapshot);
        }
        let json: String = self
            .store
            .connection()?
            .query_row("SELECT snapshot_json FROM tasks WHERE id=?", [id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| AppError::new("not_found", "任务不存在。"))?;
        self.read_snapshot(&json)
    }

    fn read_snapshot(&self, json: &str) -> Result<TaskSnapshot, AppError> {
        let mut snapshot: TaskSnapshot = serde_json::from_str(json)?;
        // Older preview records can still identify their saved dialogue through the audio asset.
        if snapshot.subject.is_none()
            && snapshot.kind == "preview"
            && let Some(asset_id) = snapshot
                .result
                .as_ref()
                .and_then(|value| value["asset"]["id"].as_str())
        {
            snapshot.subject = self
                .store
                .connection()?
                .query_row(
                    "SELECT s.title || ' · ' || e.text FROM example_media m
                 JOIN examples e ON e.id=m.example_id JOIN sources s ON s.id=e.source_id
                 WHERE m.asset_id=? ORDER BY e.id LIMIT 1",
                    [asset_id],
                    |row| row.get(0),
                )
                .optional()?;
        }
        Ok(snapshot)
    }

    pub fn list(&self) -> Result<Vec<TaskSnapshot>, AppError> {
        let connection = self.store.connection()?;
        let mut query = connection.prepare(&format!(
            "SELECT snapshot_json FROM tasks
             WHERE (kind NOT IN ({FOREGROUND_KINDS}) OR state='failed') AND
               (state NOT IN ('succeeded','failed','cancelled')
                OR id IN (SELECT id FROM tasks WHERE state IN ('succeeded','failed','cancelled')
                          AND (kind NOT IN ({FOREGROUND_KINDS}) OR state='failed')
                          ORDER BY updated_at DESC,id DESC LIMIT 1))
             ORDER BY updated_at DESC,id DESC",
        ))?;
        let rows = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(query);
        drop(connection);
        rows.into_iter()
            .map(|json| self.read_snapshot(&json))
            .collect()
    }

    pub fn history(&self, offset: u32, limit: u32) -> Result<TaskHistoryPage, AppError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        let total = transaction.query_row(
            &format!(
                "SELECT COUNT(*) FROM tasks WHERE state IN ('succeeded','failed','cancelled')
                AND (kind NOT IN ({FOREGROUND_KINDS}) OR state='failed')"
            ),
            [],
            |row| row.get(0),
        )?;
        let rows = {
            let mut query = transaction.prepare(&format!(
                "SELECT snapshot_json FROM tasks WHERE state IN ('succeeded','failed','cancelled')
                 AND (kind NOT IN ({FOREGROUND_KINDS}) OR state='failed')
                 ORDER BY updated_at DESC,id DESC LIMIT ? OFFSET ?",
            ))?;
            query
                .query_map(params![limit.clamp(1, 100), offset], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        transaction.commit()?;
        drop(connection);
        let items = rows
            .into_iter()
            .map(|json| self.read_snapshot(&json))
            .collect::<Result<_, _>>()?;
        Ok(TaskHistoryPage { items, total })
    }

    // Shutdown must also cancel foreground work even though it is absent from the task page.
    pub fn active(&self) -> Result<Vec<TaskSnapshot>, AppError> {
        let mut active = self
            .foreground
            .lock()
            .map_err(|_| AppError::new("internal_error", "临时操作状态不可用。"))?
            .values()
            .filter(|task| !task.terminal())
            .cloned()
            .collect::<Vec<_>>();
        let rows = {
            let connection = self.store.connection()?;
            let mut query = connection.prepare("SELECT snapshot_json FROM tasks WHERE state NOT IN ('succeeded','failed','cancelled')")?;
            query
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for json in rows {
            active.push(self.read_snapshot(&json)?);
        }
        Ok(active)
    }

    pub fn cancel(&self, id: &str) -> Result<TaskSnapshot, AppError> {
        let current = self.get(id)?;
        if current.terminal() {
            return Ok(current);
        }
        let controls = self
            .controls
            .lock()
            .map_err(|_| AppError::new("internal_error", "任务取消状态不可用。"))?;
        let control = controls.get(id).ok_or_else(|| {
            AppError::new("interrupted", "当前进程没有此任务的执行器，请重新执行。")
        })?;
        control.store(true, Ordering::Relaxed);
        drop(controls);
        self.update(id, |snapshot| {
            if !snapshot.terminal() {
                snapshot.state = "cancel_requested".into();
                snapshot.message = "正在取消".into();
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn terminal_history_pages_keep_old_records_and_exclude_active_work() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let manager = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
        for index in 0..243 {
            let snapshot = TaskSnapshot {
                id: format!("task-{index:03}"),
                operation_id: Uuid::new_v4().to_string(),
                request_hash: "fixture".into(),
                kind: "import".into(),
                subject: Some(format!("Episode {index}.mkv")),
                stage: "file".into(),
                state: if index >= 241 {
                    "running"
                } else {
                    ["succeeded", "failed", "cancelled"][index % 3]
                }
                .into(),
                current: 1,
                total: 1,
                message: "fixture".into(),
                result: None,
                error: None,
                created_at: index as i64,
                // Ended tasks sort by completion time, including an old long-running import.
                updated_at: if index == 0 { 1000 } else { (index / 2) as i64 },
            };
            manager.save(&snapshot).unwrap();
        }
        let displayed = manager.list().unwrap();
        assert_eq!(displayed.len(), 3);
        assert_eq!(displayed.iter().filter(|task| task.terminal()).count(), 1);
        assert!(displayed.iter().any(|task| task.id == "task-000"));
        let mut all = Vec::new();
        for page in 0..25 {
            let history = manager.history(page * 10, 10).unwrap();
            assert_eq!(history.total, 241);
            assert_eq!(history.items.len(), if page == 24 { 1 } else { 10 });
            assert!(history.items.iter().all(TaskSnapshot::terminal));
            all.extend(history.items);
        }
        assert_eq!(all[0].id, "task-000");
        assert_eq!(all[1].id, "task-240");
        assert_eq!(all[2].id, "task-239");
        let ids = all
            .iter()
            .map(|task| &task.id)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), 241);
        assert_eq!(manager.history(241, 10).unwrap().items.len(), 0);
        assert_eq!(manager.history(0, 1000).unwrap().items.len(), 100);
        assert_eq!(manager.history(0, 0).unwrap().items.len(), 1);
        // A pre-update snapshot without the new field still deserializes.
        let mut legacy = serde_json::to_value(&all[1]).unwrap();
        legacy.as_object_mut().unwrap().remove("subject");
        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE tasks SET snapshot_json=? WHERE id=?",
                params![legacy.to_string(), all[1].id],
            )
            .unwrap();
        assert!(manager.get(&all[1].id).unwrap().subject.is_none());
    }

    #[test]
    fn legacy_successes_are_hidden_and_failed_subjects_survive_restart() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let manager = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
        let started = manager
            .start(
                "explanation",
                &Uuid::new_v4().to_string(),
                "target",
                |context| {
                    context.subject("reluctant");
                    Ok(serde_json::json!({"meaning":"不情愿的"}))
                },
            )
            .unwrap();
        let mut snapshot = wait(&manager, &started.id);
        assert_eq!(snapshot.subject.as_deref(), Some("reluctant"));
        store.connection().unwrap().execute_batch(
            "INSERT INTO sources(id,kind,title,fingerprint,created_at) VALUES ('source','video','Pilot','fixture',0);
             INSERT INTO examples(id,source_id,location_key,text,identity_key,start_ms,end_ms,created_at)
                 VALUES ('example','source','clip','I am reluctant.','text',0,1000,0);
             INSERT INTO media_assets(id,recipe_key,digest,relative_path,format,duration_ms,state,created_at)
                 VALUES ('asset','clip','digest','clip.wav','wav',1000,'ready',0);
             INSERT INTO example_media(example_id,asset_id) VALUES ('example','asset');"
        ).unwrap();
        snapshot.id = Uuid::new_v4().to_string();
        snapshot.kind = "preview".into();
        snapshot.subject = None;
        snapshot.result = Some(serde_json::json!({"asset":{"id":"asset","durationMs":1000}}));
        manager.persist(&snapshot).unwrap();
        let expected = Some("Pilot · I am reluctant.");
        assert_eq!(
            manager.get(&snapshot.id).unwrap().subject.as_deref(),
            expected
        );
        assert!(manager.list().unwrap().is_empty());
        assert_eq!(manager.history(0, 10).unwrap().total, 0);
        // The read-time fallback leaves the original persisted snapshot untouched.
        let persisted: String = store
            .connection()
            .unwrap()
            .query_row(
                "SELECT snapshot_json FROM tasks WHERE id=?",
                [&snapshot.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            serde_json::from_str::<TaskSnapshot>(&persisted)
                .unwrap()
                .subject
                .is_none()
        );
        snapshot.kind = "explanation".into();
        snapshot.subject = Some("reluctant".into());
        snapshot.state = "failed".into();
        snapshot.error = Some(AppError::new("provider_unavailable", "合成解释失败"));
        manager.save(&snapshot).unwrap();
        drop(manager);
        drop(store);
        let reopened = TaskManager::new(
            Arc::new(Store::open(directory.path()).unwrap()),
            Arc::new(|_| {}),
        );
        assert_eq!(
            reopened.get(&snapshot.id).unwrap().subject.as_deref(),
            Some("reluctant")
        );
        assert_eq!(reopened.history(0, 10).unwrap().total, 1);
    }

    #[test]
    fn repeated_foreground_successes_do_not_write_logs_or_displace_sync_history() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let manager = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
        let sync = manager
            .start("sync", &Uuid::new_v4().to_string(), "sync", |_| {
                Ok(serde_json::json!({"pushed":1}))
            })
            .unwrap();
        let sync = wait(&manager, &sync.id);
        for index in 0..100 {
            let task = manager
                .start(
                    ["preview", "speech", "review_audio", "explanation"][index % 4],
                    &Uuid::new_v4().to_string(),
                    "clip",
                    |_| Ok(serde_json::json!({"path":"synthetic.wav"})),
                )
                .unwrap();
            let result = wait(&manager, &task.id);
            assert_eq!(result.state, "succeeded");
            assert_eq!(result.result.unwrap()["path"], "synthetic.wav");
        }
        assert_eq!(
            manager.foreground.lock().unwrap().len(),
            FOREGROUND_RECEIPTS
        );
        assert_eq!(manager.list().unwrap()[0].id, sync.id);
        assert_eq!(manager.history(0, 10).unwrap().total, 1);
        assert_eq!(
            store
                .connection()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn foreground_work_remains_cancellable_and_errors_are_durable() {
        use std::sync::mpsc;
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let manager = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
        let operation = Uuid::new_v4().to_string();
        let (release, gate) = mpsc::channel();
        let task = manager
            .start("preview", &operation, "clip", move |context| {
                gate.recv().unwrap();
                context.check_cancelled()?;
                Ok(Value::Null)
            })
            .unwrap();
        assert_eq!(
            manager
                .start("preview", &operation, "clip", |_| panic!(
                    "Duplicate execution"
                ))
                .unwrap()
                .id,
            task.id
        );
        assert_eq!(
            manager
                .start("preview", &operation, "different", |_| Ok(Value::Null))
                .unwrap_err()
                .code,
            "conflict"
        );
        assert!(manager.list().unwrap().is_empty());
        assert_eq!(manager.active().unwrap()[0].id, task.id);
        manager.cancel(&task.id).unwrap();
        release.send(()).unwrap();
        assert_eq!(wait(&manager, &task.id).state, "cancelled");
        assert!(manager.active().unwrap().is_empty());
        let failed = manager
            .start(
                "preview",
                &Uuid::new_v4().to_string(),
                "bad-clip",
                |context| {
                    context.subject("Synthetic missing clip");
                    Err(AppError::new("media_missing", "合成原声失败"))
                },
            )
            .unwrap();
        assert_eq!(wait(&manager, &failed.id).state, "failed");
        assert_eq!(manager.history(0, 10).unwrap().total, 1);
        assert_eq!(
            store
                .connection()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(manager);
        drop(store);
        let reopened = TaskManager::new(
            Arc::new(Store::open(directory.path()).unwrap()),
            Arc::new(|_| {}),
        );
        assert_eq!(
            reopened.history(0, 10).unwrap().items[0]
                .error
                .as_ref()
                .unwrap()
                .code,
            "media_missing"
        );
        assert!(reopened.get(&task.id).is_err());
    }

    #[test]
    fn foreground_retry_reuses_the_new_execution_after_an_earlier_failure() {
        use std::sync::mpsc;
        let directory = tempfile::tempdir().unwrap();
        let manager = TaskManager::new(
            Arc::new(Store::open(directory.path()).unwrap()),
            Arc::new(|_| {}),
        );
        let operation = Uuid::new_v4().to_string();
        let failed = manager
            .start("preview", &operation, "clip", |_| {
                Err(AppError::new("media_missing", "合成失败"))
            })
            .unwrap();
        assert_eq!(wait(&manager, &failed.id).state, "failed");
        let (release, gate) = mpsc::channel();
        let retry = manager
            .start("preview", &operation, "clip", move |_| {
                gate.recv().unwrap();
                Ok(Value::Null)
            })
            .unwrap();
        for _ in 0..20 {
            assert_eq!(
                manager
                    .start("preview", &operation, "clip", |_| panic!(
                        "Must reuse active retry"
                    ))
                    .unwrap()
                    .id,
                retry.id
            );
        }
        release.send(()).unwrap();
        assert_eq!(wait(&manager, &retry.id).state, "succeeded");
        assert_eq!(
            manager
                .start("preview", &operation, "clip", |_| panic!(
                    "Must reuse successful retry"
                ))
                .unwrap()
                .id,
            retry.id
        );
    }

    #[test]
    fn older_running_and_cancelling_tasks_survive_the_recent_history_limit() {
        use std::sync::mpsc;
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let manager = TaskManager::new(store, Arc::new(|_| {}));
        let (started, ready) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let older = manager
            .start(
                "import",
                &Uuid::new_v4().to_string(),
                "older",
                move |context| {
                    started.send(()).unwrap();
                    gate.recv().unwrap();
                    context.check_cancelled()?;
                    Ok(serde_json::json!({}))
                },
            )
            .unwrap();
        ready.recv_timeout(Duration::from_secs(3)).unwrap();
        for _ in 0..35 {
            let recent = manager
                .start("fixture", &Uuid::new_v4().to_string(), "recent", |_| {
                    Ok(serde_json::json!({}))
                })
                .unwrap();
            assert_eq!(wait(&manager, &recent.id).state, "succeeded");
        }
        let listed = manager.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|task| task.id == older.id));
        manager.cancel(&older.id).unwrap();
        assert!(
            manager
                .list()
                .unwrap()
                .iter()
                .any(|task| { task.id == older.id && task.state == "cancel_requested" })
        );
        release.send(()).unwrap();
        assert_eq!(wait(&manager, &older.id).state, "cancelled");
        let listed = manager.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed.iter().all(TaskSnapshot::terminal));
        assert_eq!(listed[0].id, older.id);
        assert_eq!(manager.history(0, 10).unwrap().total, 36);
    }

    #[test]
    fn cancellation_at_collection_commit_boundary_keeps_truthful_state_and_one_receipt() {
        use crate::vocabulary::CollectionInput;
        use std::sync::mpsc;
        for after_commit in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let store = Arc::new(Store::open(directory.path()).unwrap());
            let manager = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
            let input = CollectionInput {
                operation_id: Uuid::new_v4().to_string(),
                kind: "word".into(),
                text: "reluctant".into(),
                meaning: "不情愿的".into(),
                examples: vec![],
                target_entry_id: None,
                expected_revision: None,
            };
            let (reached, boundary) = mpsc::channel();
            let (release, gate) = mpsc::channel();
            let worker_store = Arc::clone(&store);
            let worker_input = input.clone();
            let task = manager
                .start(
                    "collection",
                    &input.operation_id,
                    "fixed-collection",
                    move |context| {
                        if !after_commit {
                            reached.send(()).unwrap();
                            gate.recv().unwrap();
                            context.check_cancelled()?;
                        }
                        let result = worker_store.collect(&worker_input)?;
                        if after_commit {
                            reached.send(()).unwrap();
                            gate.recv().unwrap();
                        }
                        Ok(serde_json::to_value(result)?)
                    },
                )
                .unwrap();
            boundary.recv_timeout(Duration::from_secs(3)).unwrap();
            manager.cancel(&task.id).unwrap();
            release.send(()).unwrap();
            let completed = wait(&manager, &task.id);
            if after_commit {
                assert_eq!(
                    completed.state, "succeeded",
                    "A saved collection must not be reported as discarded"
                );
            } else {
                assert_eq!(completed.state, "cancelled");
                assert!(store.list_entries("", 0, 20).unwrap().is_empty());
                let retry_store = Arc::clone(&store);
                let retry_input = input.clone();
                let retry = manager
                    .start(
                        "collection",
                        &input.operation_id,
                        "fixed-collection",
                        move |_| Ok(serde_json::to_value(retry_store.collect(&retry_input)?)?),
                    )
                    .unwrap();
                assert_eq!(wait(&manager, &retry.id).state, "succeeded");
            }
            let repeat = manager
                .start(
                    "collection",
                    &input.operation_id,
                    "fixed-collection",
                    |_| panic!("The saved receipt must be reused"),
                )
                .unwrap();
            assert_eq!(repeat.state, "succeeded");
            let entries = store.list_entries("", 0, 20).unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].collection_count, 1);
        }
    }

    fn wait(manager: &TaskManager, id: &str) -> TaskSnapshot {
        let start = Instant::now();
        loop {
            let snapshot = manager.get(id).unwrap();
            if snapshot.terminal() {
                return snapshot;
            }
            assert!(start.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn startup_recovers_interrupted_tasks_and_retry_keeps_partial_result() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let manager = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
        let operation = Uuid::new_v4().to_string();
        let interrupted = TaskSnapshot {
            id: Uuid::new_v4().to_string(),
            operation_id: operation.clone(),
            request_hash: "recover".into(),
            kind: "import".into(),
            subject: None,
            stage: "file".into(),
            state: "running".into(),
            current: 1,
            total: 2,
            message: "processing".into(),
            result: Some(serde_json::json!({"saved":1})),
            error: None,
            created_at: 0,
            updated_at: 0,
        };
        manager.save(&interrupted).unwrap();
        manager.recover_interrupted().unwrap();
        let previous = manager.get(&interrupted.id).unwrap();
        assert_eq!(previous.state, "failed");
        assert_eq!(previous.error.unwrap().code, "interrupted");
        assert_eq!(previous.result.unwrap()["saved"], 1);
        let retry = manager
            .start("import", &operation, "recover", |_| {
                Ok(serde_json::json!({"saved":2}))
            })
            .unwrap();
        assert_ne!(retry.id, interrupted.id);
        assert_eq!(wait(&manager, &retry.id).state, "succeeded");
    }

    #[test]
    fn repeated_start_returns_one_successful_execution() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let manager = TaskManager::new(store, Arc::new(|_| {}));
        let operation = Uuid::new_v4().to_string();
        let first = manager
            .start("probe", &operation, "same", |_| {
                Ok(serde_json::json!({"value": 1}))
            })
            .unwrap();
        assert_eq!(wait(&manager, &first.id).state, "succeeded");
        let repeat = manager
            .start("probe", &operation, "same", |_| {
                panic!("must not execute twice")
            })
            .unwrap();
        assert_eq!(first.id, repeat.id);
        assert_eq!(
            manager
                .start("probe", &operation, "changed", |_| Ok(Value::Null))
                .unwrap_err()
                .code,
            "conflict"
        );
    }

    #[test]
    fn cancelling_preserves_partial_result_and_has_a_real_terminal_state() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let manager = TaskManager::new(store, Arc::new(|_| {}));
        let snapshot = manager
            .start("probe", &Uuid::new_v4().to_string(), "cancel", |context| {
                context.partial_result(serde_json::json!({"saved": 1}));
                loop {
                    context.check_cancelled()?;
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(25));
        manager.cancel(&snapshot.id).unwrap();
        let done = wait(&manager, &snapshot.id);
        assert_eq!(done.state, "cancelled");
        assert_eq!(done.result.unwrap()["saved"], 1);
    }
}
