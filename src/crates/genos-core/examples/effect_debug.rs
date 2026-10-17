fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: effect_debug <Genos Data List PDF>")?;
    let bytes = std::fs::read(&path)?;
    let catalog = genos_core::effect_catalog::import_pdf(&bytes, "Genos Data List")?;
    println!("{}", serde_json::to_string_pretty(&catalog.effects)?);
    Ok(())
}
