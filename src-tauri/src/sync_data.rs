use crate::{error::AppError, store::Store};
use rusqlite::{Connection, types::ValueRef};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncBundle {
    pub schema_version: u32,
    pub deleted: bool,
    pub entry: Value,
    pub tables: BTreeMap<String, Vec<Value>>,
}
pub fn schema() -> Value {
    serde_json::from_str(include_str!("../../protocol/svl-sync-1.json"))
        .expect("checked-in sync schema")
}

fn query_rows(
    connection: &Connection,
    table: &str,
    columns: &Map<String, Value>,
    filter: &str,
    entry_id: &str,
) -> Result<Vec<Value>, AppError> {
    let column_names = columns.keys().cloned().collect::<Vec<_>>();
    let selection = column_names
        .iter()
        .map(|name| format!("t.{name}"))
        .collect::<Vec<_>>()
        .join(",");
    // Identifiers and filters are fixed in this module/the checked-in schema, never supplied by a caller.
    let order = if columns.contains_key("id") {
        "t.id"
    } else if columns.contains_key("operation_id") {
        "t.operation_id"
    } else if columns.contains_key("unit_id") {
        "t.unit_id"
    } else if columns.contains_key("asset_id") {
        "t.example_id,t.asset_id"
    } else if columns.contains_key("scope_key") {
        "t.example_id,t.scope_key"
    } else {
        "t.example_id"
    };
    let sql = format!("SELECT {selection} FROM {table} t WHERE {filter} ORDER BY {order}");
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query([entry_id])?;
    let mut result = vec![];
    while let Some(row) = rows.next()? {
        let mut object = Map::new();
        for (index, name) in column_names.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(value) => Value::from(value),
                ValueRef::Real(value) => Value::from(value),
                ValueRef::Text(value) => Value::String(
                    std::str::from_utf8(value)
                        .map_err(|_| AppError::new("invalid_data", "词库文本编码无效。"))?
                        .into(),
                ),
                ValueRef::Blob(_) => {
                    return Err(AppError::new(
                        "invalid_data",
                        "结构资料包不包含二进制文件。",
                    ));
                }
            };
            object.insert(name.clone(), value);
        }
        result.push(Value::Object(object));
    }
    Ok(result)
}

pub(crate) fn export_on(connection: &Connection, entry_id: &str) -> Result<SyncBundle, AppError> {
    let schema = schema();
    let mut entries = query_rows(
        connection,
        "entries",
        schema["entry"].as_object().unwrap(),
        "t.id=?1",
        entry_id,
    )?;
    if entries.len() != 1 {
        return Err(AppError::new("not_found", "待同步词条不存在。"));
    }
    const EXAMPLES: &str = "SELECT example_id FROM entry_examples WHERE entry_id=?1";
    const UNITS: &str = "SELECT id FROM learning_units WHERE entry_id=?1";
    const ATTEMPTS: &str = "SELECT id FROM review_attempts WHERE unit_id IN (SELECT id FROM learning_units WHERE entry_id=?1)";
    let filters = BTreeMap::from([
        ("meanings", "t.entry_id=?1".into()),
        (
            "sources",
            format!(
                "t.id IN (SELECT source_id FROM examples WHERE id IN ({EXAMPLES})) OR t.id IN (SELECT source_id FROM collection_actions WHERE entry_id=?1)"
            ),
        ),
        ("examples", format!("t.id IN ({EXAMPLES})")),
        ("entry_examples", "t.entry_id=?1".into()),
        (
            "occurrences",
            format!(
                "t.example_id IN ({EXAMPLES}) AND (t.entry_id=?1 OR t.candidate_key=(SELECT match_key FROM entries WHERE id=?1))"
            ),
        ),
        (
            "media_assets",
            format!(
                "t.id IN (SELECT asset_id FROM example_media WHERE example_id IN ({EXAMPLES})) AND t.kind='original'"
            ),
        ),
        (
            "example_media",
            format!(
                "t.example_id IN ({EXAMPLES}) AND t.asset_id IN (SELECT id FROM media_assets WHERE kind='original')"
            ),
        ),
        ("collection_actions", "t.entry_id=?1".into()),
        ("learning_units", "t.entry_id=?1".into()),
        ("review_attempts", format!("t.unit_id IN ({UNITS})")),
        (
            "review_corrections",
            format!("t.attempt_id IN ({ATTEMPTS})"),
        ),
        ("review_states", format!("t.unit_id IN ({UNITS})")),
    ]);
    let mut tables = BTreeMap::new();
    for (table, columns) in schema["tables"].as_object().unwrap() {
        tables.insert(
            table.clone(),
            query_rows(
                connection,
                table,
                columns.as_object().unwrap(),
                &filters[table.as_str()],
                entry_id,
            )?,
        );
    }
    // Occurrences shared with explicitly separate entries must not introduce another entry's identity.
    for row in tables.get_mut("occurrences").unwrap() {
        if row["entry_id"].as_str().is_some_and(|id| id != entry_id) {
            row["entry_id"] = Value::Null;
        }
    }
    Ok(SyncBundle {
        schema_version: 1,
        deleted: false,
        entry: entries.remove(0),
        tables,
    })
}

impl Store {
    pub fn sync_export(&self, entry_id: &str) -> Result<SyncBundle, AppError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let result = export_on(&transaction, entry_id)?;
        transaction.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        reviews::AnswerInput,
        vocabulary::{CollectionInput, ExampleInput, SourceInput},
    };
    use uuid::Uuid;
    #[test]
    fn full_bundle_keeps_learning_and_retries_without_private_runtime_fields() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let source = store
            .add_source(&SourceInput {
                kind: "selection".into(),
                title: "Synthetic reading".into(),
                fingerprint: "synthetic".into(),
                path_hint: Some("C:\\private\\film.mkv".into()),
                duration_ms: None,
            })
            .unwrap();
        let input = CollectionInput {
            operation_id: Uuid::new_v4().to_string(),
            kind: "phrase".into(),
            text: "give up".into(),
            meaning: "放弃".into(),
            examples: vec![ExampleInput {
                text: "Never give up.".into(),
                context_meaning: "不要放弃".into(),
                source_id: Some(source),
                ..Default::default()
            }],
            target_entry_id: None,
            expected_revision: None,
        };
        let result = store.collect(&input).unwrap();
        store.collect(&input).unwrap();
        let unit = store
            .review_units("all", "meaning", 0, 50)
            .unwrap()
            .remove(0);
        let question = store.review_question(&unit.id, unit.revision).unwrap();
        store
            .review_submit(&AnswerInput {
                operation_id: Uuid::new_v4().to_string(),
                question_id: question.id,
                answer: "放弃".into(),
                unable: false,
            })
            .unwrap();
        let bundle = store.sync_export(&result.entry_id).unwrap();
        assert_eq!(bundle.tables.len(), 12);
        assert_eq!(bundle.tables["collection_actions"].len(), 1);
        assert_eq!(bundle.tables["review_attempts"].len(), 1);
        assert_eq!(bundle.tables["review_states"].len(), 2);
        assert_eq!(
            bundle.tables["entry_examples"][0]["context_meaning"],
            "不要放弃"
        );
        let json = serde_json::to_string(&bundle).unwrap();
        for forbidden in [
            "path_hint",
            "relative_path",
            "C:\\private",
            "corpus_path",
            "audio_path",
            "settings",
            "credentials",
            "review_questions",
        ] {
            assert!(!json.contains(forbidden), "{forbidden}");
        }
    }
    #[test]
    fn bundle_does_not_send_other_entries_or_uncollected_episode_dialogue() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let mut ids = vec![];
        for text in ["reluctant", "private unrelated vocabulary"] {
            ids.push(
                store
                    .collect(&CollectionInput {
                        operation_id: Uuid::new_v4().to_string(),
                        kind: "word".into(),
                        text: text.into(),
                        meaning: "合成释义".into(),
                        examples: vec![ExampleInput {
                            text: format!("Synthetic {text}."),
                            ..Default::default()
                        }],
                        target_entry_id: None,
                        expected_revision: None,
                    })
                    .unwrap()
                    .entry_id,
            );
        }
        let json = serde_json::to_string(&store.sync_export(&ids[0]).unwrap()).unwrap();
        assert!(!json.contains("private unrelated"));
        assert_eq!(
            store.sync_export(&ids[0]).unwrap().tables["examples"].len(),
            1
        );
        assert_eq!(store.sync_export("missing").unwrap_err().code, "not_found");
    }
}
