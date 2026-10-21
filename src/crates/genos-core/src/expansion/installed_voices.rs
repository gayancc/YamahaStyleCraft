//! Every voice and drum kit recorded in a Yamaha Expansion Manager installation file
//! (`<Model>_PackInstallData.pqi` / `.ppi`).
//!
//! Each installed item carries a title, an extension (`vce` voice / `drm` drum kit) and a small
//! SMF with the Bank Select + Program Change the keyboard assigned it, so the whole installed
//! library can be listed with real names — no per-voice typing.

use super::ppi::{self, PackItem};
use super::voice_map::scan_midi_voice_addresses;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVoice {
    pub pack: String,
    pub name: String,
    pub kit: bool,
    pub msb: u8,
    pub lsb: u8,
    pub program: u8,
}

fn is_kit(item: &PackItem) -> bool {
    item.extension
        .as_deref()
        .is_some_and(|e| e.trim_matches('\0').eq_ignore_ascii_case("drm"))
}

/// Read all installed voices/kits. Items whose setup data cannot be decoded are skipped and
/// counted in the second return value rather than failing the whole scan.
pub fn installed_voices(path: &Path) -> Result<(Vec<InstalledVoice>, usize), Error> {
    let package = ppi::parse_ppi(path)?;
    let mut seen = HashSet::new();
    let mut voices = Vec::new();
    let mut skipped = 0usize;
    for pack in &package.packs {
        let pack_name = pack.title.clone().unwrap_or_default();
        for item in pack.items.iter().filter(|i| i.fbin_kind == "midi") {
            let addresses = ppi::read_pack_item_bytes(path, item)
                .and_then(|bytes| scan_midi_voice_addresses(&bytes));
            let Ok(addresses) = addresses else {
                skipped += 1;
                continue;
            };
            let name = item
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| "Unnamed".into());
            for a in addresses {
                if seen.insert((a.msb, a.lsb, a.program)) {
                    voices.push(InstalledVoice {
                        pack: pack_name.clone(),
                        name: name.trim().to_string(),
                        kit: is_kit(item),
                        msb: a.msb,
                        lsb: a.lsb,
                        program: a.program,
                    });
                }
            }
        }
    }
    Ok((voices, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set `GENOS_PQI_TEST` to a real installation file to exercise the full scan.
    #[test]
    fn lists_voices_from_a_real_installation_file() {
        let Ok(path) = std::env::var("GENOS_PQI_TEST") else {
            return;
        };
        let (voices, skipped) = installed_voices(Path::new(&path)).unwrap();
        eprintln!("{} voices, {skipped} skipped", voices.len());
        assert!(!voices.is_empty());
        assert!(voices.iter().all(|v| !v.name.is_empty()));
    }
}
