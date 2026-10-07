use crate::{
    error::AppError,
    sync_data::{SyncBundle, schema},
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

fn bad(message: &str) -> AppError {
    AppError::new("invalid_data", message)
}
fn conflict(message: &str) -> AppError {
    AppError::new("sync_conflict", message)
}
pub fn validate(bundle: &SyncBundle) -> Result<(), AppError> {
    let definition = schema();
    if bundle.schema_version != 1
        || bundle.tables.keys().cloned().collect::<BTreeSet<_>>()
            != definition["tables"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect()
    {
        return Err(bad("同步资料缺少关联数据或版本不受支持。"));
    }
    fn valid(value: &Value, kind: &str) -> bool {
        if let Some(inner) = kind.strip_suffix('?') {
            return value.is_null() || valid(value, inner);
        }
        match kind {
            "uuid" => value
                .as_str()
                .is_some_and(|v| v.len() == 36 && uuid::Uuid::parse_str(v).is_ok()),
            "digest" => value.as_str().is_some_and(|v| {
                v.len() == 64
                    && v.bytes()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            }),
            "int" => value
                .as_i64()
                .is_some_and(|n| (0..=9_007_199_254_740_991).contains(&n)),
            "positive_int" => value
                .as_i64()
                .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n)),
            "flag" => matches!(value.as_i64(), Some(0 | 1)),
            "entry_kind" => value
                .as_str()
                .is_some_and(|v| matches!(v, "word" | "phrase" | "sentence")),
            "dimension" => value
                .as_str()
                .is_some_and(|v| matches!(v, "meaning" | "listening")),
            "scope_key" => value.as_str() == Some("entry") || valid(value, "uuid"),
            "outcome" => value.as_str().is_some_and(|v| {
                matches!(
                    v,
                    "correct" | "correct_with_hint" | "incorrect" | "needs_confirmation"
                )
            }),
            "effective_outcome" => value
                .as_str()
                .is_some_and(|v| matches!(v, "correct" | "correct_with_hint" | "incorrect")),
            "policy" => value.as_str() == Some(crate::review_policy::POLICY_VERSION),
            "original" => value.as_str() == Some("original"),
            "audio_format" => value
                .as_str()
                .is_some_and(|v| matches!(v, "m4a" | "wav" | "mp3" | "ogg" | "flac" | "webm")),
            "media_state" => value
                .as_str()
                .is_some_and(|v| matches!(v, "ready" | "missing")),
            "json_object" => value.as_str().is_some_and(|s| {
                s.encode_utf16().count() <= 150_000
                    && serde_json::from_str::<Value>(s).is_ok_and(|v| v.is_object())
            }),
            _ => kind
                .strip_prefix("text")
                .and_then(|s| s.parse::<usize>().ok())
                .is_some_and(|n| {
                    value
                        .as_str()
                        .is_some_and(|s| !s.contains('\0') && s.chars().count() <= n)
                }),
        }
    }
    fn row(row: &Value, columns: &Value) -> bool {
        row.as_object().is_some_and(|r| {
            r.len() == columns.as_object().unwrap().len()
                && columns
                    .as_object()
                    .unwrap()
                    .iter()
                    .all(|(k, t)| r.get(k).is_some_and(|v| valid(v, t.as_str().unwrap())))
        })
    }
    if !row(&bundle.entry, &definition["entry"])
        || bundle.entry["text"].as_str().unwrap().trim().is_empty()
    {
        return Err(bad("同步词条字段无效。"));
    }
    for (table, rows) in &bundle.tables {
        if rows.iter().any(|r| !row(r, &definition["tables"][table])) {
            return Err(bad(&format!("同步 {table} 字段无效。")));
        }
        let keys = rows.iter().map(|r| key(table, r)).collect::<BTreeSet<_>>();
        if keys.len() != rows.len() {
            return Err(bad("同步实体标识重复。"));
        }
    }
    let t = &bundle.tables;
    let entry = &bundle.entry["id"];
    let members = |table: &str, column: &str| -> BTreeSet<String> {
        t[table]
            .iter()
            .filter_map(|r| r[column].as_str().map(str::to_owned))
            .collect()
    };
    let meanings = members("meanings", "id");
    let sources = members("sources", "id");
    let examples = members("examples", "id");
    let media = members("media_assets", "id");
    let units = members("learning_units", "id");
    let attempts = members("review_attempts", "id");
    for table in [
        "meanings",
        "entry_examples",
        "collection_actions",
        "learning_units",
    ] {
        if t[table].iter().any(|r| &r["entry_id"] != entry) {
            return Err(bad("同步实体属于另一词条。"));
        }
    }
    let exists = |set: &BTreeSet<String>, v: &Value| v.as_str().is_some_and(|s| set.contains(s));
    if t["entry_examples"].iter().any(|r| {
        !exists(&examples, &r["example_id"])
            || (!r["meaning_id"].is_null() && !exists(&meanings, &r["meaning_id"]))
    }) || members("entry_examples", "example_id") != examples
    {
        return Err(bad("同步例句或语境关联缺失。"));
    }
    if t["examples"].iter().any(|r| {
        (!r["source_id"].is_null() && !exists(&sources, &r["source_id"]))
            || (r["start_ms"].is_null() != r["end_ms"].is_null())
            || (!r["start_ms"].is_null() && r["end_ms"].as_i64() <= r["start_ms"].as_i64())
    }) {
        return Err(bad("同步例句来源或时间无效。"));
    }
    if t["example_media"]
        .iter()
        .any(|r| !exists(&examples, &r["example_id"]) || !exists(&media, &r["asset_id"]))
        || members("example_media", "asset_id") != media
    {
        return Err(bad("同步原声关联缺失。"));
    }
    if t["occurrences"].iter().any(|r| {
        !exists(&sources, &r["source_id"])
            || !exists(&examples, &r["example_id"])
            || (!r["entry_id"].is_null() && &r["entry_id"] != entry)
            || r["token_end"].as_i64() < r["token_start"].as_i64()
    }) {
        return Err(bad("同步出现位置无效。"));
    }
    if t["collection_actions"]
        .iter()
        .any(|r| !r["source_id"].is_null() && !exists(&sources, &r["source_id"]))
        || t["learning_units"].iter().any(|r| {
            r["scope_key"].as_str() != Some("entry") && !exists(&meanings, &r["scope_key"])
        })
    {
        return Err(bad("同步收录或学习范围无效。"));
    }
    if t["review_attempts"]
        .iter()
        .any(|r| !exists(&units, &r["unit_id"]))
        || t["review_corrections"]
            .iter()
            .any(|r| !exists(&attempts, &r["attempt_id"]))
        || members("review_states", "unit_id") != units
    {
        return Err(bad("同步学习记录或安排缺失。"));
    }
    let mut positions = BTreeSet::new();
    for r in &t["review_corrections"] {
        if !positions.insert((
            r["attempt_id"].clone().to_string(),
            r["expected_revision"].clone().to_string(),
        )) {
            return Err(conflict("同一作答出现同时人工修正，需核对后处理。"));
        }
    }
    for r in &t["review_attempts"] {
        let q: Value = serde_json::from_str(r["question_json"].as_str().unwrap())?;
        let dimension = &t["learning_units"]
            .iter()
            .find(|u| u["id"] == r["unit_id"])
            .unwrap()["dimension"];
        let expected_keys: BTreeSet<_> = [
            "entryId",
            "target",
            "dimension",
            "expected",
            "context",
            "audioKind",
            "assetId",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        if !q.is_object()
            || q.as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>()
                != expected_keys
            || &q["entryId"] != entry
            || &q["dimension"] != dimension
            || !q["expected"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
            || !q["context"].is_string()
            || q["target"].as_str().is_none_or(|s| s.trim().is_empty())
            || !matches!(q["audioKind"].as_str(), Some("original" | "system"))
            || (!q["assetId"].is_null() && !exists(&media, &q["assetId"]))
        {
            return Err(bad("同步题目快照无效。"));
        }
    }
    Ok(())
}

pub(crate) fn key(table: &str, row: &Value) -> String {
    if table == "review_states" {
        return row["unit_id"].to_string();
    }
    if table == "entry_examples" {
        return format!(
            "{}:{}:{}",
            row["entry_id"], row["example_id"], row["scope_key"]
        );
    }
    if table == "example_media" {
        return format!("{}:{}", row["example_id"], row["asset_id"]);
    }
    row.get("id").unwrap_or(&row["operation_id"]).to_string()
}

/// Three-way field merge. Event originals remain immutable, and independent additions are retained.
pub fn merge(
    base: Option<&SyncBundle>,
    local: &SyncBundle,
    remote: &SyncBundle,
    prefer: Option<&str>,
) -> Result<SyncBundle, AppError> {
    validate(local)?;
    validate(remote)?;
    if local.entry["id"] != remote.entry["id"] {
        return Err(conflict("需要先明确词条身份映射。"));
    }
    if remote.deleted != local.deleted {
        return Err(conflict("网站移除与本地资料需明确选择，资料不会自动删除。"));
    }
    fn fields(
        base: Option<&Value>,
        left: &Value,
        right: &Value,
        prefer: Option<&str>,
        table: &str,
    ) -> Result<Value, AppError> {
        let mut result = Map::new();
        for (k, l) in left.as_object().unwrap() {
            let r = &right[k];
            let b = base.and_then(|v| v.get(k));
            let immutable = matches!(table, "collection_actions" | "review_corrections")
                || table == "review_attempts"
                    && matches!(
                        k.as_str(),
                        "id" | "operation_id"
                            | "request_hash"
                            | "unit_id"
                            | "question_json"
                            | "answer"
                            | "outcome"
                            | "grader"
                            | "policy_version"
                            | "created_at"
                    );
            let json_equal = k == "question_json"
                && serde_json::from_str::<Value>(l.as_str().unwrap())?
                    == serde_json::from_str::<Value>(r.as_str().unwrap())?;
            let value = if l == r || json_equal {
                l.clone()
            } else if immutable {
                return Err(conflict(
                    "同一原收录、作答或修正存在不同内容，不能覆盖历史。",
                ));
            } else if matches!(
                k.as_str(),
                "revision" | "updated_at" | "hinted" | "relearn_at"
            ) {
                Value::from(l.as_i64().unwrap().max(r.as_i64().unwrap()))
            } else if k == "created_at" {
                Value::from(l.as_i64().unwrap().min(r.as_i64().unwrap()))
            } else if b == Some(l)
                || table == "review_states"
                || table == "review_attempts"
                    && matches!(k.as_str(), "state_before_json" | "state_after_json")
            {
                r.clone()
            } else if b == Some(r) {
                l.clone()
            } else if let Some(choice) = prefer {
                if choice == "local" {
                    l.clone()
                } else {
                    r.clone()
                }
            } else {
                return Err(conflict(&format!("两端同时修改了 {table}.{k}，需要确认。")));
            };
            result.insert(k.clone(), value);
        }
        Ok(Value::Object(result))
    }
    let mut result = local.clone();
    result.entry = fields(
        base.map(|b| &b.entry),
        &local.entry,
        &remote.entry,
        prefer,
        "entries",
    )?;
    for (table, lrows) in &local.tables {
        let mut rows: BTreeMap<String, Value> =
            lrows.iter().map(|r| (key(table, r), r.clone())).collect();
        let baserows: BTreeMap<String, &Value> = base
            .map(|b| b.tables[table].iter().map(|r| (key(table, r), r)).collect())
            .unwrap_or_default();
        for r in &remote.tables[table] {
            let k = key(table, r);
            let merged = if let Some(l) = rows.get(&k) {
                fields(baserows.get(&k).copied(), l, r, prefer, table)?
            } else {
                r.clone()
            };
            rows.insert(k, merged);
        }
        result
            .tables
            .insert(table.clone(), rows.into_values().collect());
    }
    validate(&result)?;
    Ok(result)
}
