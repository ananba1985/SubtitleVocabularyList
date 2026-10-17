//! Explicit normal acceptance against a browser-approved account and private existing data.
use super::*;
use crate::{
    application::Settings, assessment_sync::AssessmentDocument, store::Store, tasks::TaskManager,
};
use rusqlite::params;
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn app(root: &Path, credential: &SiteCredential) -> Arc<Application> {
    let store = Arc::new(Store::open(root).unwrap());
    credentials::save(
        root,
        "https://english-copy-practice.hxwjb.chatgpt.site",
        credential,
    )
    .unwrap();
    let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
    let app = Arc::new(Application::new(
        store,
        tasks,
        Settings {
            offline_mode: false,
            ..Default::default()
        },
    ));
    let mut settings = app.settings().unwrap();
    settings.offline_mode = false;
    settings.site_url = "https://english-copy-practice.hxwjb.chatgpt.site".into();
    app.save_settings(settings).unwrap();
    app
}
struct SelectedWord {
    remote: HttpRemote,
    entry: String,
}
impl Remote for SelectedWord {
    fn changes(&mut self, after: i64) -> Result<ChangePage, AppError> {
        let mut page = self.remote.changes(after)?;
        page.changes.retain(|v| v.entry_id == self.entry);
        if let Some(last) = page.changes.pop() {
            page.changes = vec![last];
        }
        Ok(page)
    }
    fn change(&mut self, c: &RemoteChange) -> Result<RemoteDocument, AppError> {
        self.remote.change(c)
    }
    fn push(&mut self, p: &Push) -> Result<PushReply, AppError> {
        self.remote.push(p)
    }
    fn upload(&mut self, h: &str, f: &str, b: Vec<u8>) -> Result<(), AppError> {
        self.remote.upload(h, f, b)
    }
    fn download(&mut self, h: &str, f: &str) -> Result<Vec<u8>, AppError> {
        self.remote.download(h, f)
    }
}
fn word_sync(app: &Arc<Application>, id: &str) -> crate::synchronization::SyncResult {
    let mut remote = HttpRemote::new(Arc::clone(app)).unwrap();
    remote.account().unwrap();
    assert!(remote.supports_delta && remote.supports_delta_download);
    let scope = remote.scope();
    let mut selected = SelectedWord {
        remote,
        entry: id.into(),
    };
    let result = app
        .store
        .synchronize(&scope, &mut selected, &AtomicBool::new(false), |_, _| {})
        .unwrap();
    assert_eq!(result.conflicts, 0);
    assert_eq!(app.store.synchronization_status(&scope).unwrap().pending, 0);
    result
}
fn touch_word(app: &Arc<Application>, id: &str) {
    let c = app.store.connection().unwrap();
    c.execute(
        "UPDATE entries SET revision=revision+1,updated_at=MAX(updated_at+1,?) WHERE id=?",
        params![chrono::Utc::now().timestamp_millis(), id],
    )
    .unwrap();
    crate::synchronization::mark_dirty(&c, id).unwrap();
}
fn word_base(app: &Arc<Application>, scope: &str, id: &str) -> (i64, Value) {
    app.store
        .sync_download_base(scope, id, false)
        .unwrap()
        .unwrap()
}
fn assessment_push(app: &Arc<Application>, id: &str) -> AssessmentDocument {
    let mut remote = HttpRemote::new(Arc::clone(app)).unwrap();
    remote.account().unwrap();
    let scope = remote.scope();
    let push = app
        .store
        .assessment_freeze_push(&scope, id)
        .unwrap()
        .unwrap();
    let base = app
        .store
        .sync_upload_base(&scope, id, push.base_revision, true)
        .unwrap();
    let (status, value) = Transport::new(&remote.client, &remote.site, &remote.credential.token)
        .push_changes(
            &format!("/api/svl/v1/assessments/{id}"),
            &serde_json::to_value(&push).unwrap(),
            base.as_ref(),
            "sessionId",
            remote.supports_chunks,
            || remote.check(),
        )
        .unwrap();
    assert_eq!(
        status,
        200,
        "{}",
        value.get("error").unwrap_or(&Value::Null)
    );
    let revision = value["revision"].as_i64().unwrap();
    app.store
        .assessment_sync_saved(&scope, &push, revision)
        .unwrap();
    AssessmentDocument {
        session_id: id.into(),
        revision,
        bundle: push.bundle,
    }
}
fn assessment_pull(app: &Arc<Application>, id: &str) -> AssessmentDocument {
    let mut remote = HttpRemote::new(Arc::clone(app)).unwrap();
    remote.account().unwrap();
    let doc: AssessmentDocument = serde_json::from_value(
        remote
            .get_document(&format!("/api/svl/v1/assessments/{id}"), id, true)
            .unwrap(),
    )
    .unwrap();
    remote.receive_assessment(&remote.scope(), &doc).unwrap();
    doc
}
fn touch_assessment(app: &Arc<Application>, id: &str) {
    app.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE assessment_sessions SET updated_at=MAX(updated_at+1,?) WHERE id=?",
            params![chrono::Utc::now().timestamp_millis(), id],
        )
        .unwrap();
}
fn learning_facts(packet: &Value) -> Value {
    let mut copy = packet.clone();
    copy["session"].as_object_mut().unwrap().remove("revision");
    copy["session"].as_object_mut().unwrap().remove("updatedAt");
    // Import advances the local projection revision. A subsequent freeze can
    // retain another report snapshot with the same facts and a new timestamp.
    let reports = copy["reports"].as_array_mut().unwrap();
    for report in reports.iter_mut() {
        let mut value: Value =
            serde_json::from_str(report["reportJson"].as_str().unwrap()).unwrap();
        value.as_object_mut().unwrap().remove("revision");
        value.as_object_mut().unwrap().remove("generatedAt");
        *report = value;
    }
    reports.sort_by_key(Value::to_string);
    reports.dedup();
    copy
}
fn word_facts(packet: &Value) -> Value {
    let mut tables = packet["tables"].clone();
    for (name, rows) in tables.as_object_mut().unwrap() {
        let rows = rows.as_array_mut().unwrap();
        for row in rows.iter_mut() {
            if name == "review_states" {
                row.as_object_mut().unwrap().remove("revision");
            }
            for (key, value) in row.as_object_mut().unwrap() {
                if key.ends_with("_json") {
                    if let Some(raw) = value.as_str() {
                        *value = serde_json::from_str(raw).unwrap();
                    }
                }
            }
        }
        rows.sort_by_key(Value::to_string);
    }
    tables
}
fn save(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn confirmed_cache(app: &Arc<Application>, scope: &str, id: &str) -> Value {
    let valid: bool = app.store.connection().unwrap().query_row(
        "SELECT s.cached_epoch=d.epoch AND s.cached_digest=s.acknowledged_digest AND s.acknowledged_digest IS NOT NULL AND s.pending_json IS NULL AND s.pending_change_id IS NULL AND s.pending_digest IS NULL AND s.conflict_json IS NULL FROM assessment_sync_state s JOIN assessment_sync_dirty d ON d.session_id=s.session_id WHERE s.account_key=? AND s.session_id=?",
        params![scope,id], |r|r.get(0)).unwrap();
    assert!(
        valid,
        "confirmed cache must describe the current complete facts"
    );
    crate::local_profile::capture();
    let status = app.store.assessment_sync_status(scope).unwrap();
    assert_eq!(status.pending, 0);
    assert_eq!(status.conflicts, 0);
    assert!(
        app.store
            .assessment_freeze_push(scope, id)
            .unwrap()
            .is_none()
    );
    let samples = crate::local_profile::take_samples();
    assert_eq!(samples.len(), 2);
    assert!(samples.iter().all(|s| {
        [
            "assessment.export",
            "assessment.digest",
            "assessment.read_tx",
            "assessment.write_commit",
        ]
        .iter()
        .all(|phase| !s.timings.contains_key(phase))
    }));
    json!({"passed":true,"samples":samples})
}

fn reports_unchanged(app: &Arc<Application>, original: &AssessmentDocument) {
    let view = app
        .store
        .assessment_report(&original.session_id, None)
        .unwrap();
    assert_eq!(
        view["revisions"].as_array().unwrap().len(),
        original.bundle.reports.len()
    );
    let c = app.store.connection().unwrap();
    let mut q = c
        .prepare("SELECT report_json FROM assessment_report_versions WHERE session_id=?")
        .unwrap();
    let reports = q
        .query_map([&original.session_id], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(reports.len(), original.bundle.reports.len());
    assert!(
        original
            .bundle
            .reports
            .iter()
            .all(|r| reports.contains(&r.report_json)),
        "all old reports must retain their original JSON bytes"
    );
}

fn pull_confirmed(app: &Arc<Application>, id: &str) -> (AssessmentDocument, Value) {
    crate::local_profile::capture();
    let document = assessment_pull(app, id);
    let samples = crate::local_profile::take_samples();
    let receives = samples
        .iter()
        .filter(|s| s.operation == "assessment.receive")
        .collect::<Vec<_>>();
    assert_eq!(receives.len(), 1);
    assert_eq!(receives[0].timings["assessment.export"].calls, 1);
    assert_eq!(receives[0].timings["assessment.digest"].calls, 1);
    (document, json!(samples))
}

#[test]
#[ignore = "Explicitly authorized normal roundtrip with approved credentials and selected real data"]
fn actual_report_reuse_and_receipt_cache_roundtrip() {
    let root = PathBuf::from(
        std::env::var_os("SVL_REAL_SYNC_ROOT").expect("fresh private acceptance root"),
    );
    assert!(
        !root.exists(),
        "preserve previous evidence; use a new acceptance directory"
    );
    let credential_root = PathBuf::from(
        std::env::var_os("SVL_LIVE_CREDENTIAL_ROOT").expect("approved credential directory"),
    );
    let credential = credentials::load(
        &credential_root,
        "https://english-copy-practice.hxwjb.chatgpt.site",
    )
    .unwrap()
    .unwrap();
    assert!(credential.account_scope.is_some());
    let entry = std::env::var("SVL_REAL_ENTRY_ID").unwrap();
    let session = std::env::var("SVL_REAL_SESSION_ID").unwrap();
    let scope = format!(
        "https://english-copy-practice.hxwjb.chatgpt.site|{}",
        credential.account_scope.as_deref().unwrap()
    );
    let source = app(&root.join("source"), &credential);
    let peer = app(&root.join("peer"), &credential);

    // Fetch only the two selected real records, using the approved account and production transport.
    word_sync(&source, &entry);
    word_sync(&peer, &entry);
    let (word_revision, word_original) = word_base(&source, &scope, &entry);
    let original_facts = word_facts(&word_original);
    save(
        &root.join("word-before.json"),
        &json!({"revision":word_revision,"bundle":word_original}),
    );
    crate::sync_transport::take_live_transfers();
    touch_word(&source, &entry);
    word_sync(&source, &entry);
    word_sync(&peer, &entry);
    touch_word(&peer, &entry);
    word_sync(&peer, &entry);
    word_sync(&source, &entry);
    let word_transfers = crate::sync_transport::take_live_transfers();
    let (word_end, word_returned) = word_base(&source, &scope, &entry);
    assert_eq!(word_end, word_revision + 2);
    assert!(
        word_facts(&word_returned) == original_facts,
        "word learning facts changed"
    );
    assert!(word_base(&peer, &scope, &entry) == (word_end, word_returned.clone()));
    save(
        &root.join("word-after.json"),
        &json!({"revision":word_end,"bundle":word_returned}),
    );
    for asset in word_original["tables"]["media_assets"].as_array().unwrap() {
        let hash = asset["digest"].as_str().unwrap();
        let format = asset["format"].as_str().unwrap();
        for app in [&source, &peer] {
            let c = app.store.connection().unwrap();
            let local: String = c
                .query_row(
                    "SELECT id FROM media_assets WHERE digest=? AND format=?",
                    params![hash, format],
                    |r| r.get(0),
                )
                .unwrap();
            drop(c);
            assert_eq!(
                digest(&std::fs::read(app.store.media_file(&local).unwrap()).unwrap()),
                hash
            );
        }
    }

    let original = assessment_pull(&source, &session);
    let peer_original = assessment_pull(&peer, &session);
    assert!(
        serde_json::to_value(&peer_original).unwrap() == serde_json::to_value(&original).unwrap()
    );
    assert_eq!(
        original.bundle.answers.len(),
        6,
        "select the existing real six-question session"
    );
    assert!(!original.bundle.reports.is_empty());
    let study_facts = learning_facts(&serde_json::to_value(&original.bundle).unwrap());
    save(
        &root.join("assessment-before.json"),
        &serde_json::to_value(&original).unwrap(),
    );
    reports_unchanged(&source, &original);
    reports_unchanged(&peer, &original);
    crate::sync_transport::take_live_transfers();

    touch_assessment(&source, &session);
    assert_eq!(
        source.store.assessment_sync_status(&scope).unwrap().pending,
        1
    );
    let sent = assessment_push(&source, &session);
    assert_eq!(sent.revision, original.revision + 1);
    reports_unchanged(&source, &original);
    let source_ack = confirmed_cache(&source, &scope, &session);
    let (_, peer_receive) = pull_confirmed(&peer, &session);
    reports_unchanged(&peer, &original);

    touch_assessment(&peer, &session);
    assert_eq!(
        peer.store.assessment_sync_status(&scope).unwrap().pending,
        1
    );
    let sent_back = assessment_push(&peer, &session);
    assert_eq!(sent_back.revision, original.revision + 2);
    reports_unchanged(&peer, &original);
    let peer_ack = confirmed_cache(&peer, &scope, &session);
    let (returned, source_receive) = pull_confirmed(&source, &session);
    reports_unchanged(&source, &original);
    assert!(
        learning_facts(&serde_json::to_value(&returned.bundle).unwrap()) == study_facts,
        "assessment learning facts changed"
    );
    let final_cache = confirmed_cache(&source, &scope, &session);
    let assessment_transfers = crate::sync_transport::take_live_transfers();
    for transfers in [&word_transfers, &assessment_transfers] {
        for direction in ["upload", "download"] {
            assert_eq!(
                transfers
                    .iter()
                    .filter(|t| t["direction"] == direction && t["delta"] == true)
                    .count(),
                2
            );
        }
    }
    save(
        &root.join("assessment-after.json"),
        &serde_json::to_value(&returned).unwrap(),
    );
    save(
        &root.join("transfers.json"),
        &json!({"word":word_transfers,"assessment":assessment_transfers}),
    );
    save(
        &root.join("cache-probes.json"),
        &json!({"sourceAck":source_ack,"peerAck":peer_ack,"finalReceive":final_cache,"receives":[peer_receive,source_receive]}),
    );
    let result = json!({"passed":true,"applicationVersion":env!("CARGO_PKG_VERSION"),
        "word":{"initialRevision":word_revision,"finalRevision":word_end,"examples":original_facts["examples"].as_array().unwrap().len(),"answers":original_facts["review_attempts"].as_array().unwrap().len(),"corrections":original_facts["review_corrections"].as_array().unwrap().len(),"originalAssets":word_original["tables"]["media_assets"].as_array().unwrap().len(),"factsDigest":digest(original_facts.to_string().as_bytes())},
        "assessment":{"initialRevision":original.revision,"finalRevision":returned.revision,"answers":returned.bundle.answers.len(),"corrections":returned.bundle.corrections.len(),"initialReports":original.bundle.reports.len(),"finalReports":returned.bundle.reports.len(),"factsDigest":digest(study_facts.to_string().as_bytes())},"cacheProbes":3,"receiveProbes":2,"deltaUploads":4,"deltaDownloads":4});
    save(&root.join("result.json"), &result);
    println!("{result}");
}

#[test]
#[ignore = "Only run on explicit authorization with approved credentials and existing real private data"]
fn actual_incremental_history_roundtrip() {
    let root =
        PathBuf::from(std::env::var_os("SVL_REAL_SYNC_ROOT").expect("dedicated acceptance root"));
    let credential_root = PathBuf::from(
        std::env::var_os("SVL_LIVE_CREDENTIAL_ROOT").expect("existing approved store"),
    );
    let entry = std::env::var("SVL_REAL_ENTRY_ID").unwrap();
    let session = std::env::var("SVL_REAL_SESSION_ID").unwrap();
    let credential = credentials::load(
        &credential_root,
        "https://english-copy-practice.hxwjb.chatgpt.site",
    )
    .unwrap()
    .unwrap();
    assert!(credential.account_scope.is_some());
    let source = app(&root.join("word-source"), &credential);
    let peer = app(&root.join("word-peer"), &credential);
    let scope = format!(
        "https://english-copy-practice.hxwjb.chatgpt.site|{}",
        credential.account_scope.as_deref().unwrap()
    );
    word_sync(&source, &entry);
    word_sync(&peer, &entry);
    let original_path = root.join("word-original.json");
    if !original_path.exists() {
        let (revision, bundle) = word_base(&source, &scope, &entry);
        save(
            &original_path,
            &json!({"revision":revision,"bundle":bundle}),
        );
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(&original_path).unwrap()).unwrap();
    let start = saved["revision"].as_i64().unwrap();
    let original = saved["bundle"].clone();
    assert!(!original["deleted"].as_bool().unwrap());
    assert!(!original["tables"]["examples"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!original["tables"]["media_assets"]
        .as_array()
        .unwrap()
        .is_empty());
    let facts = word_facts(&original);
    let roundtrip_path = root.join("word-roundtrip.json");
    if word_base(&source, &scope, &entry).0 == start {
        crate::sync_transport::take_live_transfers();
        touch_word(&source, &entry);
        word_sync(&source, &entry);
        word_sync(&peer, &entry);
        touch_word(&peer, &entry);
        word_sync(&peer, &entry);
        word_sync(&source, &entry);
        let roundtrip = crate::sync_transport::take_live_transfers();
        assert_eq!(
            roundtrip
                .iter()
                .filter(|r| r["direction"] == "upload" && r["delta"] == true)
                .count(),
            2
        );
        assert_eq!(
            roundtrip
                .iter()
                .filter(|r| r["direction"] == "download" && r["delta"] == true)
                .count(),
            2
        );
        save(&roundtrip_path, &json!(roundtrip));
    }
    let roundtrip: Option<Value> = roundtrip_path
        .exists()
        .then(|| serde_json::from_slice(&std::fs::read(&roundtrip_path).unwrap()).unwrap());
    // First post-upgrade write is a checkpoint. Reach its next normal 16-version checkpoint.
    while word_base(&source, &scope, &entry).0 < start + 17 {
        touch_word(&source, &entry);
        word_sync(&source, &entry);
    }
    let (end, latest) = word_base(&source, &scope, &entry);
    assert!(word_facts(&latest) == facts, "word facts changed");
    let receiver = app(&root.join("word-receiver"), &credential);
    word_sync(&receiver, &entry);
    assert_eq!(word_base(&receiver, &scope, &entry), (end, latest));
    for asset in original["tables"]["media_assets"].as_array().unwrap() {
        let hash = asset["digest"].as_str().unwrap();
        let format = asset["format"].as_str().unwrap();
        let c = receiver.store.connection().unwrap();
        let local: String = c
            .query_row(
                "SELECT id FROM media_assets WHERE digest=? AND format=?",
                params![hash, format],
                |r| r.get(0),
            )
            .unwrap();
        drop(c);
        assert_eq!(
            digest(&std::fs::read(receiver.store.media_file(&local).unwrap()).unwrap()),
            hash
        );
    }
    let actual = app(&root.join("assessment-source"), &credential);
    let study_path = root.join("assessment-original.json");
    let first: AssessmentDocument = if study_path.exists() {
        serde_json::from_slice(&std::fs::read(&study_path).unwrap()).unwrap()
    } else {
        let document = assessment_push(&actual, &session);
        save(&study_path, &serde_json::to_value(&document).unwrap());
        document
    };
    assert_eq!(
        first.bundle.answers.len(),
        6,
        "Use the user's actual completed six-question session"
    );
    let original_study = learning_facts(&serde_json::to_value(&first.bundle).unwrap());
    let other = app(&root.join("assessment-peer"), &credential);
    let resumed = actual
        .store
        .sync_download_base(&scope, &session, true)
        .unwrap()
        .is_some_and(|(revision, _)| revision >= first.revision + 2);
    let returned = if resumed {
        assessment_pull(&actual, &session)
    } else {
        assessment_pull(&other, &session);
        crate::sync_transport::take_live_transfers();
        touch_assessment(&actual, &session);
        assessment_push(&actual, &session);
        assessment_pull(&other, &session);
        touch_assessment(&other, &session);
        assessment_push(&other, &session);
        assessment_pull(&actual, &session)
    };
    let assessment_transfers = if resumed {
        None
    } else {
        Some(crate::sync_transport::take_live_transfers())
    };
    if let Some(ref assessment_transfers) = assessment_transfers {
        save(
            &root.join("assessment-roundtrip.json"),
            &json!(assessment_transfers),
        );
        assert_eq!(
            assessment_transfers
                .iter()
                .filter(|r| r["direction"] == "upload" && r["delta"] == true)
                .count(),
            2
        );
        assert_eq!(
            assessment_transfers
                .iter()
                .filter(|r| r["direction"] == "download" && r["delta"] == true)
                .count(),
            2
        );
    }
    assert!(
        learning_facts(&serde_json::to_value(&returned.bundle).unwrap()) == original_study,
        "assessment facts changed"
    );
    assert!(
        first.bundle.reports.iter().all(|original| returned
            .bundle
            .reports
            .iter()
            .any(|v| v.digest == original.digest && v.report_json == original.report_json)),
        "original report not retained byte for byte"
    );
    let result = json!({"passed":true,"word":{"initialRevision":start,"finalRevision":end,"examples":facts["examples"].as_array().unwrap().len(),"answers":facts["review_attempts"].as_array().unwrap().len(),"corrections":facts["review_corrections"].as_array().unwrap().len(),"originalAssets":facts["media_assets"].as_array().unwrap().len(),"factsDigest":digest(facts.to_string().as_bytes()),"roundtrip":roundtrip},"assessment":{"answers":returned.bundle.answers.len(),"reports":returned.bundle.reports.len(),"factsDigest":digest(original_study.to_string().as_bytes()),"roundtrip":assessment_transfers}});
    std::fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    println!("{result}");
}
