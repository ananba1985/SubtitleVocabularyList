//! Assessment synchronization coordination, persistence and frozen receipts.
mod merging;
mod packet;
mod projection;

pub(crate) use merging::merge;
use merging::{equal, preserved_copy};
pub use packet::{
    AnswerFacts, AssessmentDocument, AssessmentPacket, AssessmentPush, AssessmentSyncStatus,
    CollectionLink, ItemFacts, KeepAssessmentBoth, MediaReference, ReportFacts, SessionFacts,
};
use packet::{invalid, sha};
pub(crate) use projection::{PreparedPacket, export_on};
use projection::{cache_received_packet, export_with_base, load_base, write_settings};

use crate::{
    assessment::{
        AssessmentAnswer, AssessmentCorrection, FrozenItem, Interaction, SamplingSnapshot,
        load_session,
    },
    assessment_bank::AssessmentConfig,
    assessment_reports::{AssessmentReport, PreparedReport, prepare_report, report_on},
    error::AppError,
    store::Store,
    vocabulary::digest,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

fn canonical<T: Serialize>(v: &T) -> Result<String, AppError> {
    Ok(serde_json::to_string(&serde_json::to_value(v)?)?)
}
pub(crate) fn packet_digest(v: &AssessmentPacket) -> Result<String, AppError> {
    let _timing = crate::local_profile::phase("assessment.digest");
    Ok(digest(canonical(v)?.as_bytes()))
}
// Physical cache addresses are local; the semantic and original recording descriptors are frozen.
fn without_paths(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        if o.contains_key("relativePath") && o.contains_key("sha256") && o.contains_key("voiceId") {
            o.insert("relativePath".into(), json!(""));
            o.insert("assetId".into(), Value::Null);
        }
        for v in o.values_mut() {
            without_paths(v);
        }
    } else if let Some(a) = v.as_array_mut() {
        for v in a {
            without_paths(v);
        }
    }
}
fn portable<T: Serialize + for<'a> Deserialize<'a>>(value: &T) -> Result<T, AppError> {
    let mut v = serde_json::to_value(value)?;
    without_paths(&mut v);
    Ok(serde_json::from_value(v)?)
}
fn entry_alias(
    c: &Connection,
    scope: &str,
    id: &str,
    outbound: bool,
) -> Result<Option<String>, AppError> {
    let (select, filter) = if outbound {
        ("remote_id", "local_id")
    } else {
        ("local_id", "remote_id")
    };
    Ok(c.query_row(&format!("SELECT {select} FROM sync_links WHERE account_scope=? AND entity_kind='entries' AND {filter}=? ORDER BY {select} LIMIT 1"),params![scope,id],|r|r.get(0)).optional()?)
}
fn rewrite_entry_references(
    c: &Connection,
    scope: &str,
    v: &mut Value,
    outbound: bool,
) -> Result<(), AppError> {
    rewrite_references(v, &mut |id| entry_alias(c, scope, id, outbound))
}

fn rewrite_references(
    v: &mut Value,
    alias: &mut impl FnMut(&str) -> Result<Option<String>, AppError>,
) -> Result<(), AppError> {
    if let Some(o) = v.as_object_mut() {
        for (k, v) in o.iter_mut() {
            if matches!(k.as_str(), "entryId" | "entry_id") {
                if let Some(id) = v.as_str() {
                    if let Some(mapped) = alias(id)? {
                        *v = json!(mapped);
                    }
                }
            } else if k == "collectedEntryIds" {
                if let Some(a) = v.as_array_mut() {
                    for id in a {
                        if let Some(text) = id.as_str() {
                            if let Some(mapped) = alias(text)? {
                                *id = json!(mapped);
                            }
                        }
                    }
                }
            } else {
                rewrite_references(v, alias)?;
            }
        }
    } else if let Some(a) = v.as_array_mut() {
        for v in a {
            rewrite_references(v, alias)?;
        }
    }
    Ok(())
}
fn account_on(c: &Connection, scope: &str, capability: Option<&str>) -> Result<(), AppError> {
    c.execute("INSERT INTO assessment_sync_accounts(account_key,capability,updated_at) VALUES (?,?,?) ON CONFLICT(account_key) DO UPDATE SET capability=excluded.capability,updated_at=excluded.updated_at",params![scope,capability,chrono::Utc::now().timestamp_millis()])?;
    Ok(())
}
fn local_original_path(media: &MediaReference) -> String {
    format!("media/assessment/sync-{}.{}", media.digest, media.format)
}
fn report_slot_key(session: &str, checksum: &str) -> String {
    format!("assessment_report_slot:{session}:{checksum}")
}
fn save_report(c: &Connection, id: &str, report: &ReportFacts) -> Result<(), AppError> {
    let key = report_slot_key(id, &report.digest);
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [&key],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(());
    }
    let parsed: AssessmentReport = serde_json::from_str(&report.report_json)?;
    let mut slot = parsed.revision;
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM assessment_report_versions WHERE session_id=? AND revision=?)",
        params![id, slot],
        |r| r.get::<_, bool>(0),
    )? {
        slot=c.query_row("SELECT MIN(COALESCE(MIN(revision),0),0)-1 FROM assessment_report_versions WHERE session_id=?",[id],|r|r.get(0))?;
    }
    c.execute(
        "INSERT INTO assessment_report_versions VALUES (?,?,?,?)",
        params![id, slot, report.report_json, parsed.generated_at],
    )?;
    c.execute(
        "INSERT INTO settings(key,value_json) VALUES (?,?)",
        params![key, slot.to_string()],
    )?;
    Ok(())
}
// Callers validate incoming, merged or preserved packets before applying them.
fn apply_validated_on(
    c: &Connection,
    root: &std::path::Path,
    scope: &str,
    packet: &AssessmentPacket,
) -> Result<(), AppError> {
    let s = &packet.session;
    let prior: Option<i64> = c
        .query_row(
            "SELECT revision FROM assessment_sessions WHERE id=?",
            [&s.id],
            |r| r.get(0),
        )
        .optional()?;
    // Local report display slots are never overwritten by another device's fact counter.
    let revision = prior.map_or(s.revision, |r| r.max(s.revision) + 1);
    c.execute("INSERT INTO assessment_sessions VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET state=excluded.state,revision=excluded.revision,config_json=excluded.config_json,sampling_json=excluded.sampling_json,position=excluded.position,updated_at=excluded.updated_at",params![s.id,s.state,revision,canonical(&s.config)?,canonical(&s.sampling)?,s.position,s.created_at,s.updated_at])?;
    for row in &packet.items {
        let mut snapshot = row.snapshot.clone();
        if let Some(audio) = snapshot.content.audio.as_mut() {
            if audio.kind == "original" {
                let media = packet
                    .media
                    .iter()
                    .find(|m| m.digest == audio.sha256)
                    .ok_or_else(|| invalid("会话缺少原声清单。"))?;
                audio.relative_path = local_original_path(media);
                crate::corpus::validate_media_file(
                    root,
                    &audio.relative_path,
                    &audio.sha256,
                    "ready",
                )?;
            }
        }
        c.execute("INSERT INTO assessment_session_items VALUES (?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET snapshot_json=excluded.snapshot_json,interaction_json=excluded.interaction_json WHERE assessment_session_items.snapshot_json IS NOT excluded.snapshot_json OR assessment_session_items.interaction_json IS NOT excluded.interaction_json",params![row.id,s.id,row.position,canonical(&snapshot)?,row.interaction_json])?;
    }
    for row in &packet.answers {
        let a = &row.answer;
        c.execute("INSERT INTO assessment_answers VALUES (?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision WHERE assessment_answers.revision IS NOT excluded.revision",params![a.id,s.id,a.session_item_id,canonical(a)?,row.revision,a.created_at])?;
    }
    for row in &packet.corrections {
        c.execute(
            "INSERT OR IGNORE INTO assessment_corrections VALUES (?,?,?,?)",
            params![row.id, row.answer_id, canonical(row)?, row.created_at],
        )?;
    }
    for row in &packet.reports {
        save_report(c, &s.id, row)?;
    }
    for row in &packet.collection_links {
        let local = entry_alias(c, scope, &row.entry_id, false)?;
        let Some(local) = local else {
            continue;
        };
        if !c.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE id=?)",
            [&local],
            |r| r.get::<_, bool>(0),
        )? {
            continue;
        }
        let mut details = row.details.clone();
        rewrite_entry_references(c, scope, &mut details, false)?;
        c.execute(
            "INSERT OR IGNORE INTO assessment_collection_links VALUES (?,?,?,?)",
            params![
                row.session_item_id,
                row.operation_id,
                local,
                canonical(&details)?
            ],
        )?;
    }
    if matches!(s.state.as_str(), "completed" | "ended_early") {
        report_on(c, &s.id)?;
    }
    Ok(())
}
impl Store {
    pub fn assessment_sync_conflicts(&self, scope: &str) -> Result<Vec<Value>, AppError> {
        let c = self.connection()?;
        let mut q=c.prepare("SELECT session_id,conflict_json FROM assessment_sync_state WHERE account_key=? AND conflict_json IS NOT NULL ORDER BY session_id")?;
        q.query_map([scope],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.map(|r|{
            let(id,raw)=r?;let v:Value=serde_json::from_str(&raw)?;
            let side=|p:&Value|json!({"state":p["session"]["state"],"position":p["session"]["position"],"planned":p["session"]["config"]["questionCount"],"answers":p["answers"].as_array().map(Vec::len).unwrap_or(0),"corrections":p["corrections"].as_array().map(Vec::len).unwrap_or(0),"reports":p["reports"].as_array().map(Vec::len).unwrap_or(0)});
            let local=export_on(&c,self.root(),scope,&id)?;
            Ok(json!({"sessionId":id,"reason":v["reason"],"localRevision":local.session.revision,"remoteRevision":v["remote"]["revision"],"local":side(&serde_json::to_value(local)?),"remote":side(&v["remote"]["bundle"])}))
        }).collect()
    }
    pub fn assessment_sync_keep_both(
        &self,
        scope: &str,
        input: &KeepAssessmentBoth,
    ) -> Result<Value, AppError> {
        // Keep the existing original recording in its portable cache before the transaction.
        let packet = {
            let c = self.connection()?;
            export_on(&c, self.root(), scope, &input.session_id)?
        };
        for media in &packet.media {
            let bytes = self.assessment_sync_media(&media.digest, &media.format)?;
            self.assessment_save_remote_media(media, &bytes)?;
        }
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        let result = crate::assessment_bank::operation(
            &tx,
            &input.operation_id,
            "assessment_keep_both",
            input,
            |tx| {
                let raw:Option<String>=tx.query_row("SELECT conflict_json FROM assessment_sync_state WHERE account_key=? AND session_id=?",params![scope,input.session_id],|r|r.get(0)).optional()?.flatten();
                let raw =
                    raw.ok_or_else(|| AppError::new("conflict", "这次冲突已处理，请刷新页面。"))?;
                let conflict: Value = serde_json::from_str(&raw)?;
                let remote: AssessmentDocument =
                    serde_json::from_value(conflict["remote"].clone())?;
                let local = export_on(tx, self.root(), scope, &input.session_id)?;
                if local.session.revision != input.expected_local_revision
                    || remote.revision != input.expected_remote_revision
                {
                    return Err(AppError::new(
                        "conflict",
                        "测试记录已变化，请刷新后重新选择。",
                    ));
                }
                remote.bundle.validate()?;
                let archive = Uuid::new_v4().to_string();
                let archived = json!({"local":local,"remote":remote,"reason":conflict["reason"]});
                tx.execute(
                    "INSERT INTO assessment_operations VALUES (?,?,?,?,?)",
                    params![
                        archive,
                        "assessment_conflict_archive",
                        digest(canonical(&archived)?.as_bytes()),
                        canonical(&archived)?,
                        chrono::Utc::now().timestamp_millis()
                    ],
                )?;
                let copy = preserved_copy(&local, &archive)?;
                apply_validated_on(tx, self.root(), scope, &copy)?;
                let copy_base = AssessmentDocument {
                    session_id: copy.session.id.clone(),
                    revision: 0,
                    bundle: copy.clone(),
                };
                tx.execute("INSERT INTO assessment_sync_state(account_key,session_id,base_json) VALUES (?,?,?)",params![scope,copy.session.id,canonical(&copy_base)?])?;
                // Exact old packets are archived above; replace only this session's local projection.
                tx.execute("DELETE FROM assessment_collection_links WHERE session_item_id IN (SELECT id FROM assessment_session_items WHERE session_id=?)",[&input.session_id])?;
                tx.execute("DELETE FROM assessment_corrections WHERE answer_id IN (SELECT id FROM assessment_answers WHERE session_id=?)",[&input.session_id])?;
                tx.execute(
                    "DELETE FROM assessment_answers WHERE session_id=?",
                    [&input.session_id],
                )?;
                tx.execute(
                    "DELETE FROM assessment_session_items WHERE session_id=?",
                    [&input.session_id],
                )?;
                tx.execute(
                    "DELETE FROM assessment_report_versions WHERE session_id=?",
                    [&input.session_id],
                )?;
                let prefix = format!("assessment_report_slot:{}:", input.session_id);
                tx.execute(
                    "DELETE FROM settings WHERE substr(key,1,?1)=?2",
                    params![prefix.len(), prefix],
                )?;
                apply_validated_on(tx, self.root(), scope, &remote.bundle)?;
                tx.execute("UPDATE assessment_sync_state SET base_json=?,pending_json=NULL,pending_change_id=NULL,pending_digest=NULL,conflict_json=NULL WHERE account_key=? AND session_id=?",params![canonical(&remote)?,scope,input.session_id])?;
                let acknowledged =
                    packet_digest(&export_on(tx, self.root(), scope, &input.session_id)?)?;
                tx.execute("UPDATE assessment_sync_state SET acknowledged_digest=? WHERE account_key=? AND session_id=?",params![acknowledged,scope,input.session_id])?;
                Ok(
                    json!({"sessionId":input.session_id,"copySessionId":copy.session.id,"archiveId":archive}),
                )
            },
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn assessment_sync_media(&self, hash: &str, format: &str) -> Result<Vec<u8>, AppError> {
        if !sha(hash) {
            return Err(invalid("测试原声摘要无效。"));
        }
        let c = self.connection()?;
        let path:Option<String>=c.query_row("SELECT json_extract(snapshot_json,'$.content.audio.relativePath') FROM assessment_session_items WHERE json_extract(snapshot_json,'$.content.audio.sha256')=? AND json_extract(snapshot_json,'$.content.audio.kind')='original' LIMIT 1",[hash],|r|r.get(0)).optional()?;
        let relative = path.unwrap_or_else(|| {
            local_original_path(&MediaReference {
                digest: hash.into(),
                format: format.into(),
            })
        });
        crate::corpus::validate_media_file(self.root(), &relative, hash, "ready")?;
        let bytes = std::fs::read(self.root().join(relative))?;
        if bytes.len() > 20 * 1024 * 1024 {
            return Err(AppError::new(
                "capacity_exceeded",
                "测试原声超过单文件同步上限，原资料保留。",
            ));
        }
        Ok(bytes)
    }
    pub fn assessment_save_remote_media(
        &self,
        media: &MediaReference,
        bytes: &[u8],
    ) -> Result<(), AppError> {
        if !sha(&media.digest) || bytes.len() > 20 * 1024 * 1024 || digest(bytes) != media.digest {
            return Err(invalid("测试原声内容与清单不一致。"));
        }
        let relative = local_original_path(media);
        let target = self.root().join(&relative);
        std::fs::create_dir_all(target.parent().unwrap())?;
        if target.is_file() && digest(&std::fs::read(&target)?) == media.digest {
            return Ok(());
        }
        let temporary = self
            .root()
            .join("jobs")
            .join(format!("assessment-media-{}", Uuid::new_v4()));
        std::fs::write(&temporary, bytes)?;
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(temporary, target)?;
        crate::corpus::validate_media_file(self.root(), &relative, &media.digest, "ready")?;
        Ok(())
    }
    pub fn assessment_sync_capability(
        &self,
        scope: &str,
        capability: Option<&str>,
    ) -> Result<(), AppError> {
        let c = self.connection()?;
        account_on(&c, scope, capability)
    }
    pub fn assessment_sync_status(&self, scope: &str) -> Result<AssessmentSyncStatus, AppError> {
        let _operation = crate::local_profile::operation("assessment.status");
        let ids = {
            let c = self.connection()?;
            let mut q = c.prepare("SELECT d.session_id FROM assessment_sync_dirty d LEFT JOIN assessment_sync_state s ON s.account_key=? AND s.session_id=d.session_id WHERE s.cached_digest IS NULL OR s.cached_epoch<>d.epoch ORDER BY d.session_id")?;
            q.query_map([scope], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        // Refresh only changed sessions; release the business connection between complete packets.
        for id in ids {
            self.prepared_assessment(scope, &id)?;
        }
        let c = self.connection()?;
        let row: Option<(Option<String>, Option<String>)> = c
            .query_row(
                "SELECT capability,cursor FROM assessment_sync_accounts WHERE account_key=?",
                [scope],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(AssessmentSyncStatus {
            capability: row.as_ref().and_then(|r| r.0.clone()),
            cursor: row.and_then(|r| r.1).and_then(|v| v.parse().ok()).unwrap_or(0),
            pending: c.query_row("SELECT COUNT(*) FROM assessment_sync_state WHERE account_key=? AND cached_digest IS NOT acknowledged_digest", [scope], |r| r.get(0))?,
            conflicts: c.query_row("SELECT COUNT(*) FROM assessment_sync_state WHERE account_key=? AND conflict_json IS NOT NULL", [scope], |r| r.get(0))?,
            pending_references: c.query_row("SELECT COALESCE(SUM(cached_pending_references),0) FROM assessment_sync_state WHERE account_key=?", [scope], |r| r.get(0))?,
        })
    }
    pub fn assessment_push_candidates(&self, scope: &str) -> Result<Vec<String>, AppError> {
        let c = self.connection()?;
        let mut q = c.prepare("SELECT d.session_id FROM assessment_sync_dirty d JOIN assessment_sessions a ON a.id=d.session_id LEFT JOIN assessment_sync_state s ON s.account_key=? AND s.session_id=d.session_id WHERE s.conflict_json IS NULL AND (s.pending_json IS NOT NULL OR s.cached_digest IS NULL OR s.cached_epoch<>d.epoch OR s.cached_digest IS NOT s.acknowledged_digest) ORDER BY a.created_at,a.id")?;
        Ok(q.query_map([scope], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn assessment_freeze_push(
        &self,
        scope: &str,
        id: &str,
    ) -> Result<Option<AssessmentPush>, AppError> {
        let _operation = crate::local_profile::operation("assessment.freeze");
        loop {
            let saved: Option<(i64, Option<String>, Option<String>, bool)> = {
                let c = self.connection()?;
                if c.query_row("SELECT EXISTS(SELECT 1 FROM assessment_sync_state s JOIN assessment_sync_dirty d ON d.session_id=s.session_id WHERE s.account_key=? AND s.session_id=? AND s.pending_json IS NULL AND s.conflict_json IS NULL AND s.cached_epoch=d.epoch AND s.cached_digest=s.acknowledged_digest)",params![scope,id],|r|r.get::<_,bool>(0))? {return Ok(None);}
                c.query_row(
                    "SELECT remote_revision,pending_json,acknowledged_digest,conflict_json IS NOT NULL FROM assessment_sync_state WHERE account_key=? AND session_id=?",
                    params![scope,id],
                    |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
                ).optional()?
            };
            if saved.as_ref().is_some_and(|r| r.3) {
                return Ok(None);
            }
            if let Some(pending) = saved.as_ref().and_then(|r| r.1.as_ref()) {
                return Ok(Some(serde_json::from_str(pending)?));
            }
            let prepared = self.prepared_assessment(scope, id)?;
            let acknowledged = saved.as_ref().and_then(|r| r.2.as_ref()) == Some(&prepared.hash);
            let _encoding = crate::local_profile::phase("assessment.freeze_encode");
            let push = (!acknowledged).then(|| AssessmentPush {
                change_id: Uuid::new_v4().to_string(),
                base_revision: saved.as_ref().map_or(0, |r| r.0),
                bundle: prepared.packet.clone(),
            });
            let payload = push.as_ref().map(canonical).transpose()?;
            drop(_encoding);
            if payload.as_ref().is_some_and(|v| v.len() > 8 * 1024 * 1024) {
                return Err(AppError::new(
                    "capacity_exceeded",
                    "完整水平测试资料超过单包能力，资料保持完整，未截断历史。",
                ));
            }
            let mut c = self.connection()?;
            let _transaction = crate::local_profile::phase("assessment.freeze_tx");
            let tx = c.transaction()?;
            let epoch: i64 = tx.query_row(
                "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                [id],
                |r| r.get(0),
            )?;
            let current:Option<(i64,Option<String>,Option<String>,bool)>=tx.query_row("SELECT remote_revision,pending_json,acknowledged_digest,conflict_json IS NOT NULL FROM assessment_sync_state WHERE account_key=? AND session_id=?",params![scope,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
            // A first snapshot publishes its empty sync-state row before reaching this commit.
            let expected = saved.unwrap_or((0, None, None, false));
            if epoch != prepared.epoch || current.as_ref() != Some(&expected) {
                continue;
            }
            if acknowledged {
                tx.commit()?;
                return Ok(None);
            }
            let push = push.unwrap();
            tx.execute("UPDATE assessment_sync_state SET pending_json=?,pending_change_id=?,pending_digest=? WHERE account_key=? AND session_id=?",params![payload,push.change_id,prepared.hash,scope,id])?;
            let _commit = crate::local_profile::phase("assessment.write_commit");
            tx.commit()?;
            return Ok(Some(push));
        }
    }
    pub fn assessment_freeze_pushes(&self, scope: &str) -> Result<Vec<AssessmentPush>, AppError> {
        let mut pushes = Vec::new();
        for id in self.assessment_push_candidates(scope)? {
            if let Some(push) = self.assessment_freeze_push(scope, &id)? {
                pushes.push(push);
            }
        }
        Ok(pushes)
    }
    pub fn assessment_revision_known(
        &self,
        scope: &str,
        id: &str,
        revision: i64,
    ) -> Result<bool, AppError> {
        Ok(self.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM assessment_sync_state WHERE account_key=? AND session_id=? AND remote_revision>=?)",params![scope,id,revision],|r|r.get(0))?)
    }
    pub fn assessment_sync_saved(
        &self,
        scope: &str,
        push: &AssessmentPush,
        revision: i64,
    ) -> Result<(), AppError> {
        if revision != push.base_revision + 1 {
            return Err(invalid("测试同步回执版本无效。"));
        }
        let _operation = crate::local_profile::operation("assessment.saved");
        let doc = AssessmentDocument {
            session_id: push.bundle.session.id.clone(),
            revision,
            bundle: push.bundle.clone(),
        };
        let _encoding = crate::local_profile::phase("assessment.saved_encode");
        let payload = canonical(&doc)?;
        drop(_encoding);
        let mut c = self.connection()?;
        let tx = c.transaction()?;
        // Only advance an exact frozen projection. New local facts must remain dirty.
        let reusable: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM assessment_sync_state s JOIN assessment_sync_dirty d ON d.session_id=s.session_id WHERE s.account_key=? AND s.session_id=? AND s.pending_change_id=? AND s.cached_epoch=d.epoch AND s.cached_digest=s.pending_digest)",params![scope,push.bundle.session.id,push.change_id],|r|r.get(0))?;
        tx.execute("UPDATE assessment_sync_state SET base_json=?,acknowledged_digest=pending_digest,pending_json=NULL,pending_change_id=NULL,pending_digest=NULL WHERE account_key=? AND session_id=? AND pending_change_id=?",params![payload,scope,push.bundle.session.id,push.change_id])?;
        if reusable {
            tx.execute("UPDATE assessment_sync_state SET cached_epoch=(SELECT epoch FROM assessment_sync_dirty WHERE session_id=?2) WHERE account_key=?1 AND session_id=?2",params![scope,push.bundle.session.id])?;
        }
        let _commit = crate::local_profile::phase("assessment.write_commit");
        tx.commit()?;
        Ok(())
    }
    pub fn assessment_sync_receive(
        &self,
        scope: &str,
        remote: &AssessmentDocument,
    ) -> Result<bool, AppError> {
        let _operation = crate::local_profile::operation("assessment.receive");
        remote.bundle.validate()?;
        if remote.session_id != remote.bundle.session.id || remote.revision < 1 {
            return Err(invalid("测试变更关联无效。"));
        }
        let _encoding = crate::local_profile::phase("assessment.receive_encode");
        let remote_json = canonical(remote)?;
        drop(_encoding);
        let previous = self.previous_assessment(scope, &remote.session_id);
        let mut c = self.connection()?;
        let _transaction = crate::local_profile::phase("assessment.receive_tx");
        let tx = c.transaction()?;
        let saved:Option<(Option<String>,i64,bool)>=tx.query_row("SELECT s.acknowledged_digest,s.remote_revision,COALESCE(s.cached_epoch=d.epoch AND s.cached_digest=s.acknowledged_digest,0) FROM assessment_sync_state s JOIN assessment_sync_dirty d ON d.session_id=s.session_id WHERE s.account_key=? AND s.session_id=?",params![scope,remote.session_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if saved.as_ref().is_some_and(|s| s.1 >= remote.revision) {
            return Ok(true);
        }
        let unchanged = saved.as_ref().is_some_and(|s| s.2);
        let base = if unchanged {
            None
        } else {
            load_base(&tx, scope, &remote.session_id)?
        };
        let exists = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM assessment_sessions WHERE id=?)",
            [&remote.session_id],
            |r| r.get::<_, bool>(0),
        )?;
        let local = if exists && !unchanged {
            let mut settings = Vec::new();
            let (local, _) = export_with_base(
                &tx,
                scope,
                &remote.session_id,
                base.as_ref(),
                None,
                previous.as_deref(),
                &mut settings,
            )?;
            write_settings(&tx, &settings)?;
            Some(local)
        } else {
            None
        };
        let local_hash = local.as_ref().map(packet_digest).transpose()?;
        let apply = if let Some(local) = &local {
            if saved.as_ref().and_then(|r| r.0.as_ref()) == local_hash.as_ref() {
                Ok(remote.bundle.clone())
            } else if let Some(base) = &base {
                merge(&base.bundle, local, &remote.bundle)
            } else if equal(local, &remote.bundle)? {
                Ok(remote.bundle.clone())
            } else {
                Err(AppError::new(
                    "assessment_conflict",
                    "同一会话已有本地修改，缺少共同基准。",
                ))
            }
        } else {
            Ok(remote.bundle.clone())
        };
        match apply {
            Ok(packet) => {
                apply_validated_on(&tx, self.root(), scope, &packet)?;
                tx.execute("INSERT INTO assessment_sync_state(account_key,session_id,base_json) VALUES (?,?,?) ON CONFLICT(account_key,session_id) DO UPDATE SET base_json=excluded.base_json,pending_json=NULL,pending_change_id=NULL,pending_digest=NULL,conflict_json=NULL",params![scope,remote.session_id,remote_json])?;
                let prepared = cache_received_packet(&tx, scope, remote, previous.as_deref())?;
                // A merged local append must still be pushed; a plain receive is acknowledged locally.
                let merged = local.is_some()
                    && saved.as_ref().and_then(|r| r.0.as_ref()) != local_hash.as_ref()
                    && base.is_some();
                tx.execute("UPDATE assessment_sync_state SET acknowledged_digest=? WHERE account_key=? AND session_id=?",params![if merged{None}else{Some(&prepared.hash)},scope,remote.session_id])?;
                let _commit = crate::local_profile::phase("assessment.write_commit");
                tx.commit()?;
                if let Ok(mut cached) = self.assessment_packet.lock() {
                    *cached = Some(Arc::new(prepared));
                }
                Ok(true)
            }
            Err(error) => {
                tx.execute("INSERT INTO assessment_sync_state(account_key,session_id,conflict_json) VALUES (?,?,?) ON CONFLICT(account_key,session_id) DO UPDATE SET conflict_json=excluded.conflict_json",params![scope,remote.session_id,canonical(&json!({"local":local,"remote":remote,"reason":error.message}))?])?;
                let _commit = crate::local_profile::phase("assessment.write_commit");
                tx.commit()?;
                Ok(false)
            }
        }
    }
    pub fn assessment_sync_cursor(&self, scope: &str, cursor: i64) -> Result<(), AppError> {
        self.connection()?.execute(
            "UPDATE assessment_sync_accounts SET cursor=?,updated_at=? WHERE account_key=?",
            params![
                cursor.to_string(),
                chrono::Utc::now().timestamp_millis(),
                scope
            ],
        )?;
        Ok(())
    }
}

#[cfg(all(windows, feature = "desktop"))]
impl crate::application::Application {
    pub(crate) fn prepare_assessment_audio_cached(
        &self,
        session: &str,
        item: &str,
        context: &crate::tasks::TaskContext,
    ) -> Result<std::path::PathBuf, AppError> {
        let content = {
            let c = self.store.connection()?;
            let s = load_session(&c, session)?;
            let (position, frozen, _) = crate::assessment::load_item(&c, item, session)?;
            if !(matches!(s.state.as_str(), "completed" | "ended_early")
                || position < s.position
                || (s.state == "active" && position == s.position))
            {
                return Err(invalid("请先继续当前测试，或选择已作答的题目。"));
            }
            frozen.content
        };
        let audio = content.audio.as_ref().ok_or_else(|| {
            AppError::new("assessment_audio_unavailable", "此题没有预先准备的音频。")
        })?;
        if let Ok(path) = crate::assessment::validated_audio(&self.store, audio) {
            return Ok(path);
        }
        if audio.kind != "system" {
            return Err(AppError::new(
                "resource_missing",
                "对应原声尚未同步到本机，请先完成同步。",
            ));
        }
        let source_hash = audio.sha256.clone();
        let mut batch = vec![("listening".to_owned(), content)];
        self.assessment_prepare_audio(&mut batch, context)?;
        let restored = batch[0].1.audio.as_ref().unwrap();
        self.store.connection()?.execute("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",params![format!("assessment_audio_cache:{source_hash}"),canonical(restored)?])?;
        crate::assessment::validated_audio(&self.store, restored)
    }
    pub(crate) fn prepare_assessment_audio_cached_from_draft(
        &self,
        draft: &str,
        item: &str,
        context: &crate::tasks::TaskContext,
    ) -> Result<Option<std::path::PathBuf>, AppError> {
        let session = {
            let c = self.store.connection()?;
            let raw:String=c.query_row("SELECT result_json FROM assessment_operations WHERE operation_id=? AND kind='assessment_collection_prepare'",[draft],|r|r.get(0))?;
            let draft: crate::assessment_reports::AssessmentCollectionDraft =
                serde_json::from_str(&raw)?;
            let candidate = draft
                .items
                .iter()
                .find(|i| i.session_item_id == item)
                .ok_or_else(|| invalid("请选择已预览的测试题目。"))?;
            if candidate.audio_kind.is_none() {
                return Ok(None);
            }
            draft.session_id
        };
        self.prepare_assessment_audio_cached(&session, item, context)
            .map(Some)
    }
    pub fn assessment_audio_prepare(
        self: &std::sync::Arc<Self>,
        session: String,
        item: String,
        operation: String,
    ) -> Result<crate::tasks::TaskSnapshot, AppError> {
        let app = std::sync::Arc::clone(self);
        let hash = digest(format!("{session}:{item}").as_bytes());
        self.tasks
            .start("speech", &operation, &hash, move |context| {
                context.subject("水平测试英语录音");
                context.check_cancelled()?;
                let path = app.prepare_assessment_audio_cached(&session, &item, &context)?;
                Ok(json!({"path":path}))
            })
    }
}
