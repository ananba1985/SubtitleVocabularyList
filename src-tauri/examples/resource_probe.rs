//! Validates bundled tools with a private fixture, without global tool discovery.
use std::{path::PathBuf, sync::atomic::AtomicBool};
use subtitle_vocabulary_list_core::{
    media::{self, ImportOptions, MediaTools},
    ocr,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("Usage: resource_probe RESOURCE_ROOT VIDEO OCR_IMAGE PRIVATE_OUTPUT".into());
    }
    let root = std::fs::canonicalize(&args[1])?;
    let tools = MediaTools::bundled(root);
    let cancel = AtomicBool::new(false);
    let text = ocr::recognize(&tools.tesseract, &PathBuf::from(&args[3]), &cancel)?;
    let input = PathBuf::from(&args[2]);
    let output = PathBuf::from(&args[4]);
    let imported = media::import_media_with_options(
        &tools,
        &input,
        &output,
        &ImportOptions {
            subtitle_mode: "speech".into(),
            ..Default::default()
        },
        &cancel,
        &|_, _, _| {},
    )?;
    println!(
        "{}",
        serde_json::json!({"ocr":text,"segments":imported.segments.len(),"textSource":imported.text_source,"durationMs":imported.duration_ms})
    );
    Ok(())
}
