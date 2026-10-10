use crate::{error::AppError, tasks::TaskContext};
use std::{
    collections::HashSet,
    sync::{
        Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

pub(crate) struct ModelGate {
    limit: AtomicUsize,
    active: Mutex<HashSet<(String, String)>>,
    available: Condvar,
}

pub(crate) struct ModelPermit<'a> {
    gate: &'a ModelGate,
    key: (String, String),
}

impl ModelGate {
    pub fn new() -> Self {
        Self {
            limit: AtomicUsize::new(1),
            active: Mutex::new(HashSet::new()),
            available: Condvar::new(),
        }
    }

    pub fn limit(&self) -> usize {
        self.limit.load(Ordering::Relaxed)
    }

    pub fn set_limit(&self, limit: usize) {
        self.limit.store(limit, Ordering::Relaxed);
        self.available.notify_all();
    }

    pub fn acquire(
        &self,
        text: &str,
        context_text: &str,
        context: &TaskContext,
    ) -> Result<ModelPermit<'_>, AppError> {
        let key = (text.trim().to_owned(), context_text.to_owned());
        let mut active = self
            .active
            .lock()
            .map_err(|_| AppError::new("internal_error", "本地模型执行状态不可用。"))?;
        loop {
            context.check_cancelled()?;
            if active.len() < self.limit() && !active.contains(&key) {
                active.insert(key.clone());
                return Ok(ModelPermit { gate: self, key });
            }
            active = self
                .available
                .wait_timeout(active, Duration::from_millis(100))
                .map_err(|_| AppError::new("internal_error", "本地模型执行状态不可用。"))?
                .0;
        }
    }
}

impl Drop for ModelPermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.gate.active.lock() {
            active.remove(&self.key);
            self.gate.available.notify_all();
        }
    }
}
