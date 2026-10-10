use crate::{
    application::Application,
    error::AppError,
    store::Store,
    tasks::{TaskContext, TaskSnapshot},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

const ENABLED_KEY: &str = "explanation_preparation_enabled";
const TARGETS_PER_REQUEST: usize = 8;

#[derive(Clone)]
struct PreparationBatch {
    inputs: Vec<PreparationInput>,
}

fn group_batches(inputs: Vec<PreparationInput>) -> VecDeque<PreparationBatch> {
    let mut batches = Vec::<PreparationBatch>::new();
    let mut indexes = HashMap::<(String, String), usize>::new();
    for input in inputs {
        let key = (input.source_id.clone(), input.context.clone());
        let index = match indexes.get(&key).copied() {
            Some(index) if batches[index].inputs.len() < TARGETS_PER_REQUEST => index,
            _ => {
                let index = batches.len();
                batches.push(PreparationBatch { inputs: Vec::new() });
                indexes.insert(key, index);
                index
            }
        };
        batches[index].inputs.push(input);
    }
    batches.into()
}

fn retry_delay(attempt: u32) -> Duration {
    Duration::from_secs(2u64.pow(attempt.min(6)).min(60))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationInput {
    pub source_id: String,
    pub title: String,
    pub kind: String,
    pub text: String,
    pub context: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationReply {
    pub task: Option<TaskSnapshot>,
}

#[derive(Serialize)]
pub struct PreparationStatus {
    pub state: &'static str,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceProgress {
    source_id: String,
    title: String,
    current: usize,
    total: usize,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparationProgress {
    generated: usize,
    reused: usize,
    skipped: usize,
    failed: usize,
    sources: Vec<SourceProgress>,
    failures: Vec<serde_json::Value>,
    concurrency: usize,
    in_flight: usize,
    waiting: usize,
    batches_in_flight: usize,
    batch_size: usize,
}

struct ActivePreparation {
    batch: PreparationBatch,
    stage: &'static str,
}

enum PreparationEvent {
    Progress {
        id: usize,
        stage: &'static str,
        message: String,
    },
    Finished {
        id: usize,
        result: Result<Vec<Result<Option<bool>, AppError>>, AppError>,
    },
}

impl Store {
    pub fn preparation_enabled(&self) -> Result<bool, AppError> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT value_json FROM settings WHERE key=?",
                [ENABLED_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .as_deref()
            != Some("false"))
    }

    pub fn set_preparation_enabled(&self, enabled: bool) -> Result<(), AppError> {
        self.connection()?.execute("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json", params![ENABLED_KEY, enabled.to_string()])?;
        Ok(())
    }

    pub fn explanation_work(
        &self,
        source_id: Option<&str>,
    ) -> Result<Vec<PreparationInput>, AppError> {
        if let Some(id) = source_id {
            self.source(id)?;
        }
        let connection = self.connection()?;
        let mut query = connection.prepare(
            "WITH candidates AS (
                SELECT source_id,candidate_key,kind,MIN(raw_text) target FROM occurrences
                WHERE (? IS NULL OR source_id=?) GROUP BY source_id,candidate_key,kind
             )
             SELECT DISTINCT c.source_id,s.title,c.kind,c.target,e.text
             FROM candidates c JOIN sources s ON s.id=c.source_id
             JOIN occurrences o ON o.source_id=c.source_id AND o.candidate_key=c.candidate_key AND o.kind=c.kind
             JOIN examples e ON e.id=o.example_id
             LEFT JOIN settings k ON k.key=? || c.kind || ':' || c.candidate_key
             WHERE s.corpus_path IS NOT NULL AND e.archived=0 AND k.key IS NULL
               AND NOT EXISTS(SELECT 1 FROM explanations x WHERE x.target=c.target AND x.context=e.text)
             ORDER BY s.imported_at DESC,c.source_id,length(c.target) DESC,c.target,e.start_ms")?;
        Ok(query
            .query_map(
                params![source_id, source_id, crate::known_targets::PREFIX],
                |row| {
                    Ok(PreparationInput {
                        source_id: row.get(0)?,
                        title: row.get(1)?,
                        kind: row.get(2)?,
                        text: row.get(3)?,
                        context: row.get(4)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?)
    }
}

impl Application {
    pub fn preparation_status(&self) -> Result<PreparationStatus, AppError> {
        let state = if !self.store.preparation_enabled()? {
            "paused"
        } else if self.preparation_paused.load(Ordering::Relaxed) {
            "failed"
        } else if self.preparation_requested.load(Ordering::Relaxed)
            || self.preparation_guard.try_lock().is_err()
            || self
                .tasks
                .active()?
                .iter()
                .any(|task| task.kind == "explanation_batch")
        {
            "running"
        } else {
            "idle"
        };
        Ok(PreparationStatus { state })
    }
    pub fn start_preparation_scheduler(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        std::thread::spawn(move || {
            loop {
                let Some(app) = weak.upgrade() else {
                    break;
                };
                if app.preparation_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                if app.preparation_requested.load(Ordering::Relaxed)
                    && !app.preparation_paused.load(Ordering::Relaxed)
                    && app.store.preparation_enabled().unwrap_or(false)
                    && app.tasks.active().is_ok_and(|tasks| {
                        !tasks.iter().any(|task| task.kind == "explanation_batch")
                    })
                {
                    app.preparation_requested.store(false, Ordering::Relaxed);
                    if let Err(error) = app.prepare_explanations(None, false) {
                        eprintln!("[preparation] {}", error.code);
                        app.preparation_paused.store(true, Ordering::Relaxed);
                    }
                }
                drop(app);
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    }

    pub fn stop_preparation_scheduler(&self) {
        self.preparation_shutdown.store(true, Ordering::Relaxed);
    }

    pub fn pause_preparation(&self) -> Result<(), AppError> {
        let _guard = self
            .preparation_guard
            .lock()
            .map_err(|_| AppError::new("internal_error", "中文资料准备状态不可用。"))?;
        self.store.set_preparation_enabled(false)?;
        self.preparation_paused.store(true, Ordering::Relaxed);
        for task in self
            .tasks
            .active()?
            .into_iter()
            .filter(|task| task.kind == "explanation_batch")
        {
            self.tasks.cancel(&task.id)?;
        }
        Ok(())
    }

    pub fn prepare_explanations(
        self: &Arc<Self>,
        source_id: Option<String>,
        explicit: bool,
    ) -> Result<PreparationReply, AppError> {
        let _guard = self
            .preparation_guard
            .lock()
            .map_err(|_| AppError::new("internal_error", "中文资料准备状态不可用。"))?;
        if self.preparation_shutdown.load(Ordering::Relaxed) {
            return Err(AppError::new("cancelled", "应用正在退出。"));
        }
        if !explicit
            && (self.preparation_paused.load(Ordering::Relaxed)
                || !self.store.preparation_enabled()?)
        {
            return Ok(PreparationReply { task: None });
        }
        let scoped = source_id
            .as_deref()
            .map(|id| self.store.explanation_work(Some(id)))
            .transpose()?;
        if scoped.as_ref().is_some_and(|inputs| inputs.is_empty()) {
            return Ok(PreparationReply { task: None });
        }
        if explicit {
            self.store.set_preparation_enabled(true)?;
            self.preparation_paused.store(false, Ordering::Relaxed);
            self.preparation_requested.store(true, Ordering::Relaxed);
            *self
                .preparation_priority
                .lock()
                .map_err(|_| AppError::new("internal_error", "剧集优先级不可用。"))? =
                source_id.clone();
        }
        if let Some(task) = self
            .tasks
            .active()?
            .into_iter()
            .find(|task| task.kind == "explanation_batch")
        {
            if explicit {
                self.preparation_retry_epoch.fetch_add(1, Ordering::Relaxed);
            }
            return Ok(PreparationReply { task: Some(task) });
        }
        let inputs = match scoped {
            Some(inputs) => inputs,
            None => self.store.explanation_work(None)?,
        };
        if inputs.is_empty() {
            return Ok(PreparationReply { task: None });
        }
        let hash = crate::vocabulary::digest(&serde_json::to_vec(&inputs)?);
        let app = Arc::clone(self);
        let task = self.tasks.start(
            "explanation_batch",
            &Uuid::new_v4().to_string(),
            &hash,
            move |context| app.run_preparation(inputs, context),
        )?;
        Ok(PreparationReply { task: Some(task) })
    }

    fn wait_preparation_retry(
        &self,
        context: &TaskContext,
        delay: Duration,
        epoch: u64,
    ) -> Result<(), AppError> {
        let until = Instant::now() + delay;
        loop {
            context.check_cancelled()?;
            if self.preparation_shutdown.load(Ordering::Relaxed) {
                return Err(AppError::new("cancelled", "应用正在退出。"));
            }
            if self.preparation_retry_epoch.load(Ordering::Relaxed) != epoch {
                break;
            }
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            std::thread::sleep(remaining.min(Duration::from_millis(100)));
        }
        Ok(())
    }

    fn explain_prepared(
        &self,
        input: &PreparationInput,
        context: &TaskContext,
        report: impl Fn(&'static str, String),
    ) -> Result<Option<bool>, AppError> {
        let mut connection_retries: u32 = 0;
        let mut output_retried = false;
        loop {
            let epoch = self.preparation_retry_epoch.load(Ordering::Relaxed);
            context.check_cancelled()?;
            if self.preparation_shutdown.load(Ordering::Relaxed) {
                return Err(AppError::new("cancelled", "应用正在退出。"));
            }
            if self.store.is_known_target(&input.kind, &input.text)? {
                return Ok(None);
            }
            report("translations", format!("{} · {}", input.title, input.text));
            let error = match self.explain_value(&input.text, &input.context, context) {
                Ok((_, created)) => return Ok(Some(created)),
                Err(error) => error,
            };
            let (delay, message) = if error.code == "provider_unavailable" && error.retryable {
                connection_retries = connection_retries.saturating_add(1);
                let delay = retry_delay(connection_retries);
                (
                    delay,
                    format!(
                        "{} · {} 秒后自动重试（第 {} 次），服务恢复后继续。{}",
                        input.title,
                        delay.as_secs(),
                        connection_retries,
                        error.message
                    ),
                )
            } else if error.code == "invalid_data" && !output_retried {
                output_retried = true;
                connection_retries = 0;
                (
                    Duration::from_secs(1),
                    format!(
                        "{} · {} 的输出无效，1 秒后补试一次。{}",
                        input.title, input.text, error.message
                    ),
                )
            } else {
                return Err(error);
            };
            report("retry_wait", message);
            self.wait_preparation_retry(context, delay, epoch)?;
        }
    }

    fn explain_prepared_batch(
        &self,
        batch: &PreparationBatch,
        context: &TaskContext,
        report: impl Fn(&'static str, String),
    ) -> Result<Vec<Result<Option<bool>, AppError>>, AppError> {
        if batch.inputs.len() == 1 {
            return Ok(vec![self.explain_prepared(
                &batch.inputs[0],
                context,
                report,
            )]);
        }
        let first = &batch.inputs[0];
        let mut outcomes = vec![None; batch.inputs.len()];
        let mut connection_retries: u32 = 0;
        let mut output_retried = false;
        loop {
            let epoch = self.preparation_retry_epoch.load(Ordering::Relaxed);
            context.check_cancelled()?;
            if self.preparation_shutdown.load(Ordering::Relaxed) {
                return Err(AppError::new("cancelled", "应用正在退出。"));
            }
            let mut texts = Vec::new();
            let mut unique = HashSet::new();
            for (index, input) in batch.inputs.iter().enumerate() {
                if outcomes[index].is_some() {
                    continue;
                }
                if let Err(error) = crate::explanations::validate_input(&input.text, &input.context)
                {
                    outcomes[index] = Some(Err(error));
                } else if self.store.is_known_target(&input.kind, &input.text)? {
                    outcomes[index] = Some(Ok(None));
                } else if unique.insert(input.text.trim().to_owned()) {
                    texts.push(input.text.trim().to_owned());
                }
            }
            if texts.is_empty() {
                return Ok(outcomes.into_iter().map(Option::unwrap).collect());
            }
            report(
                "translations",
                format!("{} · 同句准备 {} 个词", first.title, texts.len()),
            );
            let mut retry_error = None;
            match self.explain_many(&texts, &first.context, context) {
                Ok(mut values) => {
                    for (index, input) in batch.inputs.iter().enumerate() {
                        if outcomes[index].is_some() {
                            continue;
                        }
                        let result = values.remove(input.text.trim()).unwrap_or_else(|| {
                            self.store
                                .explanation(&input.text, &input.context)
                                .and_then(|value| {
                                    if value.is_some() {
                                        Ok(false)
                                    } else {
                                        Err(AppError::new(
                                            "invalid_data",
                                            "批量结果缺少当前词语解释。",
                                        ))
                                    }
                                })
                        });
                        match result {
                            Err(error)
                                if (error.code == "provider_unavailable" && error.retryable)
                                    || (error.code == "invalid_data" && !output_retried) =>
                            {
                                retry_error = Some(error)
                            }
                            value => outcomes[index] = Some(value.map(Some)),
                        }
                    }
                }
                Err(error) => retry_error = Some(error),
            }
            let Some(error) = retry_error else {
                return Ok(outcomes.into_iter().map(Option::unwrap).collect());
            };
            let (delay, message) = if error.code == "provider_unavailable" && error.retryable {
                connection_retries = connection_retries.saturating_add(1);
                let delay = retry_delay(connection_retries);
                (
                    delay,
                    format!(
                        "{} · {} 秒后自动重试同句缺失资料（第 {} 次）。{}",
                        first.title,
                        delay.as_secs(),
                        connection_retries,
                        error.message
                    ),
                )
            } else if error.code == "invalid_data" && !output_retried {
                output_retried = true;
                connection_retries = 0;
                (
                    Duration::from_secs(1),
                    format!(
                        "{} · 1 秒后补试本句未完成的词语。{}",
                        first.title, error.message
                    ),
                )
            } else {
                for value in &mut outcomes {
                    if value.is_none() {
                        *value = Some(Err(error.clone()));
                    }
                }
                return Ok(outcomes.into_iter().map(Option::unwrap).collect());
            };
            report("retry_wait", message);
            self.wait_preparation_retry(context, delay, epoch)?;
        }
    }

    fn publish_preparation(
        &self,
        context: &TaskContext,
        current: usize,
        total: usize,
        progress: &mut PreparationProgress,
        active: &HashMap<usize, ActivePreparation>,
        message: &str,
    ) {
        progress.concurrency = self.model_gate.limit();
        progress.in_flight = active.values().map(|job| job.batch.inputs.len()).sum();
        progress.waiting = active
            .values()
            .filter(|job| job.stage == "retry_wait")
            .map(|job| job.batch.inputs.len())
            .sum();
        progress.batches_in_flight = active.len();
        progress.batch_size = TARGETS_PER_REQUEST;
        let stage = if !active.is_empty() && progress.waiting == progress.in_flight {
            "retry_wait"
        } else {
            "translations"
        };
        context.partial_result(json!({"preparation":progress}));
        context.progress(
            stage,
            current,
            total,
            &format!(
                "并发上限 {} · 执行中 {} 批/{} 项 · 等待重试 {} 项 · {}",
                progress.concurrency,
                progress.batches_in_flight,
                progress.in_flight,
                progress.waiting,
                message
            ),
        );
    }

    fn run_preparation(
        &self,
        inputs: Vec<PreparationInput>,
        context: TaskContext,
    ) -> Result<serde_json::Value, AppError> {
        let mut total = inputs.len();
        let mut progress = PreparationProgress::default();
        let mut indexes = HashMap::new();
        for input in &inputs {
            let index = *indexes.entry(input.source_id.clone()).or_insert_with(|| {
                let index = progress.sources.len();
                progress.sources.push(SourceProgress {
                    source_id: input.source_id.clone(),
                    title: input.title.clone(),
                    ..Default::default()
                });
                index
            });
            progress.sources[index].total += 1;
        }
        let mut seen = inputs
            .iter()
            .map(|input| {
                (
                    input.source_id.clone(),
                    input.kind.clone(),
                    input.text.clone(),
                    input.context.clone(),
                )
            })
            .collect::<HashSet<_>>();
        let mut queue = group_batches(inputs);
        let mut active = HashMap::<usize, ActivePreparation>::new();
        let (mut current, mut next_id, mut workers) = (0, 0, 0);
        let (work_sender, work_receiver) = mpsc::channel::<(usize, PreparationBatch)>();
        let work_receiver = Arc::new(Mutex::new(work_receiver));
        let (events, receiver) = mpsc::channel::<PreparationEvent>();
        let stopped = Arc::new(AtomicBool::new(false));
        context.subject(&format!("中文资料准备 · {} 份剧集", progress.sources.len()));
        self.publish_preparation(
            &context,
            current,
            total,
            &mut progress,
            &active,
            "正在安排中文资料",
        );
        std::thread::scope(|scope| {
            let result = (|| {
                loop {
                    context.check_cancelled()?;
                    if self.preparation_shutdown.load(Ordering::Relaxed) {
                        return Err(AppError::new("cancelled", "应用正在退出。"));
                    }
                    if self.preparation_requested.swap(false, Ordering::Relaxed) {
                        let mut added = Vec::new();
                        for input in self.store.explanation_work(None)? {
                            let key = (
                                input.source_id.clone(),
                                input.kind.clone(),
                                input.text.clone(),
                                input.context.clone(),
                            );
                            if seen.insert(key) {
                                let index =
                                    *indexes.entry(input.source_id.clone()).or_insert_with(|| {
                                        let index = progress.sources.len();
                                        progress.sources.push(SourceProgress {
                                            source_id: input.source_id.clone(),
                                            title: input.title.clone(),
                                            ..Default::default()
                                        });
                                        index
                                    });
                                progress.sources[index].total += 1;
                                total += 1;
                                added.push(input);
                            }
                        }
                        queue.extend(group_batches(added));
                        context
                            .subject(&format!("中文资料准备 · {} 份剧集", progress.sources.len()));
                    }
                    let concurrency = self.model_gate.limit();
                    while workers < concurrency {
                        let work = Arc::clone(&work_receiver);
                        let events = events.clone();
                        let mut worker_context = context.clone();
                        worker_context.cancelled = Arc::clone(&stopped);
                        scope.spawn(move || {
                            while !worker_context.cancelled.load(Ordering::Relaxed) {
                                let next = {
                                    let work = work.lock().expect("preparation receiver lock");
                                    if worker_context.cancelled.load(Ordering::Relaxed) {
                                        break;
                                    }
                                    work.recv_timeout(Duration::from_millis(100))
                                };
                                let (id, batch) = match next {
                                    Ok(value) => value,
                                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                };
                                let output = self.explain_prepared_batch(
                                    &batch,
                                    &worker_context,
                                    |stage, message| {
                                        let _ = events.send(PreparationEvent::Progress {
                                            id,
                                            stage,
                                            message,
                                        });
                                    },
                                );
                                let _ =
                                    events.send(PreparationEvent::Finished { id, result: output });
                            }
                        });
                        workers += 1;
                    }
                    while active.len() < concurrency && !queue.is_empty() {
                        let index = {
                            let mut priority = self.preparation_priority.lock().map_err(|_| {
                                AppError::new("internal_error", "剧集优先级不可用。")
                            })?;
                            let index = priority.as_ref().and_then(|id| {
                                queue
                                    .iter()
                                    .position(|batch| &batch.inputs[0].source_id == id)
                            });
                            if index.is_none() {
                                *priority = None;
                            }
                            index.unwrap_or(0)
                        };
                        let batch = queue.remove(index).unwrap();
                        next_id += 1;
                        active.insert(
                            next_id,
                            ActivePreparation {
                                batch: batch.clone(),
                                stage: "translations",
                            },
                        );
                        work_sender
                            .send((next_id, batch))
                            .map_err(|_| AppError::new("internal_error", "中文准备队列不可用。"))?;
                    }
                    if active.is_empty() && queue.is_empty() {
                        break;
                    }
                    let event = match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(event) => event,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if progress.concurrency != concurrency {
                                self.publish_preparation(
                                    &context,
                                    current,
                                    total,
                                    &mut progress,
                                    &active,
                                    "已应用新的并发上限",
                                );
                            }
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            return Err(AppError::new("internal_error", "中文准备结果不可用。"));
                        }
                    };
                    let message = match event {
                        PreparationEvent::Progress { id, stage, message } => {
                            let Some(job) = active.get_mut(&id) else {
                                continue;
                            };
                            job.stage = stage;
                            let source = &progress.sources[indexes[&job.batch.inputs[0].source_id]];
                            format!("{} · 本集 {}/{}", message, source.current, source.total)
                        }
                        PreparationEvent::Finished { id, result } => {
                            let job = active.remove(&id).unwrap();
                            let results = result.unwrap_or_else(|error| {
                                job.batch
                                    .inputs
                                    .iter()
                                    .map(|_| Err(error.clone()))
                                    .collect()
                            });
                            for (input, result) in job.batch.inputs.iter().zip(results) {
                                match result {
                                    Ok(None) => progress.skipped += 1,
                                    Ok(Some(true)) => progress.generated += 1,
                                    Ok(Some(false)) => progress.reused += 1,
                                    Err(error)
                                        if matches!(
                                            error.code.as_str(),
                                            "invalid_data" | "invalid_input"
                                        ) =>
                                    {
                                        progress.failed += 1;
                                        if progress.failures.len() < 20 {
                                            progress.failures.push(json!({"source":input.title,"text":input.text,"message":error.message}));
                                        }
                                    }
                                    Err(error) => {
                                        if error.code != "cancelled" {
                                            self.preparation_paused.store(true, Ordering::Relaxed);
                                        }
                                        return Err(error);
                                    }
                                }
                                current += 1;
                                progress.sources[indexes[&input.source_id]].current += 1;
                            }
                            let source = &progress.sources[indexes[&job.batch.inputs[0].source_id]];
                            format!(
                                "{} · 本集 {}/{}",
                                source.title, source.current, source.total
                            )
                        }
                    };
                    self.publish_preparation(
                        &context,
                        current,
                        total,
                        &mut progress,
                        &active,
                        &message,
                    );
                }
                progress.in_flight = 0;
                progress.waiting = 0;
                progress.batches_in_flight = 0;
                Ok(json!({"preparation":progress}))
            })();
            stopped.store(true, Ordering::Relaxed);
            drop(work_sender);
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::Settings, explanations::Explanation, tasks::TaskManager};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread::JoinHandle,
        time::Instant,
    };

    fn seed(store: &Store, id: &str, words: &[(&str, &str)]) {
        let connection = store.connection().unwrap();
        connection.execute("INSERT INTO sources(id,kind,title,fingerprint,duration_ms,corpus_path,audio_path,text_source,imported_at,created_at) VALUES (?,'video',?,?,1000,'fixture.json','fixture.wav','embedded_text',1,1)", params![id,format!("Synthetic Drama S01E01 {id}"),id]).unwrap();
        for (index, (text, context)) in words.iter().enumerate() {
            let example = format!("{id}-example-{index}");
            connection.execute("INSERT INTO examples(id,source_id,location_key,identity_key,text,start_ms,end_ms,created_at) VALUES (?,?,?,?,?,0,1000,1)", params![example,id,example,example,context]).unwrap();
            connection.execute("INSERT INTO occurrences(id,source_id,example_id,candidate_key,raw_text,token_start,token_end,kind) VALUES (?,?,?,?,?,0,1,'word')", params![format!("{id}-occurrence-{index}"),id,example,crate::vocabulary::normalize(text),text]).unwrap();
        }
    }
    fn value() -> Explanation {
        Explanation {
            meaning: "合成中文词义".into(),
            translation: "合成原句译文。".into(),
            notes: "合成中文用法说明。".into(),
        }
    }
    fn response(stream: &mut std::net::TcpStream, valid: bool) {
        response_with_status(stream, valid, 200);
    }
    fn response_with_status(stream: &mut std::net::TcpStream, valid: bool, status: u16) {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut data = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let size = stream.read(&mut buffer).unwrap();
            assert!(size > 0);
            data.extend_from_slice(&buffer[..size]);
            if let Some(end) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&data[..end]);
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if data.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let content = if valid {
            serde_json::to_string(&value()).unwrap()
        } else {
            "invalid model json".into()
        };
        let body = json!({"choices":[{"message":{"content":content}}]}).to_string();
        write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    }
    fn model_server(valid: Vec<bool>) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            for flag in valid {
                let (mut stream, _) = listener.accept().unwrap();
                response(&mut stream, flag);
            }
        });
        (url, handle)
    }
    fn application(store: Arc<Store>, url: String) -> Arc<Application> {
        Arc::new(Application::new(
            Arc::clone(&store),
            TaskManager::new(store, Arc::new(|_| {})),
            Settings {
                model_url: url,
                offline_mode: true,
                ..Default::default()
            },
        ))
    }
    fn wait_until(mut ready: impl FnMut() -> bool) {
        let started = Instant::now();
        while !ready() {
            assert!(started.elapsed() < Duration::from_secs(8));
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn finish(app: &Application, task: &TaskSnapshot) -> TaskSnapshot {
        wait_until(|| app.tasks.get(&task.id).unwrap().terminal());
        app.tasks.get(&task.id).unwrap()
    }

    #[test]
    fn same_sentence_targets_share_one_request_and_keep_cached_explanations() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let sentence =
            "Initially, the reluctant apprentice carefully adjusted the unfamiliar apparatus.";
        let words = [
            "initially",
            "the",
            "reluctant",
            "apprentice",
            "carefully",
            "adjusted",
            "unfamiliar",
            "apparatus",
        ];
        seed(
            &store,
            "episode",
            &words
                .iter()
                .map(|word| (*word, sentence))
                .collect::<Vec<_>>(),
        );
        let cached = value();
        store
            .save_explanation("initially", sentence, &cached, "cached", "cached")
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let real_model = std::env::var("SVL_VALIDATE_BATCH_MODEL_URL").ok();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut data = Vec::new();
            let (body_start, body_length) = loop {
                let mut buffer = [0; 4096];
                let size = stream.read(&mut buffer).unwrap();
                assert!(size > 0);
                data.extend_from_slice(&buffer[..size]);
                if let Some(end) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let length = String::from_utf8_lossy(&data[..end])
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if data.len() >= end + 4 + length {
                        break (end + 4, length);
                    }
                }
            };
            let request: serde_json::Value =
                serde_json::from_slice(&data[body_start..body_start + body_length]).unwrap();
            let input: serde_json::Value =
                serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
            assert_eq!(input["context"], sentence);
            assert_eq!(input["targets"].as_array().unwrap().len(), 7);
            let body = if let Some(model_url) = real_model {
                reqwest::blocking::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(65))
                    .build()
                    .unwrap()
                    .post(format!(
                        "{}/v1/chat/completions",
                        model_url.trim_end_matches('/')
                    ))
                    .json(&request)
                    .send()
                    .unwrap()
                    .error_for_status()
                    .unwrap()
                    .bytes()
                    .unwrap()
                    .to_vec()
            } else {
                let items = input["targets"].as_array().unwrap().iter().map(|target| json!({"id":target["id"],"meaning":format!("合成释义 {}",target["text"].as_str().unwrap()),"notes":"合成中文用法。"})).collect::<Vec<_>>();
                let content = json!({"translation":"合成原句译文。","items":items}).to_string();
                json!({"choices":[{"message":{"content":content}}]})
                    .to_string()
                    .into_bytes()
            };
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
            stream.write_all(&body).unwrap();
        });
        let app = application(Arc::clone(&store), url);
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        let started = Instant::now();
        let ended = loop {
            let snapshot = app.tasks.get(&task.id).unwrap();
            if snapshot.terminal() {
                break snapshot;
            }
            if started.elapsed() > Duration::from_secs(90) {
                app.pause_preparation().unwrap();
                panic!("normal batch request did not complete");
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(ended.state, "succeeded");
        assert_eq!(ended.current, 7);
        assert_eq!(ended.total, 7);
        assert_eq!(ended.result.unwrap()["preparation"]["generated"], 7);
        assert_eq!(
            store.explanation("initially", sentence).unwrap().unwrap(),
            cached
        );
        let translations = words[1..]
            .iter()
            .map(|word| {
                store
                    .explanation(word, sentence)
                    .unwrap()
                    .unwrap()
                    .translation
            })
            .collect::<HashSet<_>>();
        assert_eq!(translations.len(), 1);
        assert!(store.explanation_work(None).unwrap().is_empty());
        assert_eq!(app.tasks.history(0, 10).unwrap().total, 1);
        server.join().unwrap();
        println!(
            "Shared-context normal path: one HTTP request, seven saved explanations, cached entry preserved."
        );
    }

    #[test]
    fn saved_concurrency_runs_two_preparation_requests_in_parallel() {
        use std::sync::{Barrier, atomic::AtomicUsize};
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        seed(
            &store,
            "episode",
            &[
                ("reluctant", "She is reluctant."),
                ("patient", "Stay patient."),
            ],
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let together = Arc::new(Barrier::new(2));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&peak);
        let server = std::thread::spawn(move || {
            let mut handlers = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let together = Arc::clone(&together);
                let active = Arc::clone(&active);
                let peak = Arc::clone(&peak);
                handlers.push(std::thread::spawn(move || {
                    let count = active.fetch_add(1, Ordering::Relaxed) + 1;
                    peak.fetch_max(count, Ordering::Relaxed);
                    together.wait();
                    response(&mut stream, true);
                    active.fetch_sub(1, Ordering::Relaxed);
                }));
            }
            for handler in handlers {
                handler.join().unwrap();
            }
        });
        let app = application(Arc::clone(&store), url.clone());
        assert_eq!(app.settings().unwrap().model_concurrency, 1);
        let mut settings = app.settings().unwrap();
        settings.model_concurrency = 2;
        assert_eq!(app.save_settings(settings).unwrap().model_concurrency, 2);
        drop(app);
        drop(store);
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let app = application(Arc::clone(&store), url);
        assert_eq!(app.settings().unwrap().model_concurrency, 2);
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        let task = finish(&app, &task);
        assert_eq!(task.state, "succeeded");
        assert_eq!(task.current, 2);
        assert_eq!(task.total, 2);
        assert_eq!(observed.load(Ordering::Relaxed), 2);
        let result = task.result.unwrap();
        assert_eq!(result["preparation"]["generated"], 2);
        assert_eq!(result["preparation"]["concurrency"], 2);
        assert_eq!(app.tasks.history(0, 10).unwrap().total, 1);
        assert!(store.explanation_work(None).unwrap().is_empty());
        server.join().unwrap();
    }

    #[test]
    fn batch_skips_saved_and_known_material_and_keeps_partial_invalid_results_in_one_record() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        seed(
            &store,
            "episode",
            &[
                ("reluctant", "She is reluctant."),
                ("reluctant", "Remain reluctant."),
                ("patient", "Stay patient."),
                ("ready", "I am ready."),
            ],
        );
        store.set_known_target("word", "ready", true).unwrap();
        store
            .save_explanation(
                "reluctant",
                "She is reluctant.",
                &value(),
                "legacy",
                "old model",
            )
            .unwrap();
        assert_eq!(store.explanation_work(None).unwrap().len(), 2);
        let (url, server) = model_server(vec![true, false, false]);
        let app = application(Arc::clone(&store), url);
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        let final_task = finish(&app, &task);
        assert_eq!(final_task.state, "succeeded");
        assert_eq!(final_task.current, 2);
        let result = final_task.result.unwrap();
        assert_eq!(result["preparation"]["generated"], 1);
        assert_eq!(result["preparation"]["failed"], 1);
        assert_eq!(app.tasks.history(0, 10).unwrap().total, 1);
        assert_eq!(store.explanation_work(None).unwrap().len(), 1);
        assert!(store.list_entries("", 0, 20).unwrap().is_empty());
        assert_eq!(
            store
                .connection()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE kind='explanation'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        server.join().unwrap();
    }

    #[test]
    fn cancel_and_reopen_keep_completed_material_and_explicit_continue_only_fills_missing_items() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        seed(
            &store,
            "episode",
            &[
                ("reluctant", "She is reluctant."),
                ("patient", "Stay patient."),
                ("ready", "I am ready."),
            ],
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (reached, boundary) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            response(&mut first, true);
            let (mut second, _) = listener.accept().unwrap();
            reached.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
            response(&mut second, true);
        });
        let app = application(Arc::clone(&store), url);
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        boundary.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            store
                .explanation("reluctant", "She is reluctant.")
                .unwrap()
                .is_some()
        );
        app.pause_preparation().unwrap();
        assert_eq!(app.tasks.get(&task.id).unwrap().state, "cancel_requested");
        app.tasks.cancel(&task.id).unwrap();
        release.send(()).unwrap();
        assert_eq!(finish(&app, &task).state, "cancelled");
        assert_eq!(app.preparation_status().unwrap().state, "paused");
        server.join().unwrap();
        drop(app);
        drop(store);
        let store = Arc::new(Store::open(directory.path()).unwrap());
        assert_eq!(store.explanation_work(None).unwrap().len(), 2);
        assert!(!store.preparation_enabled().unwrap());
        let (url, server) = model_server(vec![true, true]);
        let app = application(Arc::clone(&store), url);
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        assert_eq!(finish(&app, &task).current, 2);
        assert!(store.explanation_work(None).unwrap().is_empty());
        assert!(app.prepare_explanations(None, true).unwrap().task.is_none());
        server.join().unwrap();
    }

    #[test]
    fn scheduler_prepares_existing_and_newly_imported_sources_without_opening_preview() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        seed(&store, "first", &[("reluctant", "She is reluctant.")]);
        let (url, server) = model_server(vec![true, true]);
        let app = application(Arc::clone(&store), url);
        app.start_preparation_scheduler();
        wait_until(|| {
            store
                .explanation("reluctant", "She is reluctant.")
                .unwrap()
                .is_some()
        });
        seed(&store, "next", &[("fresh", "A fresh start.")]);
        app.preparation_requested.store(true, Ordering::Relaxed);
        wait_until(|| {
            store
                .explanation("fresh", "A fresh start.")
                .unwrap()
                .is_some()
        });
        wait_until(|| app.tasks.active().unwrap().is_empty());
        app.stop_preparation_scheduler();
        assert!(store.explanation_work(None).unwrap().is_empty());
        assert!(
            app.tasks
                .history(0, 10)
                .unwrap()
                .items
                .iter()
                .all(|task| task.kind == "explanation_batch")
        );
        server.join().unwrap();
    }

    #[test]
    fn connection_failure_waits_in_one_task_and_manual_pause_cancels_retries() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        seed(&store, "episode", &[("reluctant", "She is reluctant.")]);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let app = application(Arc::clone(&store), url);
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        wait_until(|| app.tasks.get(&task.id).unwrap().stage == "retry_wait");
        let waiting = app.tasks.get(&task.id).unwrap();
        assert_eq!(waiting.state, "running");
        assert!(waiting.error.is_none());
        assert!(waiting.message.contains("2 秒后自动重试"));
        assert_eq!(waiting.current, 0);
        assert_eq!(waiting.total, 1);
        assert_eq!(waiting.result.unwrap()["preparation"]["failed"], 0);
        assert_eq!(app.preparation_status().unwrap().state, "running");
        assert_eq!(store.explanation_work(None).unwrap().len(), 1);
        assert_eq!(app.tasks.history(0, 10).unwrap().total, 0);
        let cancelled_at = Instant::now();
        app.pause_preparation().unwrap();
        assert_eq!(finish(&app, &task).state, "cancelled");
        assert!(cancelled_at.elapsed() < Duration::from_secs(1));
        assert_eq!(app.preparation_status().unwrap().state, "paused");
        assert!(
            app.prepare_explanations(None, false)
                .unwrap()
                .task
                .is_none()
        );
        assert_eq!(app.tasks.history(0, 10).unwrap().total, 1);
    }

    #[test]
    fn busy_model_recovers_automatically_without_another_task_or_duplicate_progress() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        seed(
            &store,
            "episode",
            &[
                ("reluctant", "She is reluctant."),
                ("patient", "Stay patient."),
            ],
        );
        store
            .save_explanation("patient", "Stay patient.", &value(), "old", "old")
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for status in [503, 200] {
                let (mut stream, _) = listener.accept().unwrap();
                response_with_status(&mut stream, true, status);
            }
        });
        let app = application(Arc::clone(&store), url);
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        wait_until(|| app.tasks.get(&task.id).unwrap().stage == "retry_wait");
        let final_task = finish(&app, &task);
        assert_eq!(final_task.state, "succeeded");
        assert_eq!(final_task.current, 1);
        assert_eq!(final_task.total, 1);
        assert_eq!(final_task.result.unwrap()["preparation"]["generated"], 1);
        assert_eq!(app.tasks.history(0, 10).unwrap().total, 1);
        assert!(store.explanation_work(None).unwrap().is_empty());
        server.join().unwrap();
    }

    #[test]
    fn manual_retry_wakes_the_existing_task_after_connection_is_restored() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        seed(&store, "episode", &[("reluctant", "She is reluctant.")]);
        let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reserved.local_addr().unwrap();
        drop(reserved);
        let app = application(Arc::clone(&store), format!("http://{address}"));
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        wait_until(|| app.tasks.get(&task.id).unwrap().stage == "retry_wait");
        let listener = TcpListener::bind(address).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            response(&mut stream, true);
        });
        let started = Instant::now();
        let retry = app.prepare_explanations(None, true).unwrap().task.unwrap();
        assert_eq!(retry.id, task.id);
        assert_eq!(finish(&app, &task).state, "succeeded");
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(app.tasks.history(0, 10).unwrap().total, 1);
        server.join().unwrap();
    }

    #[test]
    fn invalid_output_is_retried_once_and_only_the_valid_value_is_saved() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        seed(&store, "episode", &[("reluctant", "She is reluctant.")]);
        let (url, server) = model_server(vec![false, true]);
        let app = application(Arc::clone(&store), url);
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        let final_task = finish(&app, &task);
        assert_eq!(final_task.state, "succeeded");
        assert_eq!(final_task.current, 1);
        let result = final_task.result.unwrap();
        assert_eq!(result["preparation"]["generated"], 1);
        assert_eq!(result["preparation"]["failed"], 0);
        assert_eq!(app.tasks.history(0, 10).unwrap().total, 1);
        assert!(store.explanation_work(None).unwrap().is_empty());
        server.join().unwrap();
    }

    #[test]
    fn permanent_http_error_ends_the_batch_with_a_manual_retry_path() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        seed(&store, "episode", &[("reluctant", "She is reluctant.")]);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            response_with_status(&mut stream, true, 400);
        });
        let app = application(Arc::clone(&store), url);
        let task = app.prepare_explanations(None, true).unwrap().task.unwrap();
        let final_task = finish(&app, &task);
        assert_eq!(final_task.state, "failed");
        let error = final_task.error.unwrap();
        assert_eq!(error.code, "provider_unavailable");
        assert!(!error.retryable);
        assert!(error.message.contains("HTTP 400"));
        assert_eq!(app.preparation_status().unwrap().state, "failed");
        assert_eq!(app.tasks.history(0, 10).unwrap().total, 1);
        assert_eq!(store.explanation_work(None).unwrap().len(), 1);
        server.join().unwrap();
        let (url, server) = model_server(vec![true]);
        let mut settings = app.settings().unwrap();
        settings.model_url = url;
        app.save_settings(settings).unwrap();
        let retry = app.prepare_explanations(None, true).unwrap().task.unwrap();
        assert_eq!(finish(&app, &retry).state, "succeeded");
        assert!(store.explanation_work(None).unwrap().is_empty());
        server.join().unwrap();
    }

    #[test]
    fn connection_retry_delay_grows_and_remains_bounded() {
        assert_eq!(retry_delay(1), Duration::from_secs(2));
        assert_eq!(retry_delay(2), Duration::from_secs(4));
        assert_eq!(retry_delay(u32::MAX), Duration::from_secs(60));
    }
}
