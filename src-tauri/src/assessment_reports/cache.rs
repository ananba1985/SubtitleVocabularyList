//! Rebuildable business fingerprints; immutable report JSON remains the source of truth.
use super::AssessmentReport;
use crate::{error::AppError, vocabulary::digest};
use rusqlite::{Connection, OptionalExtension, params, types::ValueRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct ReportCache {
    inputs: String,
    current: Option<i64>,
    reports: BTreeMap<i64, Content>,
}

#[derive(Serialize, Deserialize)]
struct Alias {
    item: String,
    entry: String,
    mapped: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Content {
    source: String,
    digest: String,
    aliases: Vec<Alias>,
}

fn key(id: &str) -> String {
    format!("assessment_report_business:v1:{id}")
}

fn alias(c: &Connection, item: &str, entry: &str) -> Result<Option<String>, AppError> {
    let mut q = c.prepare("SELECT DISTINCT local_id FROM sync_links WHERE entity_kind='entries' AND remote_id=? AND local_id IN (SELECT entry_id FROM assessment_collection_links WHERE session_item_id=?)")?;
    let aliases = q
        .query_map(params![entry, item], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((aliases.len() == 1).then(|| aliases[0].clone()))
}

impl Content {
    pub(super) fn build(c: &Connection, report: &AssessmentReport) -> Result<Self, AppError> {
        let _timing = crate::local_profile::phase("assessment.report_content_build");
        let mut content = report.clone();
        content.revision = 0;
        content.generated_at = 0;
        let mut aliases = Vec::new();
        for row in &mut content.items {
            if let Some(audio) = row.item.content.audio.as_mut() {
                audio.relative_path.clear();
                audio.asset_id = None;
            }
            for entry in &mut row.collected_entry_ids {
                let mapped = alias(c, &row.id, entry)?;
                aliases.push(Alias {
                    item: row.id.clone(),
                    entry: entry.clone(),
                    mapped: mapped.clone(),
                });
                if let Some(mapped) = mapped {
                    *entry = mapped;
                }
            }
            row.collected_entry_ids.sort();
            row.collected_entry_ids.dedup();
        }
        Ok(Self {
            source: String::new(),
            digest: digest(&serde_json::to_vec(&content)?),
            aliases,
        })
    }

    fn valid(&self, c: &Connection, raw: &str) -> Result<bool, AppError> {
        if self.source != digest(raw.as_bytes()) {
            return Ok(false);
        }
        for dep in &self.aliases {
            if alias(c, &dep.item, &dep.entry)? != dep.mapped {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl ReportCache {
    pub(super) fn load(c: &Connection, id: &str) -> Result<Self, AppError> {
        let raw: Option<String> = c
            .query_row(
                "SELECT value_json FROM settings WHERE key=?",
                [key(id)],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw
            .and_then(|r| serde_json::from_str(&r).ok())
            .unwrap_or_default())
    }

    // These are exactly the inputs used by report preparation. Network revisions,
    // update timestamps, playback progress and report insertion do not change business facts.
    pub(super) fn inputs(c: &Connection, id: &str) -> Result<String, AppError> {
        let _timing = crate::local_profile::phase("assessment.report_inputs");
        let mut hash = Sha256::new();
        for sql in [
            "SELECT state,created_at,config_json,sampling_json FROM assessment_sessions WHERE id=?",
            "SELECT id,position,snapshot_json FROM assessment_session_items WHERE session_id=? ORDER BY position,id",
            "SELECT id,session_item_id,revision,answer_json FROM assessment_answers WHERE session_id=? ORDER BY id",
            "SELECT c.id,c.answer_id,c.correction_json FROM assessment_corrections c JOIN assessment_answers a ON a.id=c.answer_id WHERE a.session_id=? ORDER BY c.id",
            "SELECT DISTINCT l.session_item_id,l.entry_id FROM assessment_collection_links l JOIN assessment_session_items i ON i.id=l.session_item_id WHERE i.session_id=? ORDER BY l.session_item_id,l.entry_id",
            "SELECT DISTINCT l.session_item_id,m.account_scope,m.local_id,m.remote_id FROM assessment_collection_links l JOIN assessment_session_items i ON i.id=l.session_item_id JOIN sync_links m ON m.entity_kind='entries' AND m.local_id=l.entry_id WHERE i.session_id=? ORDER BY l.session_item_id,m.account_scope,m.local_id,m.remote_id",
        ] {
            hash.update(sql.as_bytes());
            let mut q = c.prepare(sql)?;
            let columns = q.column_count();
            let mut rows = q.query([id])?;
            while let Some(row) = rows.next()? {
                hash.update([0xff]);
                for index in 0..columns {
                    match row.get_ref(index)? {
                        ValueRef::Text(value) | ValueRef::Blob(value) => {
                            hash.update((value.len() as u64).to_le_bytes());
                            hash.update(value);
                        }
                        ValueRef::Integer(value) => {
                            hash.update([0xfe]);
                            hash.update(value.to_le_bytes());
                        }
                        ValueRef::Real(value) => {
                            hash.update([0xfd]);
                            hash.update(value.to_le_bytes());
                        }
                        ValueRef::Null => hash.update([0xfc]),
                    }
                }
            }
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    pub(super) fn current(
        &self,
        c: &Connection,
        id: &str,
        inputs: &str,
    ) -> Result<Option<(i64, String)>, AppError> {
        if self.inputs != inputs {
            return Ok(None);
        }
        let Some(slot) = self.current else {
            return Ok(None);
        };
        let Some(content) = self.reports.get(&slot) else {
            return Ok(None);
        };
        let raw: Option<String> = c.query_row("SELECT report_json FROM assessment_report_versions WHERE session_id=? AND revision=?", params![id,slot], |r| r.get(0)).optional()?;
        if let Some(raw) = raw {
            if content.valid(c, &raw)? {
                let _timing = crate::local_profile::phase("assessment.report_inputs_reuse");
                return Ok(Some((slot, raw)));
            }
        }
        Ok(None)
    }

    pub(super) fn matching(
        &mut self,
        c: &Connection,
        slot: i64,
        raw: &str,
        target: &Content,
    ) -> Result<Option<AssessmentReport>, AppError> {
        if let Some(content) = self.reports.get(&slot) {
            if content.valid(c, raw)? {
                let _timing = crate::local_profile::phase("assessment.report_content_reuse");
                return if content.digest == target.digest {
                    Ok(Some(serde_json::from_str(raw)?))
                } else {
                    Ok(None)
                };
            }
        }
        let report: AssessmentReport = serde_json::from_str(raw)?;
        let mut content = Content::build(c, &report)?;
        content.source = digest(raw.as_bytes());
        let same = content.digest == target.digest;
        self.reports.insert(slot, content);
        Ok(same.then_some(report))
    }

    pub(super) fn publish(
        &mut self,
        c: &Connection,
        id: &str,
        inputs: String,
        slot: i64,
        new: Option<(&str, Content)>,
    ) -> Result<String, AppError> {
        if let Some((raw, mut content)) = new {
            content.source = digest(raw.as_bytes());
            self.reports.insert(slot, content);
        }
        // A conflict can replace the local projection; keep only extant slots plus the draft.
        let mut q =
            c.prepare("SELECT revision FROM assessment_report_versions WHERE session_id=?")?;
        let mut slots = q
            .query_map([id], |r| r.get::<_, i64>(0))?
            .collect::<Result<BTreeSet<_>, _>>()?;
        slots.insert(slot);
        self.reports.retain(|slot, _| slots.contains(slot));
        self.inputs = inputs;
        self.current = Some(slot);
        Ok(serde_json::to_string(self)?)
    }

    pub(super) fn persist(c: &Connection, id: &str, raw: &str) -> Result<(), AppError> {
        c.execute("INSERT INTO settings(key,value_json) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json WHERE value_json IS NOT excluded.value_json", params![key(id),raw])?;
        Ok(())
    }
}
