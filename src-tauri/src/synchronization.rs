use crate::{
    error::AppError,
    store::Store,
    sync_data::{self, SyncBundle},
    sync_merge,
    vocabulary::digest,
};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

fn error(message: &str) -> AppError {
    AppError::new("sync_conflict", message)
}
fn id<'a>(row: &'a Value, column: &str) -> &'a str {
    row[column].as_str().expect("validated protocol text")
}
pub(crate) fn mark_dirty(connection: &Connection, entry: &str) -> Result<(), AppError> {
    connection.execute("INSERT INTO sync_dirty_entries(entry_id,epoch,updated_at) VALUES (?,1,?) ON CONFLICT(entry_id) DO UPDATE SET epoch=epoch+1,updated_at=excluded.updated_at",params![entry,Utc::now().timestamp_millis()])?;
    Ok(())
}
fn linked(
    c: &Connection,
    scope: &str,
    table: &str,
    remote: &str,
) -> Result<Option<String>, AppError> {
    Ok(c.query_row(
        "SELECT local_id FROM sync_links WHERE account_scope=? AND entity_kind=? AND remote_id=?",
        params![scope, table, remote],
        |r| r.get(0),
    )
    .optional()?)
}
fn link(
    c: &Connection,
    scope: &str,
    table: &str,
    remote: &str,
    local: &str,
) -> Result<(), AppError> {
    if linked(c, scope, table, remote)?.is_some_and(|old| old != local) {
        return Err(error("实体映射已变化，请重新核对。"));
    }
    c.execute("INSERT OR IGNORE INTO sync_links(account_scope,remote_id,entity_kind,local_id) VALUES (?,?,?,?)",params![scope,remote,table,local])?;
    Ok(())
}
type IdMap = BTreeMap<(String, String), String>;
fn mapped(map: &IdMap, table: &str, value: &Value) -> Value {
    value
        .as_str()
        .and_then(|v| map.get(&(table.into(), v.into())))
        .map(|v| Value::String(v.clone()))
        .unwrap_or_else(|| value.clone())
}
fn scope_value(map: &IdMap, value: &Value) -> Value {
    let Some(text) = value.as_str() else {
        return value.clone();
    };
    let (prefix, suffix) = text
        .split_once(':')
        .map(|(p, s)| (p, format!(":{s}")))
        .unwrap_or((text, String::new()));
    Value::String(format!(
        "{}{suffix}",
        map.get(&("meanings".into(), prefix.into()))
            .map(String::as_str)
            .unwrap_or(prefix)
    ))
}
fn rewritten(table: &str, row: &Value, map: &IdMap) -> Result<Value, AppError> {
    let mut row = row.clone();
    for (field, target) in [
        ("entry_id", "entries"),
        ("source_id", "sources"),
        ("example_id", "examples"),
        ("meaning_id", "meanings"),
        ("asset_id", "media_assets"),
        ("unit_id", "learning_units"),
        ("attempt_id", "review_attempts"),
    ] {
        if let Some(value) = row.get(field) {
            row[field] = mapped(map, target, value);
        }
    }
    if row.get("id").is_some() {
        row["id"] = mapped(map, table, &row["id"]);
    }
    if row.get("scope_key").is_some() {
        row["scope_key"] = scope_value(map, &row["scope_key"]);
    }
    if table == "collection_actions" {
        let mut result: Value = serde_json::from_str(id(&row, "result_json"))?;
        if let Some(v) = result.get("entryId") {
            result["entryId"] = mapped(map, "entries", v);
        }
        if let Some(items) = result.get_mut("exampleIds").and_then(Value::as_array_mut) {
            for v in items {
                *v = mapped(map, "examples", v);
            }
        }
        row["result_json"] = serde_json::to_string(&result)?.into();
    }
    if table == "review_attempts" {
        let mut q: Value = serde_json::from_str(id(&row, "question_json"))?;
        q["entryId"] = mapped(map, "entries", &q["entryId"]);
        q["assetId"] = mapped(map, "media_assets", &q["assetId"]);
        row["question_json"] = serde_json::to_string(&q)?.into();
    }
    Ok(row)
}
fn primary(table: &str) -> Vec<&'static str> {
    match table {
        "entries" => vec!["id"],
        "collection_actions" => vec!["operation_id"],
        "entry_examples" => vec!["entry_id", "example_id", "scope_key"],
        "example_media" => vec!["example_id", "asset_id"],
        "review_states" => vec!["unit_id"],
        _ => vec!["id"],
    }
}
fn sql_value(v: &Value) -> SqlValue {
    match v {
        Value::Null => SqlValue::Null,
        Value::Number(n) => SqlValue::Integer(n.as_i64().unwrap()),
        Value::String(s) => SqlValue::Text(s.clone()),
        _ => unreachable!("validated SQL scalar"),
    }
}
fn row_on(c: &Connection, table: &str, row: &Value) -> Result<Option<Value>, AppError> {
    let definition = sync_data::schema();
    let columns = if table == "entries" {
        &definition["entry"]
    } else {
        &definition["tables"][table]
    };
    let names = columns
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let keys = primary(table);
    let query = format!(
        "SELECT {} FROM {table} WHERE {}",
        names.join(","),
        keys.iter()
            .map(|k| format!("{k}=?"))
            .collect::<Vec<_>>()
            .join(" AND ")
    );
    Ok(c.query_row(
        &query,
        rusqlite::params_from_iter(keys.iter().map(|k| sql_value(&row[*k]))),
        |r| {
            let mut object = serde_json::Map::new();
            for (i, name) in names.iter().enumerate() {
                let v = r.get::<_, SqlValue>(i)?;
                object.insert(
                    name.clone(),
                    match v {
                        SqlValue::Null => Value::Null,
                        SqlValue::Integer(n) => n.into(),
                        SqlValue::Text(s) => s.into(),
                        _ => unreachable!(),
                    },
                );
            }
            Ok(Value::Object(object))
        },
    )
    .optional()?)
}
fn put_row(c: &Connection, table: &str, row: &Value) -> Result<(), AppError> {
    let mut row = row.clone();
    if let Some(old) = row_on(c, table, &row)? {
        if table == "media_assets"
            && (old["digest"] != row["digest"] || old["format"] != row["format"])
        {
            return Err(error("同一原声标识对应不同文件，不能覆盖已保存音频。"));
        }
        for (k, v) in old.as_object().unwrap() {
            let frozen = matches!(table, "collection_actions" | "review_corrections")
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
            let same = if matches!(k.as_str(), "question_json" | "result_json") {
                serde_json::from_str::<Value>(v.as_str().unwrap())?
                    == serde_json::from_str::<Value>(row[k].as_str().unwrap())?
            } else {
                v == &row[k]
            };
            if frozen && !same {
                return Err(error("原学习事件存在不同内容，未修改本地词库。"));
            }
            if matches!(k.as_str(), "revision" | "hinted") {
                row[k] = Value::from(v.as_i64().unwrap().max(row[k].as_i64().unwrap()));
            }
            if k == "created_at" && !frozen {
                row[k] = Value::from(v.as_i64().unwrap().min(row[k].as_i64().unwrap()));
            }
        }
        if table == "media_assets" {
            row["recipe_key"] = old["recipe_key"].clone();
        }
    }
    let mut fields = row.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    let mut values = fields
        .iter()
        .map(|k| sql_value(&row[k]))
        .collect::<Vec<_>>();
    if table == "media_assets" {
        fields.push("relative_path".into());
        values.push(SqlValue::Text(format!(
            "media/sync-{}.{}",
            id(&row, "digest"),
            id(&row, "format")
        )));
    }
    let keys = primary(table);
    let updates = fields
        .iter()
        .filter(|k| !keys.contains(&k.as_str()) && k.as_str() != "relative_path")
        .map(|k| format!("{k}=excluded.{k}"))
        .collect::<Vec<_>>();
    let sql = format!(
        "INSERT INTO {table}({}) VALUES ({}) ON CONFLICT({}) DO {}",
        fields.join(","),
        vec!["?"; fields.len()].join(","),
        keys.join(","),
        if updates.is_empty() {
            "NOTHING".into()
        } else {
            format!("UPDATE SET {}", updates.join(","))
        }
    );
    c.execute(&sql, rusqlite::params_from_iter(values))?;
    Ok(())
}
fn natural(c: &Connection, table: &str, row: &Value) -> Result<Option<String>, AppError> {
    let (query, values) = match table {
        "meanings" => (
            "SELECT id FROM meanings WHERE entry_id=? AND text=?",
            vec![sql_value(&row["entry_id"]), sql_value(&row["text"])],
        ),
        "sources" => (
            "SELECT id FROM sources WHERE kind=? AND fingerprint=?",
            vec![sql_value(&row["kind"]), sql_value(&row["fingerprint"])],
        ),
        "examples" => (
            "SELECT id FROM examples WHERE identity_key=?",
            vec![sql_value(&row["identity_key"])],
        ),
        "media_assets" => (
            "SELECT id FROM media_assets WHERE digest=? AND format=? AND kind='original'",
            vec![sql_value(&row["digest"]), sql_value(&row["format"])],
        ),
        "learning_units" => (
            "SELECT id FROM learning_units WHERE entry_id=? AND scope_key=? AND dimension=?",
            vec![
                sql_value(&row["entry_id"]),
                sql_value(&row["scope_key"]),
                sql_value(&row["dimension"]),
            ],
        ),
        "occurrences" => (
            "SELECT id FROM occurrences WHERE source_id=? AND example_id=? AND candidate_key=? AND token_start=? AND token_end=?",
            vec![
                sql_value(&row["source_id"]),
                sql_value(&row["example_id"]),
                sql_value(&row["candidate_key"]),
                sql_value(&row["token_start"]),
                sql_value(&row["token_end"]),
            ],
        ),
        _ => return Ok(None),
    };
    Ok(
        c.query_row(query, rusqlite::params_from_iter(values), |r| r.get(0))
            .optional()?,
    )
}

fn import_on(
    c: &Connection,
    scope: &str,
    bundle: &SyncBundle,
    target: Option<&str>,
    root: &std::path::Path,
) -> Result<String, AppError> {
    sync_merge::validate(bundle)?;
    let remote = id(&bundle.entry, "id");
    let local = target
        .map(str::to_owned)
        .or(linked(c, scope, "entries", remote)?)
        .unwrap_or_else(|| remote.into());
    link(c, scope, "entries", remote, &local)?;
    let mut map = IdMap::from([(("entries".into(), remote.into()), local.clone())]);
    let mut entry = bundle.entry.clone();
    entry["id"] = local.clone().into();
    put_row(c, "entries", &entry)?;
    c.execute(
        "UPDATE entries SET archived=? WHERE id=?",
        params![bundle.deleted, &local],
    )?;
    for table in [
        "sources",
        "meanings",
        "media_assets",
        "examples",
        "learning_units",
        "occurrences",
        "entry_examples",
        "example_media",
        "collection_actions",
        "review_attempts",
        "review_corrections",
        "review_states",
    ] {
        for source in &bundle.tables[table] {
            let mut row = rewritten(table, source, &map)?;
            if table == "examples" {
                row["identity_key"] = digest(
                    serde_json::to_string(&(
                        row["source_id"].as_str().unwrap_or("manual"),
                        id(&row, "location_key"),
                    ))?
                    .as_bytes(),
                )
                .into();
            }
            if source.get("id").is_some() {
                let remote_id = id(source, "id");
                let existing = linked(c, scope, table, remote_id)?.or(natural(c, table, &row)?);
                let local_id = existing.unwrap_or_else(|| remote_id.into());
                map.insert((table.into(), remote_id.into()), local_id.clone());
                link(c, scope, table, remote_id, &local_id)?;
                row["id"] = local_id.into();
            }
            put_row(c, table, &row)?;
            if table == "media_assets" {
                let asset = id(&row, "id");
                let relative: String = c.query_row(
                    "SELECT relative_path FROM media_assets WHERE id=?",
                    [asset],
                    |r| r.get(0),
                )?;
                if crate::corpus::validate_media_file(root, &relative, id(&row, "digest"), "ready")
                    .is_err()
                {
                    let repaired =
                        format!("media/sync-{}.{}", id(&row, "digest"), id(&row, "format"));
                    crate::corpus::validate_media_file(
                        root,
                        &repaired,
                        id(&row, "digest"),
                        "ready",
                    )?;
                    c.execute(
                        "UPDATE media_assets SET relative_path=?,state='ready' WHERE id=?",
                        params![repaired, asset],
                    )?;
                }
            }
        }
    }
    crate::reviews::ensure_units(c, &local, bundle.entry["created_at"].as_i64().unwrap())?;
    crate::reviews::recompute_entry(c, &local)?;
    c.execute("UPDATE review_attempts SET revision=MAX(revision,COALESCE((SELECT MAX(expected_revision)+1 FROM review_corrections WHERE attempt_id=review_attempts.id),1)) WHERE unit_id IN (SELECT id FROM learning_units WHERE entry_id=?)",[&local])?;
    Ok(local)
}

fn map_on(c: &Connection, scope: &str) -> Result<IdMap, AppError> {
    let mut q =
        c.prepare("SELECT entity_kind,remote_id,local_id FROM sync_links WHERE account_scope=?")?;
    Ok(
        q.query_map([scope], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?
            .collect::<Result<_, _>>()?,
    )
}
fn outbound(
    c: &Connection,
    scope: &str,
    remote: &str,
    local: &str,
    base: Option<&SyncBundle>,
) -> Result<SyncBundle, AppError> {
    let current = sync_data::export_on(c, local)?;
    let incoming = map_on(c, scope)?;
    let mut reverse = IdMap::new();
    // Prefer the identifiers already used in this remote document.
    if let Some(base) = base {
        for (table, rows) in &base.tables {
            for r in rows {
                if let Some(rid) = r.get("id").and_then(Value::as_str) {
                    let lid = incoming
                        .get(&(table.clone(), rid.into()))
                        .map(String::as_str)
                        .unwrap_or(rid);
                    reverse
                        .entry((table.clone(), lid.into()))
                        .or_insert(rid.into());
                }
            }
        }
    }
    for ((table, rid), lid) in &incoming {
        reverse
            .entry((table.clone(), lid.clone()))
            .or_insert(rid.clone());
    }
    reverse.insert(("entries".into(), local.into()), remote.into());
    let mut result = current.clone();
    result.entry["id"] = remote.into();
    result.deleted = c.query_row("SELECT archived FROM entries WHERE id=?", [local], |r| {
        r.get(0)
    })?;
    for (table, rows) in &current.tables {
        let mut used = BTreeSet::new();
        let mut output = vec![];
        if let Some(base) = base {
            for original in &base.tables[table] {
                let local_row = rewritten(table, original, &incoming)?;
                if let Some(current_row) = rows
                    .iter()
                    .find(|r| sync_merge::key(table, r) == sync_merge::key(table, &local_row))
                {
                    used.insert(sync_merge::key(table, current_row));
                    let mut value = rewritten(table, current_row, &reverse)?;
                    // Preserve exact aliases for fields already known in the remote record.
                    for (field, v) in original.as_object().unwrap() {
                        if local_row.get(field) == current_row.get(field)
                            && matches!(
                                field.as_str(),
                                "id" | "entry_id"
                                    | "source_id"
                                    | "example_id"
                                    | "meaning_id"
                                    | "asset_id"
                                    | "unit_id"
                                    | "attempt_id"
                                    | "scope_key"
                            )
                        {
                            value[field] = v.clone();
                        }
                    }
                    if matches!(table.as_str(), "collection_actions" | "review_corrections") {
                        value = original.clone();
                    }
                    if table == "review_attempts" {
                        value["question_json"] = original["question_json"].clone();
                    }
                    output.push(value);
                } else {
                    return Err(error("已同步的来源或学习记录缺失，请保留资料并检查。"));
                }
            }
        }
        for row in rows {
            if !used.contains(&sync_merge::key(table, row)) {
                output.push(rewritten(table, row, &reverse)?);
            }
        }
        result.tables.insert(table.clone(), output);
    }
    sync_merge::validate(&result)?;
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteChange {
    pub cursor: i64,
    pub entry_id: String,
    pub revision: i64,
    pub change_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePage {
    pub changes: Vec<RemoteChange>,
    #[serde(default)]
    pub backfill_pending: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDocument {
    pub entry_id: String,
    pub revision: i64,
    pub bundle: SyncBundle,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Push {
    pub change_id: String,
    pub base_revision: i64,
    pub bundle: SyncBundle,
}
#[derive(Debug, Clone)]
pub enum PushReply {
    Saved { i: i64 },
    Conflict(RemoteDocument),
}
pub trait Remote {
    fn changes(&mut self, after: i64) -> Result<ChangePage, AppError>;
    fn change(&mut self, cursor: i64) -> Result<RemoteDocument, AppError>;
    fn push(&mut self, request: &Push) -> Result<PushReply, AppError>;
    fn upload(&mut self, digest: &str, format: &str, bytes: Vec<u8>) -> Result<(), AppError>;
    fn download(&mut self, digest: &str, format: &str) -> Result<Vec<u8>, AppError>;
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub id: String,
    pub remote_id: String,
    pub local_id: Option<String>,
    pub text: String,
    pub reason: String,
    pub remote_revision: i64,
    pub local: Option<SyncBundle>,
    pub remote: SyncBundle,
    pub candidates: Vec<crate::vocabulary::Entry>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolutionInput {
    pub conflict_id: String,
    pub choice: String,
    pub target_entry_id: Option<String>,
    pub expected_remote_revision: i64,
    pub expected_local_revision: Option<i64>,
    pub expected_target_revision: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub cursor: i64,
    pub pending: usize,
    pub conflicts: usize,
}
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub pulled: usize,
    pub pushed: usize,
    pub conflicts: usize,
    pub cursor: i64,
}
type DocumentRecord = (String, i64, Option<SyncBundle>, i64);
fn document(c: &Connection, scope: &str, remote: &str) -> Result<Option<DocumentRecord>, AppError> {
    let r:Option<(String,i64,Option<String>,i64)>=c.query_row("SELECT local_id,remote_revision,base_json,acknowledged_epoch FROM sync_documents WHERE account_scope=? AND remote_id=?",params![scope,remote],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    r.map(|(local, rev, json, epoch)| {
        Ok((
            local,
            rev,
            json.map(|s| serde_json::from_str(&s)).transpose()?,
            epoch,
        ))
    })
    .transpose()
}
fn save_conflict(
    c: &Connection,
    scope: &str,
    remote: &RemoteDocument,
    local: Option<&str>,
    payload: Option<&SyncBundle>,
    reason: &str,
) -> Result<(), AppError> {
    c.execute("INSERT INTO sync_conflicts(id,account_scope,remote_id,local_id,remote_revision,remote_json,local_json,reason,created_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(account_scope,remote_id,state) DO UPDATE SET local_id=excluded.local_id,remote_revision=excluded.remote_revision,remote_json=excluded.remote_json,local_json=excluded.local_json,reason=excluded.reason",
        params![Uuid::new_v4().to_string(),scope,remote.entry_id,local,remote.revision,serde_json::to_string(&remote.bundle)?,payload.map(serde_json::to_string).transpose()?,reason,Utc::now().timestamp_millis()])?;
    Ok(())
}
fn set_document(
    c: &Connection,
    scope: &str,
    remote: &RemoteDocument,
    local: &str,
    epoch: i64,
) -> Result<(), AppError> {
    c.execute("INSERT INTO sync_documents(account_scope,remote_id,local_id,remote_revision,base_json,acknowledged_epoch) VALUES (?,?,?,?,?,?) ON CONFLICT(account_scope,remote_id) DO UPDATE SET local_id=excluded.local_id,remote_revision=excluded.remote_revision,base_json=excluded.base_json,acknowledged_epoch=excluded.acknowledged_epoch",params![scope,remote.entry_id,local,remote.revision,serde_json::to_string(&remote.bundle)?,epoch])?;
    Ok(())
}
fn check_cancel(cancelled: &AtomicBool) -> Result<(), AppError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(AppError::new(
            "cancelled",
            "同步已取消，已保存内容与待同步资料保留。",
        ))
    } else {
        Ok(())
    }
}

impl Store {
    pub fn synchronization_status(&self, scope: &str) -> Result<SyncStatus, AppError> {
        let c = self.connection()?;
        Ok(SyncStatus {
            cursor: c
                .query_row(
                    "SELECT state_json FROM sync_state WHERE account_scope=?",
                    [scope],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .and_then(|v| v["cursor"].as_i64())
                .unwrap_or(0),
            pending: c.query_row(
                "SELECT COUNT(*) FROM sync_dirty_entries x WHERE NOT EXISTS(SELECT 1 FROM sync_documents d WHERE d.account_scope=?1 AND d.local_id=x.entry_id) OR EXISTS(SELECT 1 FROM sync_documents d WHERE d.account_scope=?1 AND d.local_id=x.entry_id AND d.acknowledged_epoch<x.epoch)",
                [scope],
                |r| r.get(0),
            )?,
            conflicts: c.query_row(
                "SELECT COUNT(*) FROM sync_conflicts WHERE account_scope=? AND state='pending'",
                [scope],
                |r| r.get(0),
            )?,
        })
    }
    pub fn synchronization_conflicts(&self, scope: &str) -> Result<Vec<SyncConflict>, AppError> {
        let c = self.connection()?;
        let mut q=c.prepare("SELECT id,remote_id,local_id,remote_revision,remote_json,local_json,reason FROM sync_conflicts WHERE account_scope=? AND state='pending' ORDER BY created_at,id")?;
        let rows = q
            .query_map([scope], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(q);
        drop(c);
        rows.into_iter()
            .map(
                |(
                    conflict_id,
                    remote_id,
                    local_id,
                    remote_revision,
                    remote_json,
                    local_json,
                    reason,
                )| {
                    let remote: SyncBundle = serde_json::from_str(&remote_json)?;
                    let candidates =
                        self.find_entries(id(&remote.entry, "kind"), id(&remote.entry, "text"))?;
                    let local = if let Some(local_id) = &local_id {
                        let c = self.connection()?;
                        let base = document(&c, scope, &remote_id)?.and_then(|d| d.2);
                        Some(outbound(&c, scope, &remote_id, local_id, base.as_ref())?)
                    } else {
                        local_json.map(|s| serde_json::from_str(&s)).transpose()?
                    };
                    Ok(SyncConflict {
                        id: conflict_id,
                        remote_id,
                        local_id,
                        text: id(&remote.entry, "text").into(),
                        reason,
                        remote_revision,
                        local,
                        remote,
                        candidates,
                    })
                },
            )
            .collect()
    }
    fn save_media<R: Remote>(
        &self,
        bundle: &SyncBundle,
        remote: &mut R,
        cancel: &AtomicBool,
    ) -> Result<(), AppError> {
        for asset in &bundle.tables["media_assets"] {
            check_cancel(cancel)?;
            let hash = id(asset, "digest");
            let format = id(asset, "format");
            let path = self.root().join(format!("media/sync-{hash}.{format}"));
            if self.media_file_for_digest(hash, format).is_ok() {
                continue;
            }
            if path.is_file() && crate::corpus::file_hash(&path)? == hash {
                continue;
            }
            let bytes = remote.download(hash, format)?;
            check_cancel(cancel)?;
            if bytes.len() < 12 || bytes.len() > 20 * 1024 * 1024 || digest(&bytes) != hash {
                return Err(AppError::new(
                    "invalid_data",
                    "下载原声的字节或摘要不匹配，游标与本地资料已保留。",
                ));
            }
            let temp = self
                .root()
                .join("jobs")
                .join(format!("sync-{}.tmp", Uuid::new_v4()));
            std::fs::write(&temp, &bytes)?;
            if let Err(e) = std::fs::rename(&temp, &path) {
                let _ = std::fs::remove_file(&temp);
                return Err(e.into());
            }
        }
        Ok(())
    }
    fn receive<R: Remote>(
        &self,
        scope: &str,
        remote_doc: &RemoteDocument,
        remote: &mut R,
        cancel: &AtomicBool,
    ) -> Result<(), AppError> {
        sync_merge::validate(&remote_doc.bundle)?;
        if remote_doc.entry_id != id(&remote_doc.bundle.entry, "id") || remote_doc.revision < 1 {
            return Err(AppError::new("invalid_data", "同步版本或词条标识无效。"));
        }
        {
            let c = self.connection()?;
            if document(&c, scope, &remote_doc.entry_id)?
                .is_some_and(|(_, rev, _, _)| rev >= remote_doc.revision)
            {
                return Ok(());
            }
        }
        self.save_media(&remote_doc.bundle, remote, cancel)?;
        check_cancel(cancel)?;
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let prior = document(&tx, scope, &remote_doc.entry_id)?;
        let target = if let Some((local, _, _, _)) = &prior {
            Some(local.clone())
        } else {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM entries WHERE id=?",
                    [&remote_doc.entry_id],
                    |r| r.get(0),
                )
                .optional()?;
            let mut aliases = BTreeSet::new();
            for action in &remote_doc.bundle.tables["collection_actions"] {
                if let Some((local, hash)) = tx
                    .query_row(
                        "SELECT entry_id,request_hash FROM collection_actions WHERE operation_id=?",
                        [id(action, "operation_id")],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )
                    .optional()?
                {
                    if hash != id(action, "request_hash") {
                        return Err(error("同一原收录标识存在不同内容。"));
                    }
                    aliases.insert(local);
                }
            }
            if aliases.len() > 1 {
                return Err(error("同步原收录涉及多个本地词条，需要核对。"));
            }
            existing.or_else(|| aliases.into_iter().next())
        };
        if target.is_none() {
            let matches: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM entries WHERE kind=? AND match_key=? AND archived=0)",
                params![
                    remote_doc.bundle.entry["kind"].as_str(),
                    remote_doc.bundle.entry["match_key"].as_str()
                ],
                |r| r.get(0),
            )?;
            if matches || remote_doc.bundle.deleted {
                save_conflict(
                    &tx,
                    scope,
                    remote_doc,
                    None,
                    None,
                    if remote_doc.bundle.deleted {
                        "网站已移除此词条，请确认是否保留资料。"
                    } else {
                        "发现同形词，请选择合并到已有词条或独立保存。"
                    },
                )?;
                tx.commit()?;
                return Ok(());
            }
        }
        let merged = if let Some(local) = &target {
            let base = prior.as_ref().and_then(|(_, _, b, _)| b.as_ref());
            let current = outbound(&tx, scope, &remote_doc.entry_id, local, base)?;
            let epoch: i64 = tx.query_row(
                "SELECT COALESCE((SELECT epoch FROM sync_dirty_entries WHERE entry_id=?),0)",
                [local],
                |r| r.get(0),
            )?;
            let clean = prior.as_ref().is_some_and(|p| epoch <= p.3)
                && current.deleted == remote_doc.bundle.deleted;
            if clean {
                remote_doc.bundle.clone()
            } else {
                match sync_merge::merge(base, &current, &remote_doc.bundle, None) {
                    Ok(m) => m,
                    Err(e) if e.code == "sync_conflict" => {
                        save_conflict(
                            &tx,
                            scope,
                            remote_doc,
                            Some(local),
                            Some(&current),
                            &e.message,
                        )?;
                        tx.commit()?;
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                }
            }
        } else {
            remote_doc.bundle.clone()
        };
        let local = import_on(&tx, scope, &merged, target.as_deref(), self.root())?;
        mark_dirty(&tx, &local)?;
        let epoch: i64 = tx.query_row(
            "SELECT epoch FROM sync_dirty_entries WHERE entry_id=?",
            [&local],
            |r| r.get(0),
        )?;
        set_document(
            &tx,
            scope,
            remote_doc,
            &local,
            if serde_json::to_value(&merged)? == serde_json::to_value(&remote_doc.bundle)? {
                epoch
            } else {
                prior.as_ref().map(|p| p.3).unwrap_or(0)
            },
        )?;
        tx.commit()?;
        Ok(())
    }
    fn enqueue(&self, scope: &str) -> Result<(), AppError> {
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let mut q=tx.prepare("SELECT id FROM entries WHERE NOT EXISTS(SELECT 1 FROM sync_documents WHERE account_scope=?1 AND local_id=entries.id) AND archived=0 AND NOT EXISTS(SELECT 1 FROM sync_conflicts f WHERE f.account_scope=?1 AND f.state='pending' AND (f.local_id=entries.id OR (json_extract(f.remote_json,'$.entry.kind')=entries.kind AND json_extract(f.remote_json,'$.entry.match_key')=entries.match_key)))")?;
        let ids = q
            .query_map([scope], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(q);
        for local in ids {
            tx.execute(
                "INSERT INTO sync_documents(account_scope,remote_id,local_id) VALUES (?,?,?)",
                params![scope, local, local],
            )?;
            link(&tx, scope, "entries", &local, &local)?;
        }
        let mut q=tx.prepare("SELECT d.remote_id,d.local_id,e.epoch FROM sync_documents d JOIN sync_dirty_entries e ON e.entry_id=d.local_id WHERE d.account_scope=? AND e.epoch>d.acknowledged_epoch AND NOT EXISTS(SELECT 1 FROM sync_outbox o WHERE o.account_scope=d.account_scope AND o.entity_id=d.remote_id AND o.state IN ('pending','conflict')) AND NOT EXISTS(SELECT 1 FROM sync_conflicts f WHERE f.account_scope=d.account_scope AND f.state='pending' AND (f.remote_id=d.remote_id OR f.local_id=d.local_id))")?;
        let rows = q
            .query_map([scope], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(q);
        for (remote, local, epoch) in rows {
            let (_, revision, base, _) = document(&tx, scope, &remote)?.unwrap();
            let bundle = outbound(&tx, scope, &remote, &local, base.as_ref())?;
            let change_id = Uuid::new_v4().to_string();
            let push = Push {
                change_id: change_id.clone(),
                base_revision: revision,
                bundle,
            };
            tx.execute("INSERT INTO sync_outbox(change_id,entity_kind,entity_id,base_revision,payload_json,created_at,account_scope,local_epoch) VALUES (?,'entry',?,?,?,?,?,?)",params![change_id,remote,revision,serde_json::to_string(&push)?,Utc::now().timestamp_millis(),scope,epoch])?;
        }
        tx.commit()?;
        Ok(())
    }
    fn send_pending<R: Remote>(
        &self,
        scope: &str,
        remote: &mut R,
        cancel: &AtomicBool,
        result: &mut SyncResult,
    ) -> Result<(), AppError> {
        let rows = {
            let c = self.connection()?;
            let mut q=c.prepare("SELECT change_id,payload_json,local_epoch FROM sync_outbox WHERE account_scope=? AND state='pending' ORDER BY created_at,change_id")?;
            q.query_map([scope], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        for (change, json, epoch) in rows {
            check_cancel(cancel)?;
            let request: Push = serde_json::from_str(&json)?;
            for asset in &request.bundle.tables["media_assets"] {
                check_cancel(cancel)?;
                let bytes = std::fs::read(
                    self.media_file_for_digest(id(asset, "digest"), id(asset, "format"))?,
                )?;
                remote.upload(id(asset, "digest"), id(asset, "format"), bytes)?;
            }
            check_cancel(cancel)?;
            let reply = remote.push(&request)?;
            let mut c = self.connection()?;
            let tx = c.transaction()?;
            let (local, _, _, _) = document(&tx, scope, id(&request.bundle.entry, "id"))?
                .ok_or_else(|| error("同步映射已变化。"))?;
            match reply {
                PushReply::Saved { i } => {
                    if i != request.base_revision + 1 {
                        return Err(AppError::new(
                            "invalid_data",
                            "站点回执版本无效，待同步内容保留。",
                        ));
                    }
                    set_document(
                        &tx,
                        scope,
                        &RemoteDocument {
                            entry_id: id(&request.bundle.entry, "id").into(),
                            revision: i,
                            bundle: request.bundle,
                        },
                        &local,
                        epoch,
                    )?;
                    tx.execute(
                        "UPDATE sync_outbox SET state='sent' WHERE change_id=?",
                        [change],
                    )?;
                    result.pushed += 1;
                }
                PushReply::Conflict(doc) => {
                    save_conflict(
                        &tx,
                        scope,
                        &doc,
                        Some(&local),
                        Some(&request.bundle),
                        "两端版本发生冲突，请核对后合并。",
                    )?;
                    tx.execute(
                        "UPDATE sync_outbox SET state='conflict' WHERE change_id=?",
                        [change],
                    )?;
                }
            }
            tx.commit()?;
        }
        Ok(())
    }
    fn media_file_for_digest(
        &self,
        hash: &str,
        format: &str,
    ) -> Result<std::path::PathBuf, AppError> {
        let c = self.connection()?;
        let asset: Option<String> = c
            .query_row(
                "SELECT id FROM media_assets WHERE digest=? AND format=? AND kind='original'",
                params![hash, format],
                |r| r.get(0),
            )
            .optional()?;
        drop(c);
        self.media_file(
            &asset.ok_or_else(|| {
                AppError::new("resource_missing", "原声资料缺失，待同步内容保留。")
            })?,
        )
    }
    pub fn synchronize<R: Remote>(
        &self,
        scope: &str,
        remote: &mut R,
        cancel: &AtomicBool,
        progress: impl Fn(&str, usize),
    ) -> Result<SyncResult, AppError> {
        let mut result = SyncResult::default();
        self.send_pending(scope, remote, cancel, &mut result)?;
        let mut cursor = self.synchronization_status(scope)?.cursor;
        let mut caught_up = false;
        for _ in 0..10_000 {
            check_cancel(cancel)?;
            progress("pull", result.pulled);
            let page = remote.changes(cursor)?;
            if page.changes.is_empty() {
                if page.backfill_pending {
                    continue;
                }
                caught_up = true;
                break;
            }
            for change in page.changes {
                check_cancel(cancel)?;
                if change.cursor <= cursor
                    || change.revision < 1
                    || Uuid::parse_str(&change.entry_id).is_err()
                {
                    return Err(AppError::new("invalid_data", "站点同步游标或标识无效。"));
                }
                let doc = remote.change(change.cursor)?;
                if doc.entry_id != change.entry_id || doc.revision != change.revision {
                    return Err(AppError::new("invalid_data", "站点变更与资料包不一致。"));
                }
                self.receive(scope, &doc, remote, cancel)?;
                let c = self.connection()?;
                c.execute("INSERT INTO sync_state(account_scope,state_json) VALUES (?,?) ON CONFLICT(account_scope) DO UPDATE SET state_json=excluded.state_json",params![scope,serde_json::to_string(&json!({"cursor":change.cursor}))?])?;
                cursor = change.cursor;
                result.pulled += 1;
            }
        }
        if !caught_up {
            return Err(AppError::new(
                "sync_incomplete",
                "本次同步尚有远端资料待拉取，已保存当前进度，请再次同步继续。",
            ));
        }
        check_cancel(cancel)?;
        self.enqueue(scope)?;
        progress("push", result.pushed);
        self.send_pending(scope, remote, cancel, &mut result)?;
        let status = self.synchronization_status(scope)?;
        result.cursor = status.cursor;
        result.conflicts = status.conflicts;
        Ok(result)
    }
    pub fn synchronization_resolve<R: Remote>(
        &self,
        scope: &str,
        input: &ResolutionInput,
        remote: &mut R,
        cancel: &AtomicBool,
    ) -> Result<(), AppError> {
        let conflict_id = input.conflict_id.as_str();
        let choice = input.choice.as_str();
        let target = input.target_entry_id.as_deref();
        if !matches!(
            choice,
            "merge" | "local" | "remote" | "new" | "archive" | "restore"
        ) {
            return Err(AppError::new("invalid_input", "同步处理选择无效。"));
        }
        let saved = self
            .synchronization_conflicts(scope)?
            .into_iter()
            .find(|c| c.id == conflict_id)
            .ok_or_else(|| error("待处理冲突已变化。"))?;
        if saved.remote_revision != input.expected_remote_revision {
            return Err(error("网站冲突版本已变化，请刷新后重新选择。"));
        }
        let doc = RemoteDocument {
            entry_id: saved.remote_id.clone(),
            revision: saved.remote_revision,
            bundle: saved.remote.clone(),
        };
        self.save_media(&saved.remote, remote, cancel)?;
        check_cancel(cancel)?;
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let local = saved.local_id.as_deref().or(target);
        if let Some(local) = local {
            let revision: i64 =
                tx.query_row("SELECT revision FROM entries WHERE id=?", [local], |r| {
                    r.get(0)
                })?;
            let expected = if saved.local_id.is_some() {
                input.expected_local_revision
            } else {
                input.expected_target_revision
            };
            if expected != Some(revision) {
                return Err(error("本地词条已变化，请刷新冲突资料后重新选择。"));
            }
            let matches: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM entries WHERE id=? AND kind=?)",
                params![local, id(&saved.remote.entry, "kind")],
                |r| r.get(0),
            )?;
            if !matches {
                return Err(error("待合并本地词条不存在或类型不同。"));
            }
        }
        if saved.remote.deleted && !matches!(choice, "archive" | "restore") {
            return Err(error("请明确选择归档或恢复；已有资料不会删除。"));
        }
        let mut remote_bundle = saved.remote.clone();
        if choice == "restore" {
            remote_bundle.deleted = false;
        }
        let merged = if let Some(local) = local {
            let prior = document(&tx, scope, &saved.remote_id)?;
            let base = prior.as_ref().and_then(|p| p.2.as_ref());
            let mut current = outbound(&tx, scope, &saved.remote_id, local, base)?;
            if matches!(choice, "archive" | "restore") {
                current.deleted = remote_bundle.deleted;
            }
            sync_merge::merge(
                base,
                &current,
                &remote_bundle,
                match choice {
                    "local" | "restore" => Some("local"),
                    "merge" if saved.local_id.is_none() => Some("local"),
                    "remote" | "archive" => Some("remote"),
                    _ => None,
                },
            )?
        } else {
            remote_bundle
        };
        let local = import_on(&tx, scope, &merged, local, self.root())?;
        mark_dirty(&tx, &local)?;
        set_document(&tx, scope, &doc, &local, 0)?;
        tx.execute(
            "DELETE FROM sync_conflicts WHERE id=? AND account_scope=?",
            params![conflict_id, scope],
        )?;
        tx.execute("UPDATE sync_outbox SET state='superseded' WHERE account_scope=? AND entity_id=? AND state='conflict'",params![scope,saved.remote_id])?;
        tx.commit()?;
        Ok(())
    }
}
