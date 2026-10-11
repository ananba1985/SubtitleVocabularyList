use crate::{error::AppError, tasks::TaskContext};
use std::{
    collections::HashSet,
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

pub(crate) struct ModelGate {
    limit: AtomicUsize,
    active: Mutex<ActiveRequests>,
    available: Condvar,
}

pub(crate) struct ModelPermit<'a> {
    gate: &'a ModelGate,
    keys: HashSet<(String, String)>,
}

#[derive(Default)]
struct ActiveRequests {
    requests: usize,
    keys: HashSet<(String, String)>,
    interactive_waiting: usize,
}

impl ModelGate {
    pub fn new() -> Self {
        Self {
            limit: AtomicUsize::new(1),
            active: Mutex::new(ActiveRequests::default()),
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
        self.acquire_many(&[text.to_owned()], context_text, context)
    }

    pub fn acquire_many(
        &self,
        texts: &[String],
        context_text: &str,
        context: &TaskContext,
    ) -> Result<ModelPermit<'_>, AppError> {
        self.acquire_inner(texts, context_text, false, || context.check_cancelled())
    }

    pub fn acquire_interactive(
        &self,
        text: &str,
        context_text: &str,
        cancelled: &AtomicBool,
    ) -> Result<ModelPermit<'_>, AppError> {
        self.acquire_inner(&[text.to_owned()], context_text, true, || {
            if cancelled.load(Ordering::Relaxed) {
                Err(AppError::new("cancelled", "查询已取消。"))
            } else {
                Ok(())
            }
        })
    }

    fn acquire_inner(
        &self,
        texts: &[String],
        context_text: &str,
        interactive: bool,
        check: impl Fn() -> Result<(), AppError>,
    ) -> Result<ModelPermit<'_>, AppError> {
        let keys = texts
            .iter()
            .map(|text| (text.trim().to_owned(), context_text.to_owned()))
            .collect::<HashSet<_>>();
        let mut active = self
            .active
            .lock()
            .map_err(|_| AppError::new("internal_error", "本地模型执行状态不可用。"))?;
        if interactive {
            active.interactive_waiting += 1;
        }
        loop {
            if let Err(error) = check() {
                if interactive {
                    active.interactive_waiting -= 1;
                    self.available.notify_all();
                }
                return Err(error);
            }
            if active.requests < self.limit()
                && keys.is_disjoint(&active.keys)
                && (interactive || active.interactive_waiting == 0)
            {
                if interactive {
                    active.interactive_waiting -= 1;
                }
                active.keys.extend(keys.iter().cloned());
                active.requests += 1;
                return Ok(ModelPermit { gate: self, keys });
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
            for key in &self.keys {
                active.keys.remove(key);
            }
            active.requests -= 1;
            self.gate.available.notify_all();
        }
    }
}
