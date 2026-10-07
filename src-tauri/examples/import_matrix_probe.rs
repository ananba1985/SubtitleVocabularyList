//! Private format/directory integration sample, using the formal import task.
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    corpus,
    media::MediaTools,
    store::Store,
    tasks::TaskManager,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 5 {
        return Err("Usage: import_matrix_probe BUNDLED_TOOLS PRIVATE_INPUT_DIR NEW_PRIVATE_STORE DUPLICATE_FILE".into());
    }
    let paths = vec![args[2].clone(), args[4].clone()];
    let inputs = corpus::video_inputs(&paths)?;
    let store = Arc::new(Store::open(&args[3])?);
    let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
    let app = Application::new(
        Arc::clone(&store),
        Arc::clone(&tasks),
        Settings {
            tools: MediaTools::bundled(&args[1]),
            ..Default::default()
        },
    );
    let task = app.import_start(paths, uuid::Uuid::new_v4().to_string())?;
    let deadline = Instant::now() + Duration::from_secs(70);
    loop {
        let current = tasks.get(&task.id)?;
        if current.terminal() {
            if current.state != "succeeded" {
                return Err(format!("Import failed: {:?}", current.error).into());
            }
            println!(
                "{}",
                serde_json::json!({"inputCount":inputs.len(),"sources":store.sources()?,"result":current.result})
            );
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("Private import sample timed out".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
