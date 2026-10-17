//! A task handle carries its eventual result type without changing task JSON.
use super::{TaskContext, TaskManager, TaskSnapshot};
use crate::error::AppError;
use serde::{Serialize, de::DeserializeOwned};
use std::{marker::PhantomData, ops::Deref, sync::Arc};

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct TaskHandle<T> {
    snapshot: TaskSnapshot,
    #[serde(skip)]
    marker: PhantomData<T>,
}

impl<T> Deref for TaskHandle<T> {
    type Target = TaskSnapshot;
    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl<T: Serialize + DeserializeOwned> TaskHandle<T> {
    pub(crate) fn new(snapshot: TaskSnapshot) -> Result<Self, AppError> {
        // A repeated start can return an old completed receipt. Keep its fields intact.
        if snapshot.state == "succeeded" {
            let raw = snapshot
                .result
                .as_ref()
                .ok_or_else(|| AppError::new("internal_error", "任务缺少完成结果。"))?;
            let result: T = serde_json::from_value(raw.clone())?;
            if serde_json::to_value(result)? != *raw {
                return Err(AppError::new(
                    "internal_error",
                    "任务结果与当前接口契约不一致。",
                ));
            }
        }
        Ok(Self {
            snapshot,
            marker: PhantomData,
        })
    }
}

impl TaskManager {
    pub fn start_typed<T, F>(
        self: &Arc<Self>,
        kind: &str,
        operation: &str,
        hash: &str,
        worker: F,
    ) -> Result<TaskHandle<T>, AppError>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce(TaskContext) -> Result<T, AppError> + Send + 'static,
    {
        TaskHandle::new(self.start(kind, operation, hash, move |context| {
            Ok(serde_json::to_value(worker(context)?)?)
        })?)
    }
}

// The brand represents PhantomData, never a serialized field or a second result copy.
#[cfg(feature = "ipc-types")]
impl<T: ts_rs::TS> ts_rs::TS for TaskHandle<T> {
    type WithoutGenerics = TaskHandle<ts_rs::Dummy>;
    type OptionInnerType = Self;
    fn name(cfg: &ts_rs::Config) -> String {
        format!("TaskHandle<{}>", T::name(cfg))
    }
    fn decl(_: &ts_rs::Config) -> String {
        "type TaskHandle<T> = TaskSnapshot & { readonly __taskResult: T };".into()
    }
    fn decl_concrete(cfg: &ts_rs::Config) -> String {
        format!("type TaskHandleResult = {};", Self::inline(cfg))
    }
    fn inline(cfg: &ts_rs::Config) -> String {
        format!(
            "TaskSnapshot & {{ readonly __taskResult: {} }}",
            T::name(cfg)
        )
    }
    fn visit_dependencies(v: &mut impl ts_rs::TypeVisitor)
    where
        Self: 'static,
    {
        v.visit::<TaskSnapshot>();
    }
    fn visit_generics(v: &mut impl ts_rs::TypeVisitor)
    where
        Self: 'static,
    {
        v.visit::<T>();
    }
    fn output_path() -> Option<std::path::PathBuf> {
        Some("TaskHandle.ts".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{store::Store, tasks::results::AudioPath};

    #[test]
    fn typed_task_keeps_progress_and_reuses_normal_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let events = Arc::clone(&seen);
        let manager = TaskManager::new(
            store,
            Arc::new(move |snapshot| events.lock().unwrap().push(snapshot.clone())),
        );
        let operation = uuid::Uuid::new_v4().to_string();
        let started = manager
            .start_typed("speech", &operation, "typed-normal", |context| {
                context.partial_result(serde_json::json!({"preparing":true}));
                Ok(AudioPath {
                    path: "synthetic.wav".into(),
                })
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(&started).unwrap(),
            serde_json::to_value(&*started).unwrap()
        );
        loop {
            let snapshot = manager.get(&started.id).unwrap();
            if snapshot.terminal() {
                assert_eq!(snapshot.state, "succeeded");
                assert_eq!(
                    snapshot.result.unwrap(),
                    serde_json::json!({"path":"synthetic.wav"})
                );
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let repeated = manager
            .start_typed("speech", &operation, "typed-normal", |_| {
                Ok(AudioPath {
                    path: "unused.wav".into(),
                })
            })
            .unwrap();
        assert_eq!(started.id, repeated.id);
        assert_eq!(repeated.result.as_ref().unwrap()["path"], "synthetic.wav");
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .any(|s| s.result.as_ref().is_some_and(|v| v["preparing"] == true))
        );
    }
}
