//! Voice parameter region (`VPRS`) reader (EV-0314 / EV-0317).
//!
//! The top-level `VPRS` payload on Genos installation files begins with a
//! cleartext ASCII banner `UTG VPRM …` followed by binary voice records.
//! Full per-voice field maps remain incomplete; this module exposes the
//! verified header and coarse record framing only.

use crate::Error;
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAX_VPRS_READ: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VprsInfo {
    pub offset: u64,
    pub size: u32,
    pub banner: Option<String>,
    pub banner_hex: String,
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
    // payload begins after FourCC+size
    let payload_off = offset.saturating_add(8);
    let payload_len = size as usize;
    file.seek(SeekFrom::Start(payload_off))?;
    let mut buf = vec![0u8; payload_len.min(4096)];
    let n = file.read(&mut buf)?;
    buf.truncate(n);

    let banner = extract_banner(&buf);
    let banner_hex = buf
        .iter()
        .take(64)
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join("");

    // After the 32-byte ASCII banner observed on Genos packs, records appear
    // to use a large stride; estimate only when payload is large enough.
    let record_count_estimate = if payload_len > 64 {
        // Conservative: treat remaining bytes as opaque; expose size-based hint.
        Some(((payload_len - 64) / 256) as u32)
    } else {
        None
    };

    Ok(VprsInfo {
        offset,
        size,
        banner,
        banner_hex,
        record_count_estimate,
        note: "Banner verified cleartext; per-voice field map still incomplete (EV-0317)".into(),
    })
}

fn extract_banner(buf: &[u8]) -> Option<String> {
    if buf.len() < 8 {
        return None;
    }
    if !buf.starts_with(b"UTG ") {
        return None;
    }
    let end = buf
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(buf.len().min(64));
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
}
