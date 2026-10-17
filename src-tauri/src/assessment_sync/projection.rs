//! Complete local projections, immutable part reuse and epoch-checked publication.
use super::*;

pub(crate) struct PreparedPacket {
    pub(super) scope: String,
    pub(super) id: String,
    pub(super) epoch: i64,
    pub(super) packet: AssessmentPacket,
    pub(super) hash: String,
    inputs: ProjectionInputs,
}
#[derive(Default)]
pub(super) struct ProjectionInputs {
    items: Vec<ItemInput>,
    reports: BTreeMap<i64, ReportInput>,
}
struct ItemInput {
    digest: String,
    media: Option<MediaReference>,
}
#[derive(Clone)]
struct ReportInput {
    digest: String,
    projected_digest: String,
    aliases: BTreeMap<String, Option<String>>,
}
pub(crate) fn export_on(
    c: &Connection,
    _root: &std::path::Path,
    scope: &str,
    id: &str,
) -> Result<AssessmentPacket, AppError> {
    let base = load_base(c, scope, id)?;
    let mut settings = Vec::new();
    let (packet, _) = export_with_base(c, scope, id, base.as_ref(), None, None, &mut settings)?;
    write_settings(c, &settings)?;
    Ok(packet)
}

pub(super) fn load_base(
    c: &Connection,
    scope: &str,
    id: &str,
) -> Result<Option<AssessmentDocument>, AppError> {
    let _timing = crate::local_profile::phase("assessment.base_decode");
    let raw: Option<String> = c
        .query_row(
            "SELECT base_json FROM assessment_sync_state WHERE account_key=? AND session_id=?",
            params![scope, id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(raw
        .as_deref()
        .map(serde_json::from_str::<AssessmentDocument>)
        .transpose()?)
}

pub(super) fn write_settings(
    c: &Connection,
    settings: &[(String, String)],
) -> Result<(), AppError> {
    let mut statement = c.prepare("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json WHERE settings.value_json IS NOT excluded.value_json")?;
    for (key, value) in settings {
        statement.execute(params![key, value])?;
    }
    Ok(())
}

pub(super) fn export_with_base(
    c: &Connection,
    scope: &str,
    id: &str,
    base: Option<&AssessmentDocument>,
    prepared_report: Option<&PreparedReport>,
    previous: Option<&PreparedPacket>,
    settings: &mut Vec<(String, String)>,
) -> Result<(AssessmentPacket, ProjectionInputs), AppError> {
    let _timing = crate::local_profile::phase("assessment.export");
    let previous = previous.filter(|p| p.scope == scope && p.id == id);
    let old_reports = previous
        .map(|p| {
            p.packet
                .reports
                .iter()
                .map(|r| (r.digest.as_str(), r))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut inputs = ProjectionInputs::default();
    let mut reused_items = HashSet::new();
    let s = load_session(c, id)?;
    let origin = if let Some(base) = base {
        base.bundle.origin_device_id.clone()
    } else {
        let device: Option<String> = c
            .query_row(
                "SELECT value_json FROM settings WHERE key='assessment_device_v1'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let device = if let Some(device) = device {
            serde_json::from_str::<String>(&device)?
        } else {
            let v = Uuid::new_v4().to_string();
            settings.push(("assessment_device_v1".into(), canonical(&v)?));
            v
        };
        c.query_row(
            "SELECT value_json FROM settings WHERE key=?",
            [format!("assessment_origin:{}", s.id)],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .map(|v| serde_json::from_str::<String>(&v))
        .transpose()?
        .unwrap_or(device)
    };
    let mut q=c.prepare("SELECT id,position,snapshot_json,interaction_json FROM assessment_session_items WHERE session_id=? ORDER BY position")?;
    let rows = q
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, usize>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut items = Vec::new();
    let mut media = std::collections::BTreeSet::new();
    for (id, position, snapshot, interaction) in rows {
        let source_digest = digest(snapshot.as_bytes());
        let cached = previous
            .and_then(|p| {
                p.packet
                    .items
                    .get(position)
                    .zip(p.inputs.items.get(position))
            })
            .filter(|(item, input)| {
                item.id == id && item.position == position && input.digest == source_digest
            });
        let (snapshot, reference) = if let Some((item, input)) = cached {
            let _timing = crate::local_profile::phase("assessment.item_reuse");
            reused_items.insert(position);
            (item.snapshot.clone(), input.media.clone())
        } else {
            let _timing = crate::local_profile::phase("assessment.item_project");
            let snapshot: FrozenItem = serde_json::from_str(&snapshot)?;
            let reference = snapshot
                .content
                .audio
                .as_ref()
                .filter(|a| a.kind == "original")
                .map(|audio| MediaReference {
                    digest: audio.sha256.clone(),
                    format: std::path::Path::new(&audio.relative_path)
                        .extension()
                        .and_then(|v| v.to_str())
                        .unwrap_or("wav")
                        .to_owned(),
                });
            (portable(&snapshot)?, reference)
        };
        inputs.items.push(ItemInput {
            digest: source_digest,
            media: reference.clone(),
        });
        if let Some(reference) = reference {
            media.insert(reference);
        }
        items.push(ItemFacts {
            id,
            position,
            snapshot,
            interaction_json: interaction,
        });
    }
    let mut q = c.prepare(
        "SELECT answer_json,revision FROM assessment_answers WHERE session_id=? ORDER BY id",
    )?;
    let answers = q
        .query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .map(|r| {
            let (t, revision) = r?;
            Ok(AnswerFacts {
                answer: serde_json::from_str(&t)?,
                revision,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let mut q=c.prepare("SELECT correction_json FROM assessment_corrections WHERE answer_id IN (SELECT id FROM assessment_answers WHERE session_id=?) ORDER BY id")?;
    let corrections = q
        .query_map([id], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str::<AssessmentCorrection>(&r?)?))
        .collect::<Result<Vec<_>, AppError>>()?;
    let mut q=c.prepare("SELECT report_json,revision FROM assessment_report_versions WHERE session_id=? ORDER BY created_at,revision")?;
    let mut reports = BTreeMap::new();
    let mut retained_by_slot = BTreeMap::new();
    if let Some(base) = &base {
        for report in &base.bundle.reports {
            let slot: Option<String> = c
                .query_row(
                    "SELECT value_json FROM settings WHERE key=?",
                    [report_slot_key(id, &report.digest)],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(slot) = slot {
                retained_by_slot.entry(slot).or_insert(report);
            }
        }
    }
    let new_report = prepared_report.and_then(|report| {
        report
            .new_json
            .as_ref()
            .map(|raw| Ok((raw.clone(), report.report.revision)))
    });
    // Include the pending immutable report in the complete projection without writing it yet.
    for row in q
        .query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .chain(new_report)
    {
        let (raw, slot) = row?;
        if let Some(report) = retained_by_slot.get(&slot.to_string()) {
            let _timing = crate::local_profile::phase("assessment.report_reuse");
            reports.insert(report.digest.clone(), (*report).clone());
            continue;
        }
        let source_digest = digest(raw.as_bytes());
        if let Some(input) = previous.and_then(|p| p.inputs.reports.get(&slot)) {
            let mut unchanged = input.digest == source_digest;
            if unchanged {
                for (entry, mapped) in &input.aliases {
                    if entry_alias(c, scope, entry, true)? != *mapped {
                        unchanged = false;
                        break;
                    }
                }
            }
            if unchanged {
                if let Some(report) = old_reports.get(input.projected_digest.as_str()) {
                    let _timing = crate::local_profile::phase("assessment.report_reuse");
                    settings.push((report_slot_key(id, &report.digest), slot.to_string()));
                    reports.insert(report.digest.clone(), (*report).clone());
                    inputs.reports.insert(slot, input.clone());
                    continue;
                }
            }
        }
        let _timing = crate::local_profile::phase("assessment.report_project");
        let report: AssessmentReport = portable(&serde_json::from_str::<AssessmentReport>(&raw)?)?;
        let mut v = serde_json::to_value(report)?;
        let mut aliases: BTreeMap<String, Option<String>> = BTreeMap::new();
        rewrite_references(&mut v, &mut |entry| {
            if let Some(mapped) = aliases.get(entry) {
                return Ok(mapped.clone());
            }
            let mapped = entry_alias(c, scope, entry, true)?;
            aliases.insert(entry.to_owned(), mapped.clone());
            Ok(mapped)
        })?;
        let report: AssessmentReport = serde_json::from_value(v)?;
        let report_json = canonical(&report)?;
        let checksum = digest(report_json.as_bytes());
        inputs.reports.insert(
            slot,
            ReportInput {
                digest: source_digest,
                projected_digest: checksum.clone(),
                aliases,
            },
        );
        settings.push((report_slot_key(id, &checksum), slot.to_string()));
        reports.insert(
            checksum.clone(),
            ReportFacts {
                digest: checksum,
                report_json,
            },
        );
    }
    let mut q=c.prepare("SELECT session_item_id,operation_id,entry_id,link_json FROM assessment_collection_links WHERE session_item_id IN (SELECT id FROM assessment_session_items WHERE session_id=?) ORDER BY session_item_id,operation_id")?;
    let mut links = BTreeMap::new();
    for row in q.query_map([id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (item, operation, entry, details) = row?;
        let mut details: Value = serde_json::from_str(&details)?;
        rewrite_entry_references(c, scope, &mut details, true)?;
        links.insert(
            (item.clone(), operation.clone()),
            CollectionLink {
                session_item_id: item,
                operation_id: operation,
                entry_id: entry_alias(c, scope, &entry, true)?.unwrap_or(entry),
                details,
            },
        );
    }
    if let Some(base) = &base {
        for report in &base.bundle.reports {
            reports
                .entry(report.digest.clone())
                .or_insert_with(|| report.clone());
        }
        for link in &base.bundle.collection_links {
            links
                .entry((link.session_item_id.clone(), link.operation_id.clone()))
                .or_insert_with(|| link.clone());
        }
    }
    let packet = AssessmentPacket {
        protocol: "assessment-1".into(),
        schema_version: 1,
        origin_device_id: origin,
        session: SessionFacts {
            id: s.id,
            state: s.state,
            revision: s.revision,
            config: s.config,
            sampling: s.sampling,
            position: s.position,
            created_at: s.created_at,
            updated_at: s.updated_at,
        },
        items,
        answers,
        corrections,
        reports: reports.into_values().collect(),
        collection_links: links.into_values().collect(),
        media: media.into_iter().collect(),
    };
    packet.validate_projection(previous, &reused_items)?;
    Ok((packet, inputs))
}

// Cached digests describe complete exported facts, never a truncated history or an acknowledgement.
pub(super) fn cache_received_packet(
    c: &Connection,
    scope: &str,
    remote: &AssessmentDocument,
    previous: Option<&PreparedPacket>,
) -> Result<PreparedPacket, AppError> {
    let id = &remote.session_id;
    c.execute(
        "INSERT OR IGNORE INTO assessment_sync_accounts(account_key,updated_at) VALUES (?,?)",
        params![scope, chrono::Utc::now().timestamp_millis()],
    )?;
    let mut settings = Vec::new();
    let (packet, inputs) =
        export_with_base(c, scope, id, Some(remote), None, previous, &mut settings)?;
    write_settings(c, &settings)?;
    let hash = packet_digest(&packet)?;
    let mut references = 0usize;
    for link in &packet.collection_links {
        if entry_alias(c, scope, &link.entry_id, false)?.is_none() {
            references += 1;
        }
    }
    let epoch: i64 = c.query_row(
        "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
        [id],
        |r| r.get(0),
    )?;
    c.execute("INSERT INTO assessment_sync_state(account_key,session_id,cached_epoch,cached_digest,cached_pending_references) VALUES (?,?,?,?,?) ON CONFLICT(account_key,session_id) DO UPDATE SET cached_epoch=excluded.cached_epoch,cached_digest=excluded.cached_digest,cached_pending_references=excluded.cached_pending_references",params![scope,id,epoch,hash,references])?;
    Ok(PreparedPacket {
        scope: scope.into(),
        id: id.clone(),
        epoch,
        packet,
        hash,
        inputs,
    })
}

impl Store {
    pub(super) fn previous_assessment(&self, scope: &str, id: &str) -> Option<Arc<PreparedPacket>> {
        self.assessment_packet.lock().ok().and_then(|cached| {
            cached
                .as_ref()
                .filter(|p| p.scope == scope && p.id == id)
                .cloned()
        })
    }

    pub(super) fn prepared_assessment(
        &self,
        scope: &str,
        id: &str,
    ) -> Result<Arc<PreparedPacket>, AppError> {
        loop {
            let (packet, inputs, epoch, references, settings, report) = {
                let mut c = self.connection()?;
                let epoch: i64 = c.query_row(
                    "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                    [id],
                    |r| r.get(0),
                )?;
                let previous = self.previous_assessment(scope, id);
                if let Some(cached) = previous.as_ref().filter(|p| p.epoch == epoch) {
                    return Ok(Arc::clone(cached));
                }
                let _transaction = crate::local_profile::phase("assessment.read_tx");
                let changes = c.total_changes();
                let tx = c.transaction()?;
                let report = if matches!(
                    load_session(&tx, id)?.state.as_str(),
                    "completed" | "ended_early"
                ) {
                    Some(prepare_report(&tx, id)?)
                } else {
                    None
                };
                let base = load_base(&tx, scope, id)?;
                let mut settings = Vec::new();
                let (packet, inputs) = export_with_base(
                    &tx,
                    scope,
                    id,
                    base.as_ref(),
                    report.as_ref(),
                    previous.as_deref(),
                    &mut settings,
                )?;
                let epoch: i64 = tx.query_row(
                    "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                    [id],
                    |r| r.get(0),
                )?;
                let mut references = 0;
                for link in &packet.collection_links {
                    if entry_alias(&tx, scope, &link.entry_id, false)?.is_none() {
                        references += 1;
                    }
                }
                // Report preparation and export leave business rows untouched.
                let _commit = (tx.total_changes() != changes)
                    .then(|| crate::local_profile::phase("assessment.write_commit"));
                tx.commit()?;
                (packet, inputs, epoch, references, settings, report)
            };
            // Hashing the detached complete snapshot does not occupy the business connection.
            let hash = packet_digest(&packet)?;
            let published_epoch = {
                let mut c = self.connection()?;
                let _transaction = crate::local_profile::phase("assessment.publish_tx");
                let tx = c.transaction()?;
                let current: i64 = tx.query_row(
                    "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                    [id],
                    |r| r.get(0),
                )?;
                // The first export also depends on the as-yet-unpublished device id.
                // Another Store may have initialized it while hashing was detached.
                let device_changed = settings
                    .iter()
                    .any(|(key, _)| key == "assessment_device_v1")
                    && tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM settings WHERE key='assessment_device_v1')",
                        [],
                        |r| r.get::<_, bool>(0),
                    )?;
                if current != epoch || device_changed {
                    None
                } else {
                    tx.execute("INSERT OR IGNORE INTO assessment_sync_accounts(account_key,updated_at) VALUES (?,?)",params![scope,chrono::Utc::now().timestamp_millis()])?;
                    if let Some(report) = &report {
                        report.persist(&tx)?;
                    }
                    let published_epoch: i64 = tx.query_row(
                        "SELECT epoch FROM assessment_sync_dirty WHERE session_id=?",
                        [id],
                        |r| r.get(0),
                    )?;
                    write_settings(&tx, &settings)?;
                    tx.execute("INSERT INTO assessment_sync_state(account_key,session_id,cached_epoch,cached_digest,cached_pending_references) VALUES (?,?,?,?,?) ON CONFLICT(account_key,session_id) DO UPDATE SET cached_epoch=excluded.cached_epoch,cached_digest=excluded.cached_digest,cached_pending_references=excluded.cached_pending_references",params![scope,id,published_epoch,hash,references])?;
                    let _commit = crate::local_profile::phase("assessment.write_commit");
                    tx.commit()?;
                    Some(published_epoch)
                }
            };
            if let Some(epoch) = published_epoch {
                let prepared = Arc::new(PreparedPacket {
                    scope: scope.into(),
                    id: id.into(),
                    epoch,
                    packet,
                    hash,
                    inputs,
                });
                // Retain only the most recent complete packet, never one copy per historical session.
                if let Ok(mut cached) = self.assessment_packet.lock() {
                    *cached = Some(Arc::clone(&prepared));
                }
                return Ok(prepared);
            }
        }
    }
}
