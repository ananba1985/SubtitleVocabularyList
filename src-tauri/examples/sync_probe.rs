//! Runs the production sync adapter against an explicitly supplied private store.
//! The store must already contain a browser-approved, current-user protected connection.
use std::{sync::Arc, thread, time::Duration};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    store::Store,
    tasks::TaskManager,
    vocabulary::{EntryUpdate, MeaningUpdate},
};
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if !matches!(args.len(), 2 | 4) {
        return Err("Usage: sync_probe PRIVATE_STORE [ENTRY_ID CORRECTED_FIRST_MEANING]".into());
    }
    let store = Arc::new(Store::open(&args[1])?);
    let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
    let defaults = Settings {
        offline_mode: false,
        ..Default::default()
    };
    let app = Arc::new(Application::new(
        Arc::clone(&store),
        Arc::clone(&tasks),
        defaults,
    ));
    if args.len() == 4 {
        let entry = store.get_entry(&args[2])?;
        if entry.meanings.is_empty() {
            return Err("The test entry has no meaning to edit".into());
        }
        store.update_entry(&EntryUpdate {
            id: entry.id,
            expected_revision: entry.revision,
            text: entry.text,
            meanings: entry
                .meanings
                .into_iter()
                .enumerate()
                .map(|(index, m)| MeaningUpdate {
                    id: Some(m.id),
                    text: if index == 0 { args[3].clone() } else { m.text },
                })
                .collect(),
        })?;
    }
    let started = app.sync_start(Uuid::new_v4().to_string())?;
    loop {
        let task = tasks.get(&started.id)?;
        if task.terminal() {
            if task.state != "succeeded" {
                return Err(task
                    .error
                    .map(|e| e.to_string())
                    .unwrap_or(task.message)
                    .into());
            }
            let assets: i64 = store.connection()?.query_row(
                "SELECT COUNT(*) FROM media_assets WHERE kind='original' AND state='ready'",
                [],
                |r| r.get(0),
            )?;
            let attempts: i64 =
                store
                    .connection()?
                    .query_row("SELECT COUNT(*) FROM review_attempts", [], |r| r.get(0))?;
            let corrections: i64 = store.connection()?.query_row(
                "SELECT COUNT(*) FROM review_corrections",
                [],
                |r| r.get(0),
            )?;
            println!(
                "{}",
                serde_json::json!({
                    "result":task.result,"status":app.sync_status()?,
                    "originalAssets":assets,"answers":attempts,"corrections":corrections
                })
            );
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
}
