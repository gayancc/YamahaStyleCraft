//! Lists the resources inside Yamaha Expansion Manager pack files (`.ppf`).
//!
//! Usage: ppf_list <pack.ppf> [more.ppf ...]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let package = genos_core::expansion::ppf::parse_ppf_file(std::path::Path::new(&path))?;
        println!("== {path} ({:?})", package.title);
        for resource in &package.resources {
            println!(
                "   {:<5} {:>9}  {}",
                resource
                    .extension
                    .as_deref()
                    .unwrap_or("-")
                    .trim_matches('\0'),
                resource.size,
                resource.title.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(())
}
