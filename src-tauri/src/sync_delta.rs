//! Transport-only changes against an acknowledged complete bundle.
use crate::{error::AppError, store::Store};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) const PROTOCOL: &str = "json-delta-1";

#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
enum Operation {
    Set { path: String, value: Value },
    Remove { path: String },
    Splice { path: String, index: usize, #[serde(rename = "deleteCount")] delete_count: usize, items: Vec<Value> },
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Delta { protocol: String, ops: Vec<Operation> }

fn invalid() -> AppError { AppError::new("invalid_data", "同步差量或确认基准无效，完整资料与游标保留。") }

pub(crate) fn apply(base: &Value, delta: &Value) -> Result<Value, AppError> {
    let delta: Delta = serde_json::from_value(delta.clone()).map_err(|_| invalid())?;
    if delta.protocol != PROTOCOL { return Err(invalid()); }
    let mut result = base.clone();
    for op in delta.ops {
        match op {
            Operation::Splice { path, index, delete_count, items } => {
                let array = result.pointer_mut(&path).and_then(Value::as_array_mut).ok_or_else(invalid)?;
                if index > array.len() || delete_count > array.len() - index { return Err(invalid()); }
                array.splice(index..index + delete_count, items);
            }
            Operation::Set { path, value } if path.is_empty() => result = value,
            other => {
                let (path, value) = match other {
                    Operation::Set { path, value } => (path, Some(value)),
                    Operation::Remove { path } => (path, None),
                    _ => unreachable!(),
                };
                let (parent, key) = path.rsplit_once('/').ok_or_else(invalid)?;
                let key = key.replace("~1", "/").replace("~0", "~");
                let parent = result.pointer_mut(parent).ok_or_else(invalid)?;
                match (parent, value) {
                    (Value::Object(object), Some(value)) => { object.insert(key, value); }
                    (Value::Object(object), None) => { object.remove(&key).ok_or_else(invalid)?; }
                    (Value::Array(array), Some(value)) => {
                        let index: usize = key.parse().map_err(|_| invalid())?;
                        *array.get_mut(index).ok_or_else(invalid)? = value;
                    }
                    _ => return Err(invalid()),
                }
            }
        }
    }
    Ok(result)
}

pub(crate) fn restore_document(mut value: Value, base: Option<(i64, &Value)>) -> Result<Value, AppError> {
    let Some(delta) = value.get("delta") else { return Ok(value); };
    let (revision, base) = base.ok_or_else(invalid)?;
    let (identity, root) = if base.get("session").is_some() { ("sessionId", "session") } else { ("entryId", "entry") };
    if revision <= 0 || value["baseRevision"].as_i64() != Some(revision)
        || value["revision"].as_i64().is_none_or(|v|v<=revision)
        || value[identity] != base[root]["id"] || value.get("bundle").is_some() {
        return Err(invalid());
    }
    let bundle = apply(base, delta)?;
    if bundle[root]["id"] != value[identity] || serde_json::to_vec(&bundle)?.len() > crate::sync_limits::MAX_BUNDLE_BYTES {
        return Err(invalid());
    }
    let object = value.as_object_mut().ok_or_else(invalid)?;
    object.remove("delta");
    object.remove("baseRevision");
    object.insert("bundle".into(), bundle);
    Ok(value)
}

fn child(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn diff(base: &Value, next: &Value, path: &str, ops: &mut Vec<Value>) {
    if base == next { return; }
    match (base, next) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().filter(|key| !b.contains_key(*key)) {
                ops.push(json!({"op":"remove","path":child(path,key)}));
            }
            for (key, value) in b {
                let path = child(path, key);
                if let Some(old) = a.get(key) { diff(old, value, &path, ops); }
                else { ops.push(json!({"op":"set","path":path,"value":value})); }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            // Equal rows anchor unchanged spans even when UUID-sorted history grows in the middle.
            let mut prefix = 0;
            while prefix < a.len().min(b.len()) && a[prefix] == b[prefix] { prefix += 1; }
            let indexed: BTreeMap<String, usize> = a.iter().enumerate().skip(prefix)
                .map(|(i,v)| (v.to_string(),i)).collect();
            let (mut old, mut new) = (prefix, prefix);
            for (i, value) in b.iter().enumerate().skip(prefix) {
                if let Some(&j) = indexed.get(&value.to_string()).filter(|&&j| j >= old) {
                    span(&a[old..j], &b[new..i], path, new, ops);
                    old = j + 1;
                    new = i + 1;
                }
            }
            span(&a[old..], &b[new..], path, new, ops);
        }
        _ => ops.push(json!({"op":"set","path":path,"value":next})),
    }
}

fn span(a: &[Value], b: &[Value], path: &str, index: usize, ops: &mut Vec<Value>) {
    if a.len() == b.len() {
        for (offset, (old, new)) in a.iter().zip(b).enumerate() {
            diff(old, new, &child(path, &(index + offset).to_string()), ops);
        }
    } else {
        ops.push(json!({"op":"splice","path":path,"index":index,"deleteCount":a.len(),"items":b}));
    }
}

pub(crate) fn request(full: &Value, base: Option<&Value>, identity: &str) -> Value {
    let Some(base) = base else { return full.clone(); };
    if full["baseRevision"].as_i64().unwrap_or(0) <= 0 { return full.clone(); }
    let mut ops = vec![];
    diff(base, &full["bundle"], "", &mut ops);
    let delta = json!({"changeId":full["changeId"],"baseRevision":full["baseRevision"],
        identity:if identity=="entryId" { &full["bundle"]["entry"]["id"] } else { &full["bundle"]["session"]["id"] },
        "delta":{"protocol":PROTOCOL,"ops":ops}});
    if delta.to_string().len() < full.to_string().len() { delta } else { full.clone() }
}

impl Store {
    pub(crate) fn sync_upload_base(&self, scope: &str, id: &str, revision: i64, assessment: bool) -> Result<Option<Value>, AppError> {
        if revision <= 0 { return Ok(None); }
        Ok(self.sync_download_base(scope,id,assessment)?.filter(|(rev,_)| *rev==revision).map(|(_,base)|base))
    }
    pub(crate) fn sync_download_base(&self, scope: &str, id: &str, assessment: bool) -> Result<Option<(i64, Value)>, AppError> {
        let c = self.connection()?;
        let raw: Option<(i64, String)> = if assessment {
            c.query_row("SELECT remote_revision,base_json FROM assessment_sync_state WHERE account_key=? AND session_id=? AND remote_revision>0 AND base_json IS NOT NULL",
                params![scope,id], |r|Ok((r.get(0)?,r.get(1)?))).optional()?
        } else {
            c.query_row("SELECT remote_revision,base_json FROM sync_documents WHERE account_scope=? AND remote_id=? AND remote_revision>0 AND base_json IS NOT NULL",
                params![scope,id], |r|Ok((r.get(0)?,r.get(1)?))).optional()?
        };
        raw.map(|(revision,s)| {
            let mut v: Value = serde_json::from_str(&s)?;
            if assessment { v = v["bundle"].take(); }
            Ok((revision,v))
        }).transpose()
    }
}
