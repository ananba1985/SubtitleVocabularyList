use crate::{
    reviews::AnswerInput,
    store::Store,
    sync_data::SyncBundle,
    synchronization::*,
    vocabulary::{CollectionInput, EntryUpdate, ExampleInput, MeaningUpdate},
};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
use uuid::Uuid;

#[derive(Default)]
struct Server {
    docs: BTreeMap<String, RemoteDocument>,
    events: Vec<RemoteDocument>,
    receipts: BTreeMap<String, (Push, i64)>,
    lost_reply: bool,
    received: Vec<String>,
    media: BTreeMap<String, Vec<u8>>,
}
impl Remote for Server {
    fn changes(&mut self, after: i64) -> Result<ChangePage, crate::error::AppError> {
        Ok(ChangePage {
            changes: self
                .events
                .iter()
                .enumerate()
                .filter(|(i, _)| (*i as i64) + 1 > after)
                .map(|(i, d)| RemoteChange {
                    cursor: (i as i64) + 1,
                    entry_id: d.entry_id.clone(),
                    revision: d.revision,
                    change_id: Uuid::new_v4().to_string(),
                })
                .collect(),
            backfill_pending: false,
        })
    }
    fn change(&mut self, cursor: i64) -> Result<RemoteDocument, crate::error::AppError> {
        Ok(self.events[(cursor - 1) as usize].clone())
    }
    fn push(&mut self, p: &Push) -> Result<PushReply, crate::error::AppError> {
        crate::sync_merge::validate(&p.bundle)?;
        let json = serde_json::to_string(p)?;
        self.received.push(json);
        if let Some((old, revision)) = self.receipts.get(&p.change_id) {
            assert_eq!(
                serde_json::to_value(old).unwrap(),
                serde_json::to_value(p).unwrap(),
                "a retry must retain the exact frozen change"
            );
            return Ok(PushReply::Saved { i: *revision });
        }
        let entry = p.bundle.entry["id"].as_str().unwrap();
        if let Some(current) = self.docs.get(entry)
            && current.revision != p.base_revision
        {
            return Ok(PushReply::Conflict(current.clone()));
        }
        let doc = RemoteDocument {
            entry_id: entry.into(),
            revision: p.base_revision + 1,
            bundle: p.bundle.clone(),
        };
        self.docs.insert(entry.into(), doc.clone());
        self.events.push(doc);
        self.receipts
            .insert(p.change_id.clone(), (p.clone(), p.base_revision + 1));
        if self.lost_reply {
            self.lost_reply = false;
            return Err(crate::error::AppError::new(
                "network_error",
                "synthetic lost reply",
            ));
        }
        Ok(PushReply::Saved {
            i: p.base_revision + 1,
        })
    }
    fn upload(
        &mut self,
        hash: &str,
        _: &str,
        bytes: Vec<u8>,
    ) -> Result<(), crate::error::AppError> {
        assert_eq!(crate::vocabulary::digest(&bytes), hash);
        self.media.insert(hash.into(), bytes);
        Ok(())
    }
    fn download(&mut self, hash: &str, _: &str) -> Result<Vec<u8>, crate::error::AppError> {
        self.media.get(hash).cloned().ok_or_else(|| {
            crate::error::AppError::new("resource_missing", "synthetic missing media")
        })
    }
}
#[test]
fn original_audio_bytes_are_saved_once_and_a_corrupt_download_keeps_progress() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = Store::open(a.path()).unwrap();
    let second = Store::open(b.path()).unwrap();
    let bytes=b"RIFF\x24\x00\x00\x00WAVEfmt \x10\x00\x00\x00\x01\x00\x01\x00\x22\x56\x00\x00\x44\xac\x00\x00\x02\x00\x10\x00data\x00\x00\x00\x00".to_vec();
    let hash = crate::vocabulary::digest(&bytes);
    let asset = Uuid::new_v4().to_string();
    std::fs::write(a.path().join("media/fixture.wav"), &bytes).unwrap();
    first.connection().unwrap().execute("INSERT INTO media_assets(id,recipe_key,digest,relative_path,kind,format,duration_ms,state,created_at) VALUES (?,?,?,'media/fixture.wav','original','wav',1000,'ready',1)",rusqlite::params![asset,hash,hash]).unwrap();
    let entry = first
        .collect(&CollectionInput {
            operation_id: Uuid::new_v4().to_string(),
            kind: "word".into(),
            text: "reluctant".into(),
            meaning: "不情愿的".into(),
            examples: vec![ExampleInput {
                text: "A reluctant speaker.".into(),
                media_asset_ids: vec![asset],
                ..Default::default()
            }],
            target_entry_id: None,
            expected_revision: None,
        })
        .unwrap()
        .entry_id;
    let mut server = Server::default();
    sync(&first, &mut server);
    server.media.insert(hash.clone(), b"bad download".to_vec());
    assert_eq!(
        second
            .synchronize(
                "fixture-account",
                &mut server,
                &AtomicBool::new(false),
                |_, _| {}
            )
            .unwrap_err()
            .code,
        "invalid_data"
    );
    assert_eq!(
        second
            .synchronization_status("fixture-account")
            .unwrap()
            .cursor,
        0
    );
    server.media.insert(hash, bytes.clone());
    sync(&second, &mut server);
    let saved = second.get_entry(&entry).unwrap();
    assert_eq!(saved.examples[0].audio.len(), 1);
    assert_eq!(
        std::fs::read(second.media_file(&saved.examples[0].audio[0].id).unwrap()).unwrap(),
        bytes
    );
    sync(&second, &mut server);
    assert_eq!(second.get_entry(&entry).unwrap().examples[0].audio.len(), 1);
}
fn word(store: &Store, text: &str, meaning: &str, target: Option<&str>) -> String {
    let existing = target.map(|id| store.get_entry(id).unwrap());
    store
        .collect(&CollectionInput {
            operation_id: Uuid::new_v4().to_string(),
            kind: "word".into(),
            text: text.into(),
            meaning: meaning.into(),
            examples: vec![ExampleInput {
                text: format!("A synthetic {text} example."),
                ..Default::default()
            }],
            target_entry_id: target.map(str::to_owned),
            expected_revision: existing.map(|e| e.revision),
        })
        .unwrap()
        .entry_id
}
fn sync(store: &Store, server: &mut Server) -> SyncResult {
    store
        .synchronize(
            "fixture-account",
            server,
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap()
}
#[test]
fn bidirectional_full_history_roundtrip_and_replay_do_not_add_collections() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = Store::open(a.path()).unwrap();
    let second = Store::open(b.path()).unwrap();
    let id = word(&first, "reluctant", "不情愿的", None);
    let unit = first
        .review_units("all", "meaning", 0, 100)
        .unwrap()
        .remove(0);
    let q = first.review_question(&unit.id, unit.revision).unwrap();
    first
        .review_submit(&AnswerInput {
            operation_id: Uuid::new_v4().to_string(),
            question_id: q.id,
            answer: "不情愿的".into(),
            unable: false,
        })
        .unwrap();
    let mut server = Server::default();
    assert_eq!(sync(&first, &mut server).pushed, 1);
    assert_eq!(sync(&second, &mut server).pulled, 1);
    assert_eq!(second.get_entry(&id).unwrap().collection_count, 1);
    assert_eq!(
        second.sync_export(&id).unwrap().tables["review_attempts"].len(),
        1
    );
    assert_eq!(
        second.review_units("all", "meaning", 0, 100).unwrap()[0]
            .state
            .streak,
        1
    );
    assert_eq!(sync(&second, &mut server).pulled, 0);
    assert_eq!(second.get_entry(&id).unwrap().collection_count, 1);
    word(&second, "reluctant", "犹豫的", Some(&id));
    sync(&second, &mut server);
    sync(&first, &mut server);
    assert_eq!(first.get_entry(&id).unwrap().collection_count, 2);
    assert_eq!(first.get_entry(&id).unwrap().meanings.len(), 2);
}
#[test]
fn a_lost_receipt_retries_old_payload_before_sending_new_edits() {
    let a = tempfile::tempdir().unwrap();
    let store = Store::open(a.path()).unwrap();
    let id = word(&store, "morning", "早上", None);
    let mut server = Server {
        lost_reply: true,
        ..Default::default()
    };
    assert_eq!(
        store
            .synchronize(
                "fixture-account",
                &mut server,
                &AtomicBool::new(false),
                |_, _| {}
            )
            .unwrap_err()
            .code,
        "network_error"
    );
    word(&store, "morning", "早安", Some(&id));
    let result = sync(&store, &mut server);
    assert_eq!(result.pushed, 2);
    assert_eq!(server.events.len(), 2);
    assert_eq!(server.received[0], server.received[1]);
    assert_ne!(server.received[1], server.received[2]);
    assert_eq!(
        server.docs[&id].bundle.tables["collection_actions"].len(),
        2
    );
}
#[test]
fn simultaneous_meaning_edits_preserve_both_versions_until_explicit_resolution() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = Store::open(a.path()).unwrap();
    let second = Store::open(b.path()).unwrap();
    let id = word(&first, "morning", "早上", None);
    let mut server = Server::default();
    sync(&first, &mut server);
    sync(&second, &mut server);
    for (store, text) in [(&first, "上午"), (&second, "早晨")] {
        let entry = store.get_entry(&id).unwrap();
        store
            .update_entry(&EntryUpdate {
                id: id.clone(),
                expected_revision: entry.revision,
                text: entry.text,
                meanings: vec![MeaningUpdate {
                    id: Some(entry.meanings[0].id.clone()),
                    text: text.into(),
                }],
            })
            .unwrap();
    }
    sync(&first, &mut server);
    assert_eq!(sync(&second, &mut server).conflicts, 1);
    assert_eq!(
        server.docs.len(),
        1,
        "an identity conflict must not publish an independent duplicate before a choice"
    );
    assert_eq!(second.get_entry(&id).unwrap().meanings[0].text, "早晨");
    let conflict = second
        .synchronization_conflicts("fixture-account")
        .unwrap()
        .remove(0);
    assert_eq!(conflict.remote.tables["meanings"][0]["text"], "上午");
    second
        .synchronization_resolve(
            "fixture-account",
            &ResolutionInput {
                conflict_id: conflict.id.clone(),
                choice: "local".into(),
                target_entry_id: None,
                expected_remote_revision: conflict.remote_revision,
                expected_local_revision: conflict
                    .local
                    .as_ref()
                    .and_then(|b| b.entry["revision"].as_i64()),
                expected_target_revision: None,
            },
            &mut server,
            &AtomicBool::new(false),
        )
        .unwrap();
    sync(&second, &mut server);
    sync(&first, &mut server);
    assert_eq!(first.get_entry(&id).unwrap().meanings[0].text, "早晨");
    assert_eq!(first.get_entry(&id).unwrap().collection_count, 1);
}
#[test]
fn same_spelling_requires_a_choice_and_merge_preserves_existing_actions() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = Store::open(a.path()).unwrap();
    let second = Store::open(b.path()).unwrap();
    let remote_id = word(&first, "morning", "早上", None);
    let local_id = word(&second, "morning", "早安", None);
    let mut server = Server::default();
    sync(&first, &mut server);
    assert_eq!(sync(&second, &mut server).conflicts, 1);
    assert_eq!(
        server.docs.len(),
        1,
        "do not publish a duplicate before choosing its identity"
    );
    assert_eq!(second.list_entries("", 0, 100).unwrap().len(), 1);
    let c = second
        .synchronization_conflicts("fixture-account")
        .unwrap()
        .remove(0);
    second
        .synchronization_resolve(
            "fixture-account",
            &ResolutionInput {
                conflict_id: c.id.clone(),
                choice: "merge".into(),
                target_entry_id: Some(local_id.clone()),
                expected_remote_revision: c.remote_revision,
                expected_local_revision: None,
                expected_target_revision: Some(second.get_entry(&local_id).unwrap().revision),
            },
            &mut server,
            &AtomicBool::new(false),
        )
        .unwrap();
    sync(&second, &mut server);
    assert_eq!(second.get_entry(&local_id).unwrap().collection_count, 2);
    assert_eq!(second.get_entry(&local_id).unwrap().meanings.len(), 2);
    assert_eq!(second.list_entries("", 0, 100).unwrap().len(), 1);
    assert_eq!(
        server.docs[&remote_id].bundle.tables["collection_actions"].len(),
        2
    );
}

#[test]
fn recomputed_projection_does_not_create_endless_echo_pushes() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = Store::open(a.path()).unwrap();
    let second = Store::open(b.path()).unwrap();
    let id = word(&first, "morning", "早上", None);
    let mut server = Server::default();
    sync(&first, &mut server);
    sync(&second, &mut server);
    word(&first, "morning", "上午", Some(&id));
    sync(&first, &mut server);
    assert_eq!(sync(&second, &mut server).pushed, 0);
    assert_eq!(sync(&first, &mut server).pushed, 0);
    assert_eq!(sync(&second, &mut server).pushed, 0);
    assert_eq!(server.events.len(), 2);
}
#[test]
fn malformed_download_or_missing_audio_does_not_advance_the_cursor() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = Store::open(a.path()).unwrap();
    let second = Store::open(b.path()).unwrap();
    let id = word(&first, "morning", "早上", None);
    let mut bundle = first.sync_export(&id).unwrap();
    let example = bundle.tables["examples"][0]["id"].clone();
    let asset = Uuid::new_v4().to_string();
    bundle.tables.get_mut("media_assets").unwrap().push(serde_json::json!({"id":asset,"recipe_key":"a".repeat(64),"digest":"b".repeat(64),"kind":"original","format":"wav","duration_ms":1000,"state":"ready","created_at":1}));
    bundle
        .tables
        .get_mut("example_media")
        .unwrap()
        .push(serde_json::json!({"example_id":example,"asset_id":asset}));
    let mut server = Server::default();
    server.events.push(RemoteDocument {
        entry_id: id,
        revision: 1,
        bundle,
    });
    assert_eq!(
        second
            .synchronize(
                "fixture-account",
                &mut server,
                &AtomicBool::new(false),
                |_, _| {}
            )
            .unwrap_err()
            .code,
        "resource_missing"
    );
    assert_eq!(
        second
            .synchronization_status("fixture-account")
            .unwrap()
            .cursor,
        0
    );
    assert!(second.list_entries("", 0, 100).unwrap().is_empty());
}
#[test]
fn deletion_is_explicit_and_archiving_preserves_history() {
    let a = tempfile::tempdir().unwrap();
    let store = Store::open(a.path()).unwrap();
    let id = word(&store, "morning", "早上", None);
    let mut server = Server::default();
    sync(&store, &mut server);
    let mut deleted = server.docs[&id].bundle.clone();
    deleted.deleted = true;
    server
        .push(&Push {
            change_id: Uuid::new_v4().to_string(),
            base_revision: 1,
            bundle: deleted,
        })
        .unwrap();
    assert_eq!(sync(&store, &mut server).conflicts, 1);
    assert_eq!(store.list_entries("", 0, 100).unwrap().len(), 1);
    let c = store
        .synchronization_conflicts("fixture-account")
        .unwrap()
        .remove(0);
    store
        .synchronization_resolve(
            "fixture-account",
            &ResolutionInput {
                conflict_id: c.id.clone(),
                choice: "archive".into(),
                target_entry_id: None,
                expected_remote_revision: c.remote_revision,
                expected_local_revision: c
                    .local
                    .as_ref()
                    .and_then(|b| b.entry["revision"].as_i64()),
                expected_target_revision: None,
            },
            &mut server,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(store.list_entries("", 0, 100).unwrap().is_empty());
    assert_eq!(store.get_entry(&id).unwrap().collection_count, 1);
    assert_eq!(store.sync_export(&id).unwrap().tables["examples"].len(), 1);
}
#[test]
fn cancelled_sync_and_account_cursors_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    word(&store, "morning", "早上", None);
    let mut server = Server::default();
    assert_eq!(
        store
            .synchronize(
                "fixture-account",
                &mut server,
                &AtomicBool::new(true),
                |_, _| {}
            )
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert!(server.received.is_empty());
    sync(&store, &mut server);
    sync(&store, &mut server);
    assert!(
        store
            .synchronization_status("fixture-account")
            .unwrap()
            .cursor
            > 0
    );
    assert_eq!(
        store
            .synchronization_status("another-account")
            .unwrap()
            .cursor,
        0
    );
}
#[test]
fn packet_validation_rejects_runtime_paths_and_changed_event_originals() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = word(&store, "morning", "早上", None);
    let packet = store.sync_export(&id).unwrap();
    crate::sync_merge::validate(&packet).unwrap();
    let mut private = packet.clone();
    private.tables.get_mut("examples").unwrap()[0]["relative_path"] = "C:/private".into();
    assert!(crate::sync_merge::validate(&private).is_err());
    let mut altered: SyncBundle = packet.clone();
    altered.tables.get_mut("collection_actions").unwrap()[0]["request_hash"] =
        "f".repeat(64).into();
    assert_eq!(
        crate::sync_merge::merge(Some(&packet), &packet, &altered, Some("remote"))
            .unwrap_err()
            .code,
        "sync_conflict"
    );
}
