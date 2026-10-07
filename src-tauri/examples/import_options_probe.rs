//! Exercises the actual import task and cache against an explicitly supplied private store.
use std::{path::PathBuf, sync::Arc, thread, time::Duration};
use subtitle_vocabulary_list::{
    application::{Application, Settings},
    media::{ImportOptions, MediaTools},
    store::Store,
    tasks::TaskManager,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(
            "Usage: import_options_probe PRIVATE_STORE VIDEO OPTIONS_JSON RUNTIME_DIRECTORY".into(),
        );
    }
    let options: ImportOptions = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let store = Arc::new(Store::open(&args[1])?);
    let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
    let defaults = Settings {
        tools: MediaTools::development(PathBuf::from(&args[4])),
        ..Default::default()
    };
    let app = Application::new(store, Arc::clone(&tasks), defaults);
    let started = app.import_with_options(
        vec![args[2].clone()],
        uuid::Uuid::new_v4().to_string(),
        options,
    )?;
    loop {
        let task = tasks.get(&started.id)?;
        if task.terminal() {
            println!(
                "{}",
                serde_json::json!({"state":task.state,"result":task.result,"error":task.error})
            );
            if task.state != "succeeded" {
                return Err("Import did not succeed".into());
            }
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
}
