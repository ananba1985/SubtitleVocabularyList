//! Transport-only changes against an acknowledged complete bundle.
use crate::{error::AppError, store::Store};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) const PROTOCOL: &str = "json-delta-1";

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
        let c = self.connection()?;
        let raw: Option<String> = if assessment {
            c.query_row("SELECT base_json FROM assessment_sync_state WHERE account_key=? AND session_id=? AND remote_revision=?",
                params![scope,id,revision], |r|r.get(0)).optional()?.flatten()
        } else {
            c.query_row("SELECT base_json FROM sync_documents WHERE account_scope=? AND remote_id=? AND remote_revision=?",
                params![scope,id,revision], |r|r.get(0)).optional()?.flatten()
        };
        raw.map(|s| {
            let mut v: Value = serde_json::from_str(&s)?;
            if assessment { v = v["bundle"].take(); }
            Ok(v)
        }).transpose()
    }
}
