//! Rebuild the embedded voice catalog from the fingerprinted Genos Data List.
//! cargo run -p genos-core --example build_factory_voices -- <pdf> <output-json>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("Expected Data List PDF path and output JSON path".into());
    }
    let catalog =
        genos_core::voice_catalog::import_pdf(&std::fs::read(&args[0])?, "genos_en_dl_f0.pdf")?;
    let output = std::path::Path::new(&args[1]);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    genos_core::voice_catalog::save(output, &catalog)?;
    println!("FACTORY VOICES GENERATED: {} entries", catalog.voices.len());
    Ok(())
}
