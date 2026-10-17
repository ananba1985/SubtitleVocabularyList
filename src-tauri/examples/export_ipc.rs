fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = subtitle_vocabulary_list_core::export_ipc_types();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/generated/ipc.ts");
    match std::env::args().nth(1).as_deref() {
        Some("--write") => {
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, output)?;
            println!("Generated {}", path.display());
        }
        Some("--check") => {
            let saved = std::fs::read_to_string(&path)?;
            if saved.replace("\r\n", "\n") != output {
                return Err("IPC types are stale. Run pnpm commands:generate and commit src/generated/ipc.ts.".into());
            }
            println!("Rust IPC signatures, nested DTOs and generated response fields are aligned.");
        }
        _ => return Err("Expected --write or --check".into()),
    }
    Ok(())
}
