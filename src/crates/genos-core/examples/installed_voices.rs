//! Lists voices and drum kits from a Yamaha Expansion Manager installation file
//! (`*_PackInstallData.pqi` / `.ppi`) with their keyboard addresses.
//!
//! Usage: installed_voices <installation file> [name filter, case-insensitive]
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .ok_or("usage: installed_voices <pqi> [filter]")?;
    let filter = args.get(1).map(|f| f.to_lowercase());
    let (voices, skipped) = genos_core::expansion::installed_voices(Path::new(path))?;
    for voice in voices.iter().filter(|v| {
        filter
            .as_ref()
            .is_none_or(|f| v.name.to_lowercase().contains(f) || v.pack.to_lowercase().contains(f))
    }) {
        println!(
            "{:<4} {:>3} {:>3} {:>3}  {:<28} {}",
            if voice.kit { "kit" } else { "vce" },
            voice.msb,
            voice.lsb,
            voice.program,
            voice.name,
            voice.pack
        );
    }
    eprintln!("{} installed items, {skipped} skipped", voices.len());
    Ok(())
}
