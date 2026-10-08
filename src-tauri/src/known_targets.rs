use crate::{error::AppError, store::Store, vocabulary::normalize};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};

pub(crate) const PREFIX: &str = "known_target:";

pub(crate) fn target_key(kind: &str, text: &str) -> String {
    format!("{PREFIX}{kind}:{}", normalize(text))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownTarget {
    pub kind: String,
    pub text: String,
    pub match_key: String,
    pub marked_at: i64,
}

#[derive(Serialize)]
pub struct KnownTargetPage {
    pub items: Vec<KnownTarget>,
    pub total: usize,
}

impl Store {
    pub fn is_known_target(&self, kind: &str, text: &str) -> Result<bool, AppError> {
        if text.trim().is_empty() || !matches!(kind, "word" | "phrase" | "sentence") {
            return Ok(false);
        }
        Ok(self.connection()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
            [target_key(kind, text)],
            |row| row.get(0),
        )?)
    }

    pub fn set_known_target(&self, kind: &str, text: &str, known: bool) -> Result<(), AppError> {
        if !matches!(kind, "word" | "phrase" | "sentence")
            || text.trim().is_empty()
            || text.chars().count() > 4000
            || text.contains('\0')
        {
            return Err(AppError::new(
                "invalid_input",
                "请提供有效的单词、短语或句子。",
            ));
        }
        let key = target_key(kind, text);
        let connection = self.connection()?;
        if known {
            let value = KnownTarget {
                kind: kind.into(),
                text: text.trim().into(),
                match_key: normalize(text),
                marked_at: Utc::now().timestamp_millis(),
            };
            connection.execute(
                "INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",
                params![key, serde_json::to_string(&value)?],
            )?;
        } else {
            connection.execute("DELETE FROM settings WHERE key=?", [key])?;
        }
        Ok(())
    }

    pub fn known_targets(
        &self,
        search: &str,
        offset: u32,
        limit: u32,
    ) -> Result<KnownTargetPage, AppError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let pattern = format!("{PREFIX}*");
        let search = normalize(search);
        let total = transaction.query_row(
            "SELECT COUNT(*) FROM settings WHERE key GLOB ? AND instr(json_extract(value_json,'$.matchKey'),?)>0",
            params![pattern, search], |row| row.get(0),
        )?;
        let rows = {
            let mut query = transaction.prepare(
                "SELECT value_json FROM settings WHERE key GLOB ? AND instr(json_extract(value_json,'$.matchKey'),?)>0
                 ORDER BY json_extract(value_json,'$.markedAt') DESC,key LIMIT ? OFFSET ?",
            )?;
            query
                .query_map(
                    params![pattern, search, limit.clamp(1, 100), offset],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?
        };
        transaction.commit()?;
        let items = rows
            .into_iter()
            .map(|json| Ok(serde_json::from_str(&json)?))
            .collect::<Result<_, AppError>>()?;
        Ok(KnownTargetPage { items, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::{CollectionInput, ExampleInput};
    use uuid::Uuid;

    #[test]
    fn known_preferences_are_normalized_paged_durable_and_do_not_create_learning_success() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        store.set_known_target("word", "Reluctant", true).unwrap();
        store
            .set_known_target("phrase", "  Make   sure ", true)
            .unwrap();
        store
            .set_known_target("sentence", "We had breakfast together.", true)
            .unwrap();
        assert!(store.is_known_target("word", "ｒｅｌｕｃｔａｎｔ").unwrap());
        assert!(store.is_known_target("phrase", "make sure").unwrap());
        assert!(
            store
                .is_known_target("sentence", " WE HAD BREAKFAST TOGETHER. ")
                .unwrap()
        );
        assert!(!store.is_known_target("word", "make sure").unwrap());
        assert!(!store.is_known_target("word", "reluctantly").unwrap());
        assert!(!store.is_known_target("word", "").unwrap());
        for table in ["entries", "review_attempts", "review_states"] {
            let count: i64 = store
                .connection()
                .unwrap()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0);
        }
        for n in 0..41 {
            store
                .set_known_target("word", &format!("fixture{n:02}"), true)
                .unwrap();
        }
        let first = store.known_targets("", 0, 20).unwrap();
        let second = store.known_targets("", 20, 20).unwrap();
        let last = store.known_targets("", 40, 20).unwrap();
        assert_eq!(first.total, 44);
        let all = first
            .items
            .into_iter()
            .chain(second.items)
            .chain(last.items)
            .map(|value| target_key(&value.kind, &value.text))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(all.len(), 44);
        assert_eq!(store.known_targets("MAKE SURE", 0, 20).unwrap().total, 1);
        assert_eq!(store.known_targets("", 100, 20).unwrap().items.len(), 0);
        drop(store);
        let reopened = Store::open(directory.path()).unwrap();
        assert!(reopened.is_known_target("word", "reluctant").unwrap());
        reopened
            .set_known_target("word", "RELUCTANT", false)
            .unwrap();
        assert!(!reopened.is_known_target("word", "reluctant").unwrap());
        assert_eq!(reopened.known_targets("", 0, 20).unwrap().total, 43);
    }

    #[test]
    fn new_collection_restores_visibility_atomically_but_old_retry_keeps_newer_known_preference() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let mut input = CollectionInput {
            operation_id: Uuid::new_v4().to_string(),
            kind: "word".into(),
            text: "reluctant".into(),
            meaning: "不情愿的".into(),
            examples: vec![],
            target_entry_id: None,
            expected_revision: None,
        };
        let first = store.collect(&input).unwrap();
        store.set_known_target("word", "reluctant", true).unwrap();
        store.collect(&input).unwrap();
        assert!(store.is_known_target("word", "reluctant").unwrap());
        input.operation_id = Uuid::new_v4().to_string();
        input.target_entry_id = Some(first.entry_id.clone());
        input.expected_revision = Some(first.revision);
        input.examples = vec![ExampleInput {
            text: "I was reluctant.".into(),
            media_asset_ids: vec!["missing-audio".into()],
            ..Default::default()
        }];
        assert!(store.collect(&input).is_err());
        assert!(store.is_known_target("word", "reluctant").unwrap());
        input.examples[0].media_asset_ids.clear();
        let second = store.collect(&input).unwrap();
        assert!(!store.is_known_target("word", "reluctant").unwrap());
        assert_eq!(second.entry_id, first.entry_id);
        assert_eq!(
            store.get_entry(&second.entry_id).unwrap().collection_count,
            2
        );
        let attempts: i64 = store
            .connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM review_attempts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(attempts, 0);
    }
}
