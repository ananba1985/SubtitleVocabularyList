//! Opt-in, bounded per-operation timings. Never records user data or identifiers.
use serde::Serialize;
use std::{cell::RefCell, collections::BTreeMap, sync::OnceLock, time::Instant};

#[derive(Clone, Default, Serialize)]
pub(crate) struct Timing {
    pub calls: u64,
    pub total_us: u64,
    pub max_us: u64,
}
#[derive(Clone, Serialize)]
pub(crate) struct Sample {
    pub operation: &'static str,
    pub elapsed_us: u64,
    pub timings: BTreeMap<&'static str, Timing>,
}
struct Active {
    operation: &'static str,
    started: Instant,
    depth: usize,
    timings: BTreeMap<&'static str, Timing>,
}
#[derive(Default)]
struct State {
    active: Option<Active>,
    #[cfg(test)]
    capture: bool,
    #[cfg(test)]
    samples: Vec<Sample>,
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SVL_LOCAL_PROFILE").is_some_and(|v| v == "1"))
}
pub(crate) struct Timer {
    name: &'static str,
    started: Option<Instant>,
}
pub(crate) fn phase(name: &'static str) -> Timer {
    Timer {
        name,
        started: STATE.with(|s| s.borrow().active.as_ref().map(|_| Instant::now())),
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let us = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            STATE.with(|s| {
                if let Some(active) = s.borrow_mut().active.as_mut() {
                    let timing = active.timings.entry(self.name).or_default();
                    timing.calls += 1;
                    timing.total_us += us;
                    timing.max_us = timing.max_us.max(us);
                }
            });
        }
    }
}
pub(crate) struct Operation {
    timer: Option<Timer>,
}
pub(crate) fn operation(name: &'static str) -> Operation {
    let on = enabled() || {
        #[cfg(test)]
        {
            STATE.with(|s| s.borrow().capture)
        }
        #[cfg(not(test))]
        {
            false
        }
    };
    if !on {
        return Operation { timer: None };
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let Some(active) = s.active.as_mut() {
            active.depth += 1;
        } else {
            s.active = Some(Active {
                operation: name,
                started: Instant::now(),
                depth: 1,
                timings: BTreeMap::new(),
            });
        }
    });
    Operation {
        timer: Some(phase(name)),
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if self.timer.is_none() {
            return;
        }
        drop(self.timer.take());
        let sample = STATE.with(|s| {
            let mut s = s.borrow_mut();
            let active = s.active.as_mut()?;
            active.depth -= 1;
            if active.depth != 0 {
                return None;
            }
            let active = s.active.take().unwrap();
            let sample = Sample {
                operation: active.operation,
                elapsed_us: active.started.elapsed().as_micros().min(u64::MAX as u128) as u64,
                timings: active.timings,
            };
            #[cfg(test)]
            if s.capture {
                s.samples.push(sample.clone());
            }
            Some(sample)
        });
        if enabled() {
            if let Some(sample) = sample {
                eprintln!(
                    "[svl-local-profile] {}",
                    serde_json::to_string(&sample).unwrap()
                );
            }
        }
    }
}
#[cfg(test)]
pub(crate) fn capture() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.capture = true;
        s.samples.clear();
    });
}
#[cfg(test)]
pub(crate) fn take_samples() -> Vec<Sample> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.capture = false;
        std::mem::take(&mut s.samples)
    })
}
