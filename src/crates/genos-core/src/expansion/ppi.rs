//! Pack Installation File (`.ppi` / `.pqi`) reader.
//!
//! Verified against `Genos_PackInstallData.pqi` (EV-0312–EV-0314):
//! - `XPIH` header with nested `XMDL` / `XCCC` / `XPID`
//! - `VWDL` / `VWST` high-entropy wave sections (encrypted / undecoded)
//! - `VPRS` voice-parameter section (structured, partially decoded)
//! - `PACK` content containers with nested `ETIT` / `BLOB` / `EEXT` / `FBIN`
//!   (MIDI `MThd` payloads verified recoverable)
//!
//! `.cpi` protected packs are out of scope until encryption is evidenced.

use super::iff::{self, Chunk};
use crate::Error;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_TOPLEVEL_CHUNKS: usize = 10_000;
const MAX_PACK_READ: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContentStatus {
    FullyDecoded,
    PartiallyRecovered,
    PresentUndecoded,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PpiHeader {
    pub model: Option<String>,
    pub charset: Option<String>,
    pub pack_id_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackEntry {
    pub title: Option<String>,
    pub offset: u64,
    pub size: u32,
    pub items: Vec<PackItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackItem {
    pub title: Option<String>,
    pub extension: Option<String>,
    pub icon: Option<String>,
    pub fbin_kind: String,
    pub fbin_size: u32,
    pub fbin_offset: u64,
    pub status: ContentStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionInfo {
    pub id: String,
    pub offset: u64,
    pub size: u32,
    pub status: ContentStatus,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PpiPackage {
    pub path: String,
    pub file_size: u64,
    pub sha256: Option<String>,
    pub header: PpiHeader,
    pub sections: Vec<SectionInfo>,
    pub packs: Vec<PackEntry>,
    pub diagnostics: Vec<String>,
}

pub fn parse_ppi(path: &Path) -> Result<PpiPackage, Error> {
    let meta = std::fs::metadata(path)?;
    let file_size = meta.len();
    if file_size > MAX_FILE_BYTES {
        return Err(Error::Format(format!(
            "installation file exceeds {MAX_FILE_BYTES} byte safety limit"
        )));
    }
    let mut file = File::open(path)?;
    let toplevel = iff::map_toplevel(&mut file, file_size, MAX_TOPLEVEL_CHUNKS)?;
    if toplevel.first().map(|c| c.id.as_str()) != Some("XPIH") {
        return Err(Error::Format(
            "installation file missing XPIH magic (not a .ppi/.pqi header)".into(),
        ));
    }

    let mut diagnostics = Vec::new();
    let header = read_header(&mut file, &toplevel[0])?;
    let mut sections = Vec::new();
    let mut packs = Vec::new();

    for chunk in &toplevel {
        match chunk.id.as_str() {
            "XPIH" => sections.push(SectionInfo {
                id: chunk.id.clone(),
                offset: chunk.offset,
                size: chunk.size,
                status: ContentStatus::FullyDecoded,
                note: "Installation header (model/charset/id)".into(),
            }),
            "VWDL" => sections.push(SectionInfo {
                id: chunk.id.clone(),
                offset: chunk.offset,
                size: chunk.size,
                status: ContentStatus::PresentUndecoded,
                note:
                    "Wave directory — high entropy; IO::Secure::encryptbuf suspected (EV-0312/0313)"
                        .into(),
            }),
            "VWST" => sections.push(SectionInfo {
                id: chunk.id.clone(),
                offset: chunk.offset,
                size: chunk.size,
                status: ContentStatus::PresentUndecoded,
                note: "Wave store — high entropy; original samples not decoded yet (EV-0312)"
                    .into(),
            }),
            "VPRS" => sections.push(SectionInfo {
                id: chunk.id.clone(),
                offset: chunk.offset,
                size: chunk.size,
                status: ContentStatus::PartiallyRecovered,
                note: "Voice parameter region present; full field map incomplete (EV-0314)".into(),
            }),
            "PACK" => {
                sections.push(SectionInfo {
                    id: chunk.id.clone(),
                    offset: chunk.offset,
                    size: chunk.size,
                    status: ContentStatus::PartiallyRecovered,
                    note: "Content pack container".into(),
                });
                match read_pack(&mut file, chunk) {
                    Ok(entry) => packs.push(entry),
                    Err(e) => diagnostics.push(format!("PACK @{}: {e}", chunk.offset)),
                }
            }
            other => {
                sections.push(SectionInfo {
                    id: other.to_string(),
                    offset: chunk.offset,
                    size: chunk.size,
                    status: ContentStatus::Unknown,
                    note: "Unrecognized top-level section".into(),
                });
                diagnostics.push(format!("unknown section {other} @{}", chunk.offset));
            }
        }
    }

    let sha256 = if file_size <= 64 * 1024 * 1024 {
        let mut data = Vec::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_end(&mut data)?;
        Some(hex_sha256(&data))
    } else {
        None
    };

    Ok(PpiPackage {
        path: path.display().to_string(),
        file_size,
        sha256,
        header,
        sections,
        packs,
        diagnostics,
    })
}

fn read_header(file: &mut File, xpih: &Chunk) -> Result<PpiHeader, Error> {
    let mut buf = vec![0u8; xpih.size as usize + 8];
    file.seek(SeekFrom::Start(xpih.offset))?;
    file.read_exact(&mut buf)?;
    let nested = iff::parse_chunks(&buf, 8, 8 + xpih.size as usize, 0)?;
    Ok(PpiHeader {
        model: nested
            .iter()
            .find(|c| c.id == "XMDL")
            .and_then(|c| iff::text_payload(&buf, c).ok()),
        charset: nested
            .iter()
            .find(|c| c.id == "XCCC")
            .and_then(|c| iff::text_payload(&buf, c).ok()),
        pack_id_hex: nested.iter().find(|c| c.id == "XPID").and_then(|c| {
            iff::payload(&buf, c)
                .ok()
                .map(|p| p.iter().map(|b| format!("{b:02X}")).collect())
        }),
    })
}

fn read_pack(file: &mut File, chunk: &Chunk) -> Result<PackEntry, Error> {
    if chunk.size as u64 > MAX_PACK_READ {
        return Err(Error::Format(format!(
            "PACK size {} exceeds read limit",
            chunk.size
        )));
    }
    let mut buf = vec![0u8; chunk.size as usize + 8];
    file.seek(SeekFrom::Start(chunk.offset))?;
    file.read_exact(&mut buf)?;
    let tree = iff::parse_chunks(&buf, 0, buf.len(), 0)?;
    let pack = tree
        .first()
        .ok_or_else(|| Error::Format("empty PACK".into()))?;
    let title = pack
        .children
        .iter()
        .find(|c| c.id == "ETIT")
        .and_then(|c| iff::text_payload(&buf, c).ok());
    let mut items = Vec::new();
    collect_items(&buf, &pack.children, chunk.offset, &mut items)?;
    Ok(PackEntry {
        title,
        offset: chunk.offset,
        size: chunk.size,
        items,
    })
}

fn collect_items(
    buf: &[u8],
    chunks: &[Chunk],
    file_base: u64,
    out: &mut Vec<PackItem>,
) -> Result<(), Error> {
    for c in chunks {
        if c.id == "BLOB" {
            let mut title = None;
            let mut extension = None;
            let mut icon = None;
            let mut fbin: Option<&Chunk> = None;
            for child in &c.children {
                match child.id.as_str() {
                    "ETIT" => title = iff::text_payload(buf, child).ok(),
                    "EEXT" => extension = iff::text_payload(buf, child).ok(),
                    "EICO" => icon = iff::text_payload(buf, child).ok(),
                    "FBIN" => fbin = Some(child),
                    _ => {}
                }
            }
            if let Some(fb) = fbin {
                let payload = iff::payload(buf, fb)?;
                let (fbin_kind, status) = classify_fbin(payload);
                out.push(PackItem {
                    title,
                    extension,
                    icon,
                    fbin_kind,
                    fbin_size: fb.size,
                    fbin_offset: file_base + fb.data_offset,
                    status,
                });
            }
        }
        if !c.children.is_empty() {
            collect_items(buf, &c.children, file_base, out)?;
        }
    }
    Ok(())
}

fn classify_fbin(payload: &[u8]) -> (String, ContentStatus) {
    if payload.starts_with(b"MThd") {
        ("midi".into(), ContentStatus::FullyDecoded)
    } else if payload.starts_with(b"<?xml") {
        ("xml".into(), ContentStatus::FullyDecoded)
    } else if payload.starts_with(b"FORM") {
        ("aiff".into(), ContentStatus::FullyDecoded)
    } else {
        ("binary".into(), ContentStatus::PartiallyRecovered)
    }
}

fn hex_sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:X}", h.finalize())
}

pub fn extract_pack_item(
    path: &Path,
    item: &PackItem,
    dest_dir: &Path,
    file_name: &str,
) -> Result<std::path::PathBuf, Error> {
    let name = Path::new(file_name)
        .file_name()
        .ok_or_else(|| Error::Format("invalid extract name".into()))?;
    std::fs::create_dir_all(dest_dir)?;
    let buf = read_pack_item_bytes(path, item)?;
    let out = dest_dir.join(name);
    std::fs::write(&out, buf)?;
    Ok(out)
}

/// Read a PACK item FBIN payload into memory (bounded by item size).
pub fn read_pack_item_bytes(path: &Path, item: &PackItem) -> Result<Vec<u8>, Error> {
    const MAX_ITEM: u32 = 8 * 1024 * 1024;
    if item.fbin_size > MAX_ITEM {
        return Err(Error::Format(format!(
            "pack item exceeds {MAX_ITEM} byte safety limit"
        )));
    }
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(item.fbin_offset))?;
    let mut buf = vec![0u8; item.fbin_size as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mini_install_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/expansion-fixtures/mini-install.ppi");
        if !path.exists() {
            return;
        }
        let pkg = parse_ppi(&path).unwrap();
        assert_eq!(pkg.header.model.as_deref(), Some("Genos"));
        assert!(!pkg.packs.is_empty());
        assert!(pkg.packs[0].items.iter().any(|i| i.fbin_kind == "midi"));
    }
}
