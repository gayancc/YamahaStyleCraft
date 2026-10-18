//! Voice parameter region (`VPRS`) reader (EV-0314 / EV-0317).
//!
//! Genos installation `VPRS` payloads begin with a cleartext ASCII banner
//! `UTG VPRM …` (32 bytes, NUL-padded), followed by four little-endian u32
//! header fields. The bulk of early payload is `0xFF` fill; denser binary
//! voice-parameter records begin near the fourth header value (`data_base`).
//! Per-voice field maps remain incomplete.

use crate::Error;
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAX_VPRS_READ: u64 = 16 * 1024 * 1024;
const BANNER_LEN: usize = 32;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VprsHeader {
    /// First LE u32 after the banner (role unknown; observed non-zero).
    pub field0: u32,
    /// Second LE u32 (observed `0x30` on Genos sample).
    pub field1: u32,
    /// Third LE u32 (observed `0x4030` on Genos sample).
    pub field2: u32,
    /// Fourth LE u32 — aligns with start of denser record region (`data_base`).
    pub data_base: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VprsInfo {
    pub offset: u64,
    pub size: u32,
    pub banner: Option<String>,
    pub banner_hex: String,
    pub header: Option<VprsHeader>,
    pub record_count_estimate: Option<u32>,
    pub note: String,
}

/// Read and summarize a `VPRS` section from an installation file.
pub fn summarize_vprs(path: &Path, offset: u64, size: u32) -> Result<VprsInfo, Error> {
    if size as u64 > MAX_VPRS_READ {
        return Err(Error::Format(format!(
            "VPRS size {size} exceeds {MAX_VPRS_READ} byte safety limit"
        )));
    }
    let mut file = File::open(path)?;
    let payload_off = offset.saturating_add(8);
    let payload_len = size as usize;
    file.seek(SeekFrom::Start(payload_off))?;
    let read_len = payload_len.min(64 * 1024);
    let mut buf = vec![0u8; read_len];
    let n = file.read(&mut buf)?;
    buf.truncate(n);

    let banner = extract_banner(&buf);
    let banner_hex = buf
        .iter()
        .take(64)
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join("");
    let header = parse_header(&buf);

    let record_count_estimate = header.as_ref().and_then(|h| {
        let base = h.data_base as usize;
        if base < payload_len {
            // Opaque stride estimate once denser region starts; not a verified record size.
            Some(((payload_len - base) / 256) as u32)
        } else {
            None
        }
    });

    Ok(VprsInfo {
        offset,
        size,
        banner,
        banner_hex,
        header,
        record_count_estimate,
        note: "Banner + 4×u32 LE header verified; per-voice field map still incomplete (EV-0317)"
            .into(),
    })
}

fn parse_header(buf: &[u8]) -> Option<VprsHeader> {
    if buf.len() < BANNER_LEN + 16 || !buf.starts_with(b"UTG ") {
        return None;
    }
    let field0 = u32::from_le_bytes(buf[BANNER_LEN..BANNER_LEN + 4].try_into().ok()?);
    let field1 = u32::from_le_bytes(buf[BANNER_LEN + 4..BANNER_LEN + 8].try_into().ok()?);
    let field2 = u32::from_le_bytes(buf[BANNER_LEN + 8..BANNER_LEN + 12].try_into().ok()?);
    let data_base = u32::from_le_bytes(buf[BANNER_LEN + 12..BANNER_LEN + 16].try_into().ok()?);
    Some(VprsHeader {
        field0,
        field1,
        field2,
        data_base,
    })
}

fn extract_banner(buf: &[u8]) -> Option<String> {
    if buf.len() < 8 {
        return None;
    }
    if !buf.starts_with(b"UTG ") {
        return None;
    }
    let end = buf[..BANNER_LEN.min(buf.len())]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(BANNER_LEN.min(buf.len()));
    let s = String::from_utf8_lossy(&buf[..end]).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_utg_banner() {
        let sample = b"UTG VPRM 25 12 01 10 29 A     0 \x00rest";
        assert_eq!(
            extract_banner(sample).as_deref(),
            Some("UTG VPRM 25 12 01 10 29 A     0")
        );
    }

    #[test]
    fn parses_le_header_fields() {
        let mut buf = vec![0u8; 48];
        buf[..31].copy_from_slice(b"UTG VPRM 25 12 01 10 29 A     0");
        buf[32..36].copy_from_slice(&0x002b_dfb4u32.to_le_bytes());
        buf[36..40].copy_from_slice(&0x30u32.to_le_bytes());
        buf[40..44].copy_from_slice(&0x4030u32.to_le_bytes());
        buf[44..48].copy_from_slice(&0xa930u32.to_le_bytes());
        let h = parse_header(&buf).unwrap();
        assert_eq!(h.field0, 0x002b_dfb4);
        assert_eq!(h.field1, 0x30);
        assert_eq!(h.field2, 0x4030);
        assert_eq!(h.data_base, 0xa930);
    }
}
