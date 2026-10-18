//! Genos Instrument Information (`.n27`).
//!
//! Layout verified from `docs/research/Genos_InstrumentInfo.n27` (144 bytes,
//! SHA-256 C61E687E…FAB610F0) and correlated with YEM `Instruments.ini` (EV-0315):
//! - `0x00..0x40`: `ModelName` NUL-padded ASCII (e.g. `"Genos"`)
//! - `0x40..0x58`: `SerialNumber` NUL-padded (e.g. `"I73K1827423"`)
//! - `0x58..0x7A`: `ID` composite (e.g. `"3650_I73K1827423"`)
//! - `0x84..0x88`: `PackInstallDataIdentification` u32 BE
//! - `0x88..0x8C`: `PackInstallDataFormatType` u32 BE (observed `3`)
//! - remaining trailing bytes: capacity / ROM fields — partially unknown
//!
//! Role: host targeting and pack-installation generation constraints.
//! Not established as a `.ppi` decryption key for VWST/VWDL.

use crate::Error;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const N27_SIZE: usize = 144;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentInfo {
    pub model_name: String,
    pub instrument_id: String,
    pub composite_id: String,
    pub pack_install_data_identification: u32,
    pub pack_install_data_format_type: u32,
    /// Bytes `0x7A..0x90` as hex for unresolved fields.
    pub trailing_hex: String,
    pub sha256: String,
    pub size: usize,
}

fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub fn parse_n27(data: &[u8]) -> Result<InstrumentInfo, Error> {
    if data.len() != N27_SIZE {
        return Err(Error::Format(format!(
            ".n27 expected {N27_SIZE} bytes, got {}",
            data.len()
        )));
    }
    let model_name = c_string(&data[0x00..0x40]);
    if model_name.is_empty() {
        return Err(Error::Format(".n27 missing model name at 0x00".into()));
    }
    let instrument_id = c_string(&data[0x40..0x58]);
    let composite_id = c_string(&data[0x58..0x7A]);
    let pack_install_data_identification = u32::from_be_bytes(data[0x84..0x88].try_into().unwrap());
    let pack_install_data_format_type = u32::from_be_bytes(data[0x88..0x8C].try_into().unwrap());
    let trailing = &data[0x7A..0x90];
    let trailing_hex = trailing
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join("");
    let sha256 = hex_sha256(data);
    Ok(InstrumentInfo {
        model_name,
        instrument_id,
        composite_id,
        pack_install_data_identification,
        pack_install_data_format_type,
        trailing_hex,
        sha256,
        size: data.len(),
    })
}

pub fn parse_n27_file(path: &std::path::Path) -> Result<InstrumentInfo, Error> {
    let data = std::fs::read(path)?;
    parse_n27(&data)
}

fn hex_sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:X}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_n27() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/Genos_InstrumentInfo.n27");
        if !path.exists() {
            // Workspace layout may differ in CI; skip silently.
            return;
        }
        let info = parse_n27_file(&path).unwrap();
        assert_eq!(info.model_name, "Genos");
        assert_eq!(info.instrument_id, "I73K1827423");
        assert!(info.composite_id.starts_with("3650_"));
        assert_eq!(info.pack_install_data_identification, 1361092261);
        assert_eq!(info.pack_install_data_format_type, 3);
        assert_eq!(info.size, 144);
    }
}
