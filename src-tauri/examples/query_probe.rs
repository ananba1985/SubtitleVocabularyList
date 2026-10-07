//! Explicitly sends the supplied synthetic query through the production online adapter.
use std::{sync::Arc, thread, time::Duration};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    store::Store,
    tasks::TaskManager,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Usage: query_probe PRIVATE_STORE QUERY dictionary|translation".into());
    }
    let store = Arc::new(Store::open(&args[1])?);
    let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
    let app = Application::new(
        store,
        Arc::clone(&tasks),
        Settings {
            offline_mode: false,
            ..Default::default()
        },
    );
    let task = app.online_query_start(
        args[2].clone(),
        args[3].clone(),
        uuid::Uuid::new_v4().to_string(),
    )?;
    loop {
        let state = tasks.get(&task.id)?;
        if state.terminal() {
            println!(
                "{}",
                serde_json::json!({"state":state.state,"source":state.result.as_ref().map(|v|&v["source"]),"definitions":state.result.as_ref().and_then(|v|v["definitions"].as_array()).map(Vec::len),"error":state.error})
            );
            if state.state != "succeeded" {
                return Err("Query failed".into());
            }
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
}
