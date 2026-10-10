use crate::{
    corpus::CandidateExample,
    error::AppError,
    store::Store,
    vocabulary::{self, ExampleInput, SourceInput},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticeSentence {
    pub text: String,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticeChapter {
    pub title: String,
    pub sentences: Vec<PracticeSentence>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticeDocument {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub chapters: Vec<PracticeChapter>,
    pub audio_file: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticeProgress {
    pub chapter_index: usize,
    pub sentence_index: usize,
    pub copy: String,
    pub speed: f64,
    pub audio_mode: String,
    #[serde(default)]
    pub voice_id: String,
}

impl Default for PracticeProgress {
    fn default() -> Self {
        Self {
            chapter_index: 0,
            sentence_index: 0,
            copy: String::new(),
            speed: 1.0,
            audio_mode: "voice".into(),
            voice_id: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticeState {
    pub document: PracticeDocument,
    #[serde(flatten)]
    pub progress: PracticeProgress,
}

impl Store {
    pub fn practice_get(&self) -> Result<Option<PracticeState>, AppError> {
        let value: Option<String> = self
            .connection()?
            .query_row(
                "SELECT value_json FROM settings WHERE key='practice'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|text| serde_json::from_str(&text).map_err(AppError::from))
            .transpose()
    }

    fn practice_save(&self, state: &PracticeState) -> Result<(), AppError> {
        self.connection()?.execute("INSERT INTO settings(key,value_json) VALUES ('practice',?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json", [serde_json::to_string(state)?])?;
        Ok(())
    }

    pub fn practice_import(
        &self,
        mut document: PracticeDocument,
    ) -> Result<PracticeState, AppError> {
        if document.title.trim().is_empty() || document.chapters.is_empty() || document.chapters.iter().any(|chapter| chapter.sentences.is_empty() || chapter.sentences.iter().any(|sentence| sentence.text.trim().is_empty() || !matches!((sentence.start_ms, sentence.end_ms), (None, None)) && !matches!((sentence.start_ms, sentence.end_ms), (Some(start), Some(end)) if start >= 0 && end > start))) {
            return Err(AppError::new("invalid_input", "练习文字或时间位置无效。"));
        }
        if let Some(file) = &document.audio_file {
            let expected = std::path::Path::new(file);
            if !file.starts_with("media/practice/")
                || expected
                    .components()
                    .any(|component| !matches!(component, std::path::Component::Normal(_)))
                || !self.root().join(file).is_file()
            {
                return Err(AppError::new(
                    "resource_missing",
                    "课程原声尚未保存，请重新导入课程包。",
                ));
            }
        }
        let fingerprint = vocabulary::digest(&serde_json::to_vec(&(
            &document.title,
            &document.kind,
            &document.chapters,
        ))?);
        let duration_ms = document
            .chapters
            .iter()
            .flat_map(|chapter| &chapter.sentences)
            .filter_map(|sentence| sentence.end_ms)
            .max();
        document.id = self.add_source(&SourceInput {
            kind: "reading".into(),
            title: document.title.clone(),
            fingerprint,
            path_hint: None,
            duration_ms,
        })?;
        if let Some(file) = &document.audio_file {
            self.connection()?.execute(
                "UPDATE sources SET audio_path=? WHERE id=?",
                params![self.root().join(file).to_string_lossy(), document.id],
            )?;
        }
        let previous = self.practice_get()?;
        let progress = previous
            .as_ref()
            .filter(|state| state.document.id == document.id)
            .map(|state| state.progress.clone())
            .unwrap_or(PracticeProgress {
                audio_mode: if document.audio_file.is_some() {
                    "original"
                } else {
                    "voice"
                }
                .into(),
                ..Default::default()
            });
        let state = PracticeState { document, progress };
        self.practice_save(&state)?;
        if let Some(old_file) = previous.and_then(|state| state.document.audio_file)
            && Some(&old_file) != state.document.audio_file.as_ref()
        {
            let _ = std::fs::remove_file(self.root().join(old_file));
        }
        Ok(state)
    }

    pub fn practice_progress_save(
        &self,
        document_id: &str,
        progress: PracticeProgress,
    ) -> Result<(), AppError> {
        let mut state = self
            .practice_get()?
            .ok_or_else(|| AppError::new("not_found", "请先导入练习文字。"))?;
        if state.document.id != document_id {
            return Ok(());
        }
        if !state
            .document
            .chapters
            .get(progress.chapter_index)
            .is_some_and(|chapter| progress.sentence_index < chapter.sentences.len())
            || ![0.65, 0.85, 1.0].contains(&progress.speed)
            || !["original", "voice"].contains(&progress.audio_mode.as_str())
        {
            return Err(AppError::new("invalid_input", "练习位置或播放设置无效。"));
        }
        state.progress = progress;
        self.practice_save(&state)
    }

    pub fn practice_audio_save(&self, bytes: &[u8]) -> Result<String, AppError> {
        if bytes.is_empty() {
            return Err(AppError::new("invalid_input", "课程原声为空。"));
        }
        let directory = self.root().join("media/practice");
        std::fs::create_dir_all(&directory)?;
        let relative = format!("media/practice/{}.m4a", vocabulary::digest(bytes));
        let path = self.root().join(&relative);
        if !path.is_file() {
            std::fs::write(path, bytes)?;
        }
        Ok(relative)
    }

    pub fn practice_example(
        &self,
        document_id: &str,
        chapter: usize,
        sentence: usize,
    ) -> Result<CandidateExample, AppError> {
        let state = self
            .practice_get()?
            .filter(|state| state.document.id == document_id)
            .ok_or_else(|| AppError::new("not_found", "当前阅读已更换，请重新选择。"))?;
        let sentence_text = state
            .document
            .chapters
            .get(chapter)
            .and_then(|value| value.sentences.get(sentence))
            .ok_or_else(|| AppError::new("not_found", "练习句子不存在。"))?;
        let input = ExampleInput {
            text: sentence_text.text.clone(),
            source_id: Some(document_id.into()),
            location_key: format!("practice:{chapter}:{sentence}"),
            start_ms: sentence_text.start_ms,
            end_ms: sentence_text.end_ms,
            ..Default::default()
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let id = vocabulary::insert_example(
            &transaction,
            &input,
            chrono::Utc::now().timestamp_millis(),
        )?;
        transaction.commit()?;
        drop(connection);
        self.source_example(document_id, &id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_and_original_audio_resume_with_shared_collection() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let file = store.practice_audio_save(b"synthetic fixture").unwrap();
        let state = store
            .practice_import(PracticeDocument {
                id: String::new(),
                title: "Practice fixture".into(),
                kind: "原声课程".into(),
                chapters: vec![PracticeChapter {
                    title: "Chapter".into(),
                    sentences: vec![PracticeSentence {
                        text: "Every small step helps.".into(),
                        start_ms: Some(0),
                        end_ms: Some(1000),
                    }],
                }],
                audio_file: Some(file.clone()),
            })
            .unwrap();
        store
            .practice_progress_save(
                &state.document.id,
                PracticeProgress {
                    copy: "Every small".into(),
                    speed: 0.85,
                    audio_mode: "original".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let example = store.practice_example(&state.document.id, 0, 0).unwrap();
        let result = store
            .collect(&vocabulary::CollectionInput {
                operation_id: uuid::Uuid::new_v4().to_string(),
                kind: "word".into(),
                text: "step".into(),
                meaning: "一步".into(),
                examples: vec![ExampleInput {
                    text: example.text,
                    source_id: Some(state.document.id.clone()),
                    location_key: "practice:0:0".into(),
                    start_ms: Some(0),
                    end_ms: Some(1000),
                    ..Default::default()
                }],
                target_entry_id: None,
                expected_revision: None,
            })
            .unwrap();
        drop(store);
        let reopened = Store::open(directory.path()).unwrap();
        let resumed = reopened.practice_get().unwrap().unwrap();
        assert_eq!(resumed.progress.copy, "Every small");
        assert_eq!(resumed.progress.speed, 0.85);
        assert!(reopened.root().join(file).is_file());
        let entry = reopened.get_entry(&result.entry_id).unwrap();
        assert_eq!(
            entry.examples[0].source_title.as_deref(),
            Some("Practice fixture")
        );
        assert_eq!(entry.examples[0].start_ms, Some(0));
    }
}
