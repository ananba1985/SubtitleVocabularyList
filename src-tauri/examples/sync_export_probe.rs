//! Exports a private inspection packet; never print its contents to logs.
use subtitle_vocabulary_list::store::Store;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Usage: sync_export_probe PRIVATE_STORE ENTRY_ID PRIVATE_OUTPUT_JSON".into());
    }
    let store = Store::open(&args[1])?;
    let bundle = store.sync_export(&args[2])?;
    std::fs::write(&args[3], serde_json::to_vec(&bundle)?)?;
    println!(
        "Exported schema {} with {} related sections",
        bundle.schema_version,
        bundle.tables.len()
    );
    Ok(())
}
