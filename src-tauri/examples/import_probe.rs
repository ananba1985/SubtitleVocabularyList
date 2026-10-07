use std::{path::PathBuf, sync::atomic::AtomicBool};
use subtitle_vocabulary_list::media::{MediaTools, extract_clip, import_media};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    if arguments.len() < 4 {
        return Err("Usage: import_probe INPUT OUTPUT_DIRECTORY RUNTIME_DIRECTORY [speech]".into());
    }
    let input = PathBuf::from(&arguments[1]);
    let output = PathBuf::from(&arguments[2]);
    let runtime = PathBuf::from(&arguments[3]);
    let tools = MediaTools::development(runtime);
    let cancelled = AtomicBool::new(false);
    let last_logged = std::cell::Cell::new(0usize);
    let progress = |stage: &str, current: usize, total: usize| {
        if current == 0 || current == total || current >= last_logged.get().saturating_add(50) {
            println!("{stage}: {current}/{total}");
            last_logged.set(current);
        }
    };
    let started = std::time::Instant::now();
    let result = import_media(
        &tools,
        &input,
        &output,
        arguments.get(4).is_some_and(|mode| mode == "speech"),
        &cancelled,
        &progress,
    )?;
    println!(
        "ready: {} segments; source={}; duration={}ms; elapsed={:.2}s",
        result.segments.len(),
        result.text_source,
        result.duration_ms,
        started.elapsed().as_secs_f64()
    );
    println!("warnings: {}", result.warnings.len());
    if let Some(segment) = result.segments.first() {
        let duration = extract_clip(
            &tools,
            &result.audio_path,
            &output.join("first-dialogue.m4a"),
            segment.start_ms,
            segment.end_ms,
            &cancelled,
        )?;
        println!(
            "first saved dialogue: {}..{}ms; clip={}ms",
            segment.start_ms, segment.end_ms, duration
        );
    }
    Ok(())
}
