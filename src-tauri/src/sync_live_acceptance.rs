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
