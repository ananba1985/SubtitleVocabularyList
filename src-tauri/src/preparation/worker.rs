//! Provider execution and retry policy, independent of queue coordination.
use super::*;

impl Application {
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
            if input.kind != "sentence" && self.store.is_known_target(&input.kind, &input.text)? {
                return Ok(None);
            }
            report("translations", format!("{} · {}", input.title, input.text));
            let result = if input.kind == "sentence" {
                self.translate_many(&[input.text.clone()], context)
                    .and_then(|mut values| {
                        values
                            .remove(&input.text)
                            .ok_or_else(|| AppError::new("invalid_data", "句子译文缺失。"))?
                    })
            } else {
                self.explain_value(&input.text, &input.context, context)
                    .map(|(_, created)| created)
            };
            let error = match result {
                Ok(created) => return Ok(Some(created)),
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

    pub(super) fn explain_prepared_batch(
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
                } else if input.kind != "sentence"
                    && self.store.is_known_target(&input.kind, &input.text)?
                {
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
                format!(
                    "{} · 准备 {} 个{}",
                    first.title,
                    texts.len(),
                    if first.kind == "sentence" {
                        "句子译文"
                    } else {
                        "通用词义"
                    }
                ),
            );
            let mut retry_error = None;
            let result = if first.kind == "sentence" {
                self.translate_many(&texts, context)
            } else {
                self.explain_many(&texts, &first.context, context)
            };
            match result {
                Ok(mut values) => {
                    for (index, input) in batch.inputs.iter().enumerate() {
                        if outcomes[index].is_some() {
                            continue;
                        }
                        let result = values.remove(input.text.trim()).unwrap_or_else(|| {
                            let cached = if input.kind == "sentence" {
                                self.store
                                    .sentence_translation(&input.text)
                                    .map(|v| v.is_some())
                            } else {
                                self.store
                                    .explanation(&input.text, &input.context)
                                    .map(|v| v.is_some())
                            };
                            cached.and_then(|available| {
                                if available {
                                    Ok(false)
                                } else {
                                    Err(AppError::new("invalid_data", "批量结果缺少当前词语解释。"))
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
                        "{} · {} 秒后自动重试缺失资料（第 {} 次）。{}",
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
                        "{} · 1 秒后补试未完成的资料。{}",
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
}
