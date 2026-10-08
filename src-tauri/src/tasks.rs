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

type Observer = Arc<dyn Fn(&TaskSnapshot) + Send + Sync>;

pub struct TaskManager {
    store: Arc<Store>,
    controls: Mutex<HashMap<String, Arc<AtomicBool>>>,
    start_guard: Mutex<()>,
    observer: Observer,
}

#[derive(Clone)]
pub struct TaskContext {
    manager: Arc<TaskManager>,
    pub task_id: String,
    pub cancelled: Arc<AtomicBool>,
}

impl TaskContext {
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
        let existing = {
            let connection = self.store.connection()?;
            connection.query_row("SELECT snapshot_json FROM tasks WHERE operation_id=? AND kind=? ORDER BY created_at DESC,id LIMIT 1", params![operation_id,kind], |row| row.get::<_, String>(0)).optional()?
        };
        if let Some(json) = existing {
            let snapshot: TaskSnapshot = serde_json::from_str(&json)?;
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
        self.store.connection()?.execute("INSERT INTO tasks(id,operation_id,kind,stage,state,snapshot_json,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET stage=excluded.stage,state=excluded.state,snapshot_json=excluded.snapshot_json,updated_at=excluded.updated_at", params![snapshot.id,snapshot.operation_id,snapshot.kind,snapshot.stage,snapshot.state,serde_json::to_string(snapshot)?,snapshot.created_at,snapshot.updated_at])?;
        (self.observer)(snapshot);
        Ok(())
    }

    fn update(
        &self,
        id: &str,
        change: impl FnOnce(&mut TaskSnapshot),
    ) -> Result<TaskSnapshot, AppError> {
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
        let json: String = self
            .store
            .connection()?
            .query_row("SELECT snapshot_json FROM tasks WHERE id=?", [id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| AppError::new("not_found", "任务不存在。"))?;
        Ok(serde_json::from_str(&json)?)
    }

    pub fn list(&self) -> Result<Vec<TaskSnapshot>, AppError> {
        let connection = self.store.connection()?;
        let mut query = connection.prepare(
            "SELECT snapshot_json FROM tasks
             WHERE state NOT IN ('succeeded','failed','cancelled')
                OR id IN (SELECT id FROM tasks ORDER BY created_at DESC,id LIMIT 30)
             ORDER BY created_at DESC,id",
        )?;
        let rows = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|json| Ok(serde_json::from_str(&json)?))
            .collect()
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
        assert_eq!(listed.len(), 31);
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
        assert_eq!(listed.len(), 30);
        assert!(listed.iter().all(TaskSnapshot::terminal));
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
