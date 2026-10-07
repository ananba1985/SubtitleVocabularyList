pub mod error;
pub mod media;
pub mod store;
pub mod vocabulary;

#[cfg(feature = "desktop")]
mod desktop {
    use crate::{error::AppError, store::Store};
    use serde::Serialize;
    use tauri::Manager;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct AppInfo {
        data_directory: String,
        schema_version: i64,
    }

    #[tauri::command]
    fn app_info(store: tauri::State<'_, Store>) -> Result<AppInfo, AppError> {
        Ok(AppInfo {
            data_directory: store.root().display().to_string(),
            schema_version: store.schema_version()?,
        })
    }

    pub fn run() {
        tauri::Builder::default()
            .setup(|app| {
                let root = app.path().app_data_dir()?;
                app.manage(Store::open(root)?);
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![app_info])
            .run(tauri::generate_context!())
            .expect("Cannot start SubtitleVocabularyList");
    }
}

#[cfg(feature = "desktop")]
pub use desktop::run;
