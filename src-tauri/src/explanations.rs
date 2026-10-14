use crate::{error::AppError, store::Store};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedExplanation {
    #[serde(flatten)]
    pub value: Explanation,
    pub meaning_scope: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    pub meaning: String,
    pub translation: String,
    pub notes: String,
}

pub fn validate_input(text: &str, context: &str) -> Result<(), AppError> {
    if text.trim().is_empty() || text.chars().count() > 4000 || context.chars().count() > 20000 {
        return Err(AppError::new(
            "invalid_input",
            "请提供有效的词语和有限语境。",
        ));
    }
    Ok(())
}

impl Explanation {
    pub fn validate(&self, context: &str) -> Result<(), AppError> {
        let chinese = |value: &str| {
            value
                .chars()
                .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
        };
        if !chinese(&self.meaning)
            || (!context.trim().is_empty() && !chinese(&self.translation))
            || [&self.meaning, &self.translation, &self.notes]
                .iter()
                .any(|value| value.chars().count() > 16000)
        {
            return Err(AppError::new(
                "invalid_data",
                "模型解释缺少有效中文词义或译文，请重试。",
            ));
        }
        Ok(())
    }
}

impl Store {
    pub fn explanation(&self, text: &str, context: &str) -> Result<Option<Explanation>, AppError> {
        validate_input(text, context)?;
        Ok(self
            .connection()?
            .query_row(
                "SELECT meaning,translation,notes FROM explanations WHERE target=? AND context=?",
                params![
                    if context.trim().is_empty() {
                        crate::vocabulary::normalize(text)
                    } else {
                        text.trim().to_owned()
                    },
                    context
                ],
                |row| {
                    Ok(Explanation {
                        meaning: row.get(0)?,
                        translation: row.get(1)?,
                        notes: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn prepared_explanation(
        &self,
        text: &str,
        context: &str,
    ) -> Result<Option<PreparedExplanation>, AppError> {
        if let Some(value) = self.explanation(text, context)? {
            return Ok(Some(PreparedExplanation {
                value,
                meaning_scope: if context.trim().is_empty() {
                    "general"
                } else {
                    "context"
                },
            }));
        }
        let (mut value, meaning_scope) = if let Some(value) = self.explanation(text, "")? {
            (value, "general")
        } else if let Some(value) = self.dictionary.explanation(text)? {
            (value, "dictionary")
        } else {
            return Ok(self.sentence_translation(context)?.map(|translation| {
                PreparedExplanation {
                    value: Explanation {
                        meaning: String::new(),
                        translation,
                        notes: String::new(),
                    },
                    meaning_scope: "translation",
                }
            }));
        };
        value.translation = self.sentence_translation(context)?.unwrap_or_default();
        Ok(Some(PreparedExplanation {
            value,
            meaning_scope,
        }))
    }

    // A cached sentence alone must not masquerade as a word definition.
    pub fn available_word_explanation(
        &self,
        text: &str,
        context: &str,
    ) -> Result<Option<PreparedExplanation>, AppError> {
        Ok(self.prepared_explanation(text, context)?.filter(|value| !value.value.meaning.is_empty()))
    }

    pub fn sentence_translation(&self, sentence: &str) -> Result<Option<String>, AppError> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT translation FROM sentence_translations WHERE sentence=?",
                [sentence.trim()],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn save_sentence_translation(
        &self,
        sentence: &str,
        translation: &str,
        model_url: &str,
        model_name: &str,
    ) -> Result<(), AppError> {
        validate_input(sentence, "")?;
        if !translation
            .chars()
            .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
            || translation.chars().count() > 16000
        {
            return Err(AppError::new("invalid_data", "未获得有效中文句子译文。"));
        }
        self.connection()?.execute("INSERT INTO sentence_translations(sentence,translation,model_url,model_name,created_at) VALUES (?,?,?,?,?) ON CONFLICT(sentence) DO NOTHING", params![sentence.trim(),translation,model_url,model_name,Utc::now().timestamp_millis()])?;
        Ok(())
    }

    // Keep the first durable result when requests or legacy imports race.
    // Generated annotations never modify confirmed vocabulary or learning records.
    pub fn save_explanation(
        &self,
        text: &str,
        context: &str,
        value: &Explanation,
        model_url: &str,
        model_name: &str,
    ) -> Result<Explanation, AppError> {
        validate_input(text, context)?;
        value.validate(context)?;
        let text = if context.trim().is_empty() {
            crate::vocabulary::normalize(text)
        } else {
            text.trim().to_owned()
        };
        if model_url.len() > 4000 || model_name.len() > 1000 {
            return Err(AppError::new("invalid_input", "模型来源标识过长。"));
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO explanations(target,context,meaning,translation,notes,model_url,model_name,created_at)
             VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(target,context) DO NOTHING",
            params![text.trim(), context, value.meaning, value.translation, value.notes, model_url, model_name, Utc::now().timestamp_millis()],
        )?;
        let saved = transaction.query_row(
            "SELECT meaning,translation,notes FROM explanations WHERE target=? AND context=?",
            params![text.trim(), context],
            |row| {
                Ok(Explanation {
                    meaning: row.get(0)?,
                    translation: row.get(1)?,
                    notes: row.get(2)?,
                })
            },
        )?;
        if !context.trim().is_empty() {
            transaction.execute("INSERT INTO sentence_translations(sentence,translation,model_url,model_name,created_at) VALUES (?,?,?,?,?) ON CONFLICT(sentence) DO NOTHING", params![context.trim(),value.translation,model_url,model_name,Utc::now().timestamp_millis()])?;
        }
        transaction.commit()?;
        Ok(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{Application, Settings},
        tasks::{TaskManager, TaskSnapshot},
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
        time::{Duration, Instant},
    };

    fn finish(app: &Application, task: TaskSnapshot) -> TaskSnapshot {
        let started = Instant::now();
        loop {
            let current = app.tasks.get(&task.id).unwrap();
            if current.terminal() {
                return current;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn model_result_is_durable_before_success_and_reused_after_restart_with_another_model() {
        let directory = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let model_url = format!("http://{}", listener.local_addr().unwrap());
        let expected = value("不同的");
        let response = serde_json::json!({"choices":[{"message":{"content":serde_json::to_string(&expected).unwrap()}}]}).to_string();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..end]);
                    let length: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
        });
        {
            let store = Arc::new(Store::open(directory.path()).unwrap());
            let app = Application::new(
                Arc::clone(&store),
                TaskManager::new(Arc::clone(&store), Arc::new(|_| {})),
                Settings {
                    model_url,
                    ..Settings::default()
                },
            );
            let task = finish(
                &app,
                app.explain_start(
                    "different".into(),
                    "Take a different route.".into(),
                    uuid::Uuid::new_v4().to_string(),
                )
                .unwrap(),
            );
            assert_eq!(task.state, "succeeded", "{:?}", task.error);
            assert_eq!(
                store
                    .explanation("different", "Take a different route.")
                    .unwrap(),
                Some(expected.clone())
            );
            assert_eq!(
                serde_json::from_value::<Explanation>(task.result.unwrap()).unwrap(),
                expected
            );
        }
        server.join().unwrap();
        let unused_model = TcpListener::bind("127.0.0.1:0").unwrap();
        unused_model.set_nonblocking(true).unwrap();
        let store = Arc::new(Store::open(directory.path()).unwrap());
        let app = Application::new(
            Arc::clone(&store),
            TaskManager::new(Arc::clone(&store), Arc::new(|_| {})),
            Settings {
                model_url: format!("http://{}", unused_model.local_addr().unwrap()),
                model_name: "Different model".into(),
                offline_mode: true,
                ..Settings::default()
            },
        );
        let task = finish(
            &app,
            app.explain_start(
                "different".into(),
                "Take a different route.".into(),
                uuid::Uuid::new_v4().to_string(),
            )
            .unwrap(),
        );
        assert_eq!(task.state, "succeeded");
        assert_eq!(
            serde_json::from_value::<Explanation>(task.result.unwrap()).unwrap(),
            expected
        );
        assert_eq!(
            unused_model.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    fn value(meaning: &str) -> Explanation {
        Explanation {
            meaning: meaning.into(),
            translation: "走一条不同的路线。".into(),
            notes: "形容词，修饰路线。".into(),
        }
    }

    #[test]
    fn annotations_survive_reopen_without_eviction_or_vocabulary_side_effects() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let original = value("不同的");
        store
            .save_explanation(
                " different ",
                "Take a different route.",
                &original,
                "http://127.0.0.1:8096",
                "Qwen",
            )
            .unwrap();
        assert_eq!(
            store
                .save_explanation(
                    "different",
                    "Take a different route.",
                    &value("不同的（重复导入）"),
                    "legacy",
                    "old"
                )
                .unwrap(),
            original
        );
        store
            .save_explanation(
                "different",
                "We're very different.",
                &value("有差异的"),
                "local",
                "Qwen",
            )
            .unwrap();
        for index in 0..510 {
            store
                .save_explanation(
                    &format!("word{index}"),
                    "",
                    &value("合成词义"),
                    "local",
                    "Qwen",
                )
                .unwrap();
        }
        drop(store);
        let reopened = Store::open(directory.path()).unwrap();
        assert_eq!(
            reopened
                .explanation("different", "Take a different route.")
                .unwrap(),
            Some(original)
        );
        assert_eq!(
            reopened
                .explanation("different", "We're very different.")
                .unwrap()
                .unwrap()
                .meaning,
            "有差异的"
        );
        assert!(
            reopened
                .explanation("different", "Corrected context.")
                .unwrap()
                .is_none()
        );
        let connection = reopened.connection().unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM explanations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 512);
        for table in ["entries", "collection_actions", "learning_units"] {
            assert_eq!(
                connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }

    #[test]
    fn invalid_or_non_chinese_results_are_not_persisted() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        for invalid in [
            value("different"),
            Explanation {
                translation: "English only".into(),
                ..value("不同的")
            },
        ] {
            assert_eq!(
                store
                    .save_explanation(
                        "different",
                        "Take a different route.",
                        &invalid,
                        "local",
                        "Qwen"
                    )
                    .unwrap_err()
                    .code,
                "invalid_data"
            );
        }
        assert!(
            store
                .explanation("different", "Take a different route.")
                .unwrap()
                .is_none()
        );
    }
}
