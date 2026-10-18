//! Pack Project File (`.ppf`) reader.
//!
//! Verified against ConvUVx `/univ` fixture (EV-0311):
//! - Root marker `XPFH` (size 0)
//! - Pack title `ETIT`
//! - Content entries as `BLOB` containers with nested `ETIT` / `EEXT` / `FBIN`
//! - Voice definitions: `EEXT=uvf`, `FBIN` = UVF XML
//! - Waveforms: `EEXT=APCp`, `FBIN` = AIFF `FORM` payloads (lossless extractable)

use super::iff::{self, Chunk, ChunkTree};
use crate::Error;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResourceKind {
    UvfXml,
    Aiff,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PpfResource {
    pub title: Option<String>,
    pub extension: Option<String>,
    pub kind: ResourceKind,
    pub size: u32,
    pub data_offset: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PpfPackage {
    pub title: Option<String>,
    pub sha256: String,
    pub file_size: u64,
    pub tree: ChunkTree,
    pub resources: Vec<PpfResource>,
}

pub fn parse_ppf(data: &[u8]) -> Result<PpfPackage, Error> {
    if data.len() < 8 {
        return Err(Error::Format(".ppf too small".into()));
    }
    if &data[0..4] != b"XPFH" {
        return Err(Error::Format(format!(
            ".ppf missing XPFH magic, found {:?}",
            String::from_utf8_lossy(&data[0..4])
        )));
    }
    let tree = iff::parse_file(data)?;
    let title = tree
        .chunks
        .iter()
        .find(|c| c.id == "ETIT")
        .map(|c| iff::text_payload(data, c))
        .transpose()?;
    let mut resources = Vec::new();
    collect_resources(data, &tree.chunks, &mut resources)?;
    Ok(PpfPackage {
        title,
        sha256: hex_sha256(data),
        file_size: data.len() as u64,
        tree,
        resources,
    })
}

pub fn parse_ppf_file(path: &std::path::Path) -> Result<PpfPackage, Error> {
    const MAX_PPF: u64 = 512 * 1024 * 1024;
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_PPF {
        return Err(Error::Format(format!(
            ".ppf exceeds {MAX_PPF} byte safety limit"
        )));
    }
    let data = std::fs::read(path)?;
    parse_ppf(&data)
}

fn collect_resources(
    data: &[u8],
    chunks: &[Chunk],
    out: &mut Vec<PpfResource>,
) -> Result<(), Error> {
    let mut i = 0;
    while i < chunks.len() {
        let c = &chunks[i];
        if c.id == "BLOB" && !c.children.is_empty() {
            let mut title = None;
            let mut extension = None;
            let mut fbin: Option<&Chunk> = None;
            for child in &c.children {
                match child.id.as_str() {
                    "ETIT" => title = Some(iff::text_payload(data, child)?),
                    "EEXT" => extension = Some(iff::text_payload(data, child)?),
                    "FBIN" => fbin = Some(child),
                    _ => {}
                }
            }
            if let Some(fb) = fbin {
                let payload = iff::payload(data, fb)?;
                let kind = classify(&extension, payload);
                out.push(PpfResource {
                    title,
                    extension,
                    kind,
                    size: fb.size,
                    data_offset: fb.data_offset,
                    sha256: hex_sha256(payload),
                });
            }
        }
        if !c.children.is_empty() {
            collect_resources(data, &c.children, out)?;
        }
        i += 1;
    }
    Ok(())
}

fn classify(ext: &Option<String>, payload: &[u8]) -> ResourceKind {
    if payload.starts_with(b"<?xml") || ext.as_deref() == Some("uvf") {
        ResourceKind::UvfXml
    } else if payload.starts_with(b"FORM") || ext.as_deref() == Some("APCp") {
        ResourceKind::Aiff
    } else if ext.is_some() {
        ResourceKind::Other
    } else {
        ResourceKind::Unknown
    }
}

fn hex_sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:X}", h.finalize())
}

/// Extract resource bytes by index into `dest` directory (non-destructive).
pub fn extract_resource(
    data: &[u8],
    resource: &PpfResource,
    dest: &std::path::Path,
    file_name: &str,
) -> Result<std::path::PathBuf, Error> {
    let start = resource.data_offset as usize;
    let end = start
        .checked_add(resource.size as usize)
        .ok_or_else(|| Error::Format("extract overflow".into()))?;
    if end > data.len() {
        return Err(Error::Format("extract out of bounds".into()));
    }
    // Path traversal guard
    let name = std::path::Path::new(file_name)
        .file_name()
        .ok_or_else(|| Error::Format("invalid extract name".into()))?;
    std::fs::create_dir_all(dest)?;
    let path = dest.join(name);
    std::fs::write(&path, &data[start..end])?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_univ_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/expansion-fixtures/univ-tiny.ppf");
        if !path.exists() {
            return;
        }
        let data = std::fs::read(&path).unwrap();
        let pkg = parse_ppf(&data).unwrap();
        assert_eq!(pkg.title.as_deref(), Some("univ-tiny"));
        assert!(pkg.resources.iter().any(|r| r.kind == ResourceKind::UvfXml));
        assert!(pkg.resources.iter().any(|r| r.kind == ResourceKind::Aiff));
    }
}
