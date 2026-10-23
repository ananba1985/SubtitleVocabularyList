use super::{AppState, background};
use crate::error::AppError;

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuerySeed {
    text: String,
    kind: String,
    context: String,
    meaning: String,
    source_id: Option<String>,
    source_title: String,
    location_key: String,
}

#[svl_ipc_macros::command]
pub(crate) async fn mobile_query_seed(
    app: AppState<'_>,
    session_id: String,
    revision: u64,
    provider_id: String,
) -> Result<QuerySeed, AppError> {
    let app = app.inner().clone();
    background(move || {
        Ok(serde_json::from_value(app.query_collection(
            &session_id,
            revision,
            &provider_id,
        )?)?)
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_copy(text: String) -> Result<(), AppError> {
    if text.len() > 100000 {
        return Err(AppError::new("invalid_input", "复制文字过长。"));
    }
    background(move || {
        crate::mobile_video::invoke::<serde_json::Value>(
            "copyText",
            serde_json::json!({"text":text}),
        )?;
        Ok(())
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn entry_review_history(
    app: AppState<'_>,
    entry_id: String,
    offset: u32,
    limit: u32,
) -> Result<Vec<crate::reviews::Attempt>, AppError> {
    let app = app.inner().clone();
    background(move || app.store.entry_review_history(&entry_id, offset, limit)).await
}
#[derive(serde::Serialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct MobileResources {
    dictionaries_ready: bool,
    ocr_ready: bool,
    ocr_model_bytes: u64,
    temporary_images: u64,
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_resources_get(app: AppState<'_>) -> Result<MobileResources, AppError> {
    let app = app.inner().clone();
    background(move || {
        let size = std::fs::metadata(app.store.root().join("ocr/tessdata/eng.traineddata"))
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(MobileResources {
            dictionaries_ready: app.store.dictionary_warnings().is_empty(),
            ocr_ready: size == 15400601,
            ocr_model_bytes: size,
            temporary_images: app.store.connection()?.query_row(
                "SELECT COUNT(*) FROM settings WHERE key LIKE 'android.image:%'",
                [],
                |r| r.get(0),
            )?,
        })
    })
    .await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_images_release(
    app: AppState<'_>,
    ids: Vec<String>,
) -> Result<(), AppError> {
    let app = app.inner().clone();
    background(move || crate::mobile_capture::release_images(&app.store, &ids)).await
}
#[svl_ipc_macros::command]
pub(crate) async fn mobile_images_cleanup(
    app: AppState<'_>,
    keep: Vec<String>,
) -> Result<(), AppError> {
    let app = app.inner().clone();
    background(move || {
        let ids = {
            let db = app.store.connection()?;
            let mut q =
                db.prepare("SELECT substr(key,15) FROM settings WHERE key LIKE 'android.image:%'")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        crate::mobile_capture::release_images(
            &app.store,
            &ids.into_iter()
                .filter(|id| !keep.contains(id))
                .collect::<Vec<_>>(),
        )
    })
    .await
}
