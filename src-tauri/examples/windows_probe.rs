use std::{path::PathBuf, sync::atomic::AtomicBool};
use subtitle_vocabulary_list::windows_native::{
    foreground_origin, selected_text, synthesize, system_voices,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    match arguments.get(1).map(String::as_str) {
        Some("screen-crop") => {
            let output = PathBuf::from(arguments.get(2).ok_or("provide a crop output path")?);
            let numbers: Vec<u32> = arguments
                .iter()
                .skip(3)
                .map(|value| value.parse())
                .collect::<Result<_, _>>()?;
            if numbers.len() != 4 {
                return Err("provide x y width height in physical pixels".into());
            }
            let snapshot = output.with_extension("snapshot.tmp.png");
            let result = (|| {
                let bounds = subtitle_vocabulary_list::windows_native::capture_screen(&snapshot)?;
                let rect = subtitle_vocabulary_list::ocr::PixelRect {
                    x: numbers[0],
                    y: numbers[1],
                    width: numbers[2],
                    height: numbers[3],
                };
                rect.validate(&bounds)?;
                let image = image::open(&snapshot)?;
                image
                    .crop_imm(rect.x, rect.y, rect.width, rect.height)
                    .save(&output)?;
                println!("{}", serde_json::to_string(&bounds)?);
                Ok::<_, Box<dyn std::error::Error>>(())
            })();
            let _ = std::fs::remove_file(snapshot);
            result?;
        }
        Some("voices") => println!("{}", serde_json::to_string_pretty(&system_voices()?)?),
        Some("speech") => {
            let path = PathBuf::from(arguments.get(2).ok_or("provide a WAV output path")?);
            let text = arguments
                .get(3)
                .map(String::as_str)
                .unwrap_or("I was reluctant to ask for help.");
            println!(
                "{}",
                serde_json::to_string(&synthesize(text, "", &path, &AtomicBool::new(false))?)?
            );
            println!("saved {} bytes", std::fs::metadata(path)?.len());
        }
        Some("selection") => {
            let origin = foreground_origin()?;
            if let Some(expected) = arguments.get(2)
                && origin.title != *expected
            {
                return Err("The expected fixture is not foreground; no text read.".into());
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&selected_text(&origin)?)?
            );
        }
        Some("speech-cancel") => {
            let path = PathBuf::from(arguments.get(2).ok_or("provide a WAV output path")?);
            let signal = std::sync::Arc::new(AtomicBool::new(false));
            let cancellation = std::sync::Arc::clone(&signal);
            let progress_file = path.clone();
            let thread = std::thread::spawn(move || {
                let started = std::time::Instant::now();
                loop {
                    if std::fs::metadata(&progress_file).is_ok_and(|file| file.len() > 65536)
                        || started.elapsed() > std::time::Duration::from_secs(3)
                    {
                        cancellation.store(true, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });
            let outcome = synthesize(
                &"I was reluctant to ask for help. ".repeat(500),
                "",
                &path,
                &signal,
            );
            thread.join().map_err(|_| "cancellation probe failed")?;
            match outcome {
                Err(error) if error.code == "cancelled" => {
                    println!("cancelled after audio generation began; partial output is disposable")
                }
                other => return Err(format!("unexpected cancellation result: {other:?}").into()),
            }
        }
        _ => {
            return Err(
                "Usage: windows_probe voices | speech OUTPUT.wav [TEXT] | selection".into(),
            );
        }
    }
    Ok(())
}
