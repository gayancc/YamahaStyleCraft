//! YEM local wave-index XML (`InstallTargets/.../.waves`) helper (EV-0318).
//!
//! When Expansion Manager has already resolved an installation for a host, it
//! writes a `<waves>` catalog mapping `wavekey` → byte `startaddress` inside
//! the encrypted `VWST` payload (`frameaddress << 4 == startaddress`).
//!
//! This does not decrypt samples by itself; it supplies offsets for keyed SME
//! decrypt once the offline 16-byte SME seed is available. `wavekey` is the
//! SHA-1 of the source FORM (EV-0318).

use crate::Error;
use serde::Serialize;
use std::path::Path;

const MAX_WAVE_INDEX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveIndexEntry {
    pub wave_key: String,
    pub pack_key: Option<String>,
    pub start_address: u64,
    pub frame_address: u64,
    pub wave_size: u64,
    pub wave_type: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveIndex {
    pub path: String,
    pub version: Option<String>,
    pub entries: Vec<WaveIndexEntry>,
    pub note: String,
}

pub fn parse_wave_index_file(path: &Path) -> Result<WaveIndex, Error> {
    let meta = std::fs::metadata(path).map_err(|e| Error::Format(format!("wave index: {e}")))?;
    if meta.len() > MAX_WAVE_INDEX_BYTES {
        return Err(Error::Format(format!(
            "wave index exceeds {MAX_WAVE_INDEX_BYTES} byte safety limit"
        )));
    }
    let text =
        std::fs::read_to_string(path).map_err(|e| Error::Format(format!("wave index: {e}")))?;
    parse_wave_index_xml(&text, path)
}

pub fn parse_wave_index_xml(text: &str, path: &Path) -> Result<WaveIndex, Error> {
    if !text.contains("<waves") {
        return Err(Error::Format("not a YEM <waves> index".into()));
    }
    let version = text
        .find("<waves")
        .and_then(|i| attr_value(&text[i..], "version"));
    let mut entries = Vec::new();
    for block in text.split("<file>").skip(1) {
        let body = block.split("</file>").next().unwrap_or("");
        let Some(wave_key) = tag_value(body, "wavekey") else {
            continue;
        };
        let start = parse_hex(tag_value(body, "startaddress").as_deref().unwrap_or("0"))?;
        let frame = parse_hex(tag_value(body, "frameaddress").as_deref().unwrap_or("0"))?;
        let size = parse_hex(tag_value(body, "wavesize").as_deref().unwrap_or("0"))?;
        let wave_type = tag_value(body, "wavetype")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if frame << 4 != start && frame != 0 {
            // Keep the entry but callers can validate; YEM catalogs observed so far match.
        }
        entries.push(WaveIndexEntry {
            wave_key,
            pack_key: tag_value(body, "packkey"),
            start_address: start,
            frame_address: frame,
            wave_size: size,
            wave_type,
        });
    }
    Ok(WaveIndex {
        path: path.display().to_string(),
        version,
        entries,
        note: "Offsets address encrypted VWST bytes; offline SME seed still required for PCM recovery (EV-0316/0318)".into(),
    })
}

fn tag_value(body: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&close)? + start;
    Some(body[start..end].trim().to_string())
}

fn attr_value(text: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = text.find(&key)? + key.len();
    let end = text[start..].find('"')? + start;
    Some(text[start..end].to_string())
}

fn parse_hex(s: &str) -> Result<u64, Error> {
    u64::from_str_radix(s.trim(), 16).map_err(|e| Error::Format(format!("invalid hex '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_minimal_waves_xml() {
        let xml = r#"<?xml version="1.0"?><waves version="1.4">
 <file>
  <wavekey>aabbccddeeff00112233445566778899aabbccdd</wavekey>
  <packkey>{744add46-ff43-41d4-9093-fdfd3fe942a8}</packkey>
  <wavetype>0</wavetype>
  <startaddress>100</startaddress>
  <frameaddress>10</frameaddress>
  <wavesize>20</wavesize>
 </file>
</waves>"#;
        let idx = parse_wave_index_xml(xml, Path::new("test.waves")).unwrap();
        assert_eq!(idx.version.as_deref(), Some("1.4"));
        assert_eq!(idx.entries.len(), 1);
        assert_eq!(idx.entries[0].start_address, 0x100);
        assert_eq!(
            idx.entries[0].frame_address << 4,
            idx.entries[0].start_address
        );
    }
}
