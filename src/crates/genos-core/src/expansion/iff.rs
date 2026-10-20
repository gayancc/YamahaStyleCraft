//! Big-endian Yamaha Expansion IFF-like chunk stream.
//!
//! Verified against ConvUVx-generated `.ppf` (EV-0311) and Genos `.pqi` (EV-0312):
//! each chunk is `FourCC (4) + u32 BE size (4) + payload[size]`, then padded to
//! a 4-byte boundary. The size field is the logical payload length.

use crate::Error;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    pub id: String,
    pub size: u32,
    pub offset: u64,
    pub data_offset: u64,
    pub aligned_end: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Chunk>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkTree {
    pub chunks: Vec<Chunk>,
    pub file_size: u64,
}

pub fn align4(n: u64) -> u64 {
    (n + 3) & !3
}

fn is_fourcc(id: &[u8; 4]) -> bool {
    id.iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}

/// Parse chunks from `data[start..end]`.
/// `base` is the absolute file offset corresponding to `data[0]`.
/// Nested children are detected when a payload begins with a valid FourCC and
/// an in-bounds size (EV-0311 BLOB nesting, EV-0312 XPIH nesting).
pub fn parse_chunks(data: &[u8], start: usize, end: usize, base: u64) -> Result<Vec<Chunk>, Error> {
    parse_chunks_bounded(data, start, end, base, 0, &mut 100_000)
}

fn parse_chunks_bounded(
    data: &[u8],
    start: usize,
    end: usize,
    base: u64,
    depth: usize,
    remaining: &mut usize,
) -> Result<Vec<Chunk>, Error> {
    if depth > 64 {
        return Err(Error::Format(
            "chunk nesting exceeds safety limit 64".into(),
        ));
    }
    if end > data.len() || start > end {
        return Err(Error::Format("chunk range out of bounds".into()));
    }
    let mut chunks = Vec::new();
    let mut o = start;
    while o + 8 <= end {
        let id_bytes: [u8; 4] = data[o..o + 4].try_into().unwrap();
        if data[o..end].iter().all(|&b| b == 0) {
            break;
        }
        if !is_fourcc(&id_bytes) {
            return Err(Error::Format(format!(
                "invalid chunk id at offset {}: {:?}",
                base + o as u64,
                String::from_utf8_lossy(&id_bytes)
            )));
        }
        if *remaining == 0 {
            return Err(Error::Format(
                "chunk count exceeds safety limit 100000".into(),
            ));
        }
        *remaining -= 1;
        let size = u32::from_be_bytes(data[o + 4..o + 8].try_into().unwrap());
        let data_start = o + 8;
        let data_end = data_start
            .checked_add(size as usize)
            .ok_or_else(|| Error::Format("chunk size overflow".into()))?;
        if data_end > end {
            return Err(Error::Format(format!(
                "chunk {} size {} overflows parent at offset {}",
                String::from_utf8_lossy(&id_bytes),
                size,
                base + o as u64
            )));
        }
        let id = String::from_utf8_lossy(&id_bytes).into_owned();
        let mut children = Vec::new();
        // Only recurse into known container IDs (EV-0311/0312). Opaque FBIN/FORM
        // payloads must not be forced through the chunk parser.
        if matches!(
            id.as_str(),
            "BLOB" | "XPIH" | "XPFH" | "PACK" | "PPI" | "PPF" | "VPRS"
        ) && size >= 8
        {
            let child_id: [u8; 4] = data[data_start..data_start + 4].try_into().unwrap();
            if is_fourcc(&child_id) {
                let child_size =
                    u32::from_be_bytes(data[data_start + 4..data_start + 8].try_into().unwrap());
                if (child_size as usize)
                    .checked_add(8)
                    .is_some_and(|n| n <= size as usize)
                {
                    // Keep the same `data` buffer so offsets stay valid for payload().
                    children = parse_chunks_bounded(
                        data,
                        data_start,
                        data_end,
                        base,
                        depth + 1,
                        remaining,
                    )?;
                }
            }
        }
        let abs_o = base + o as u64;
        let abs_data = base + data_start as u64;
        let abs_aligned = base + align4(data_end as u64);
        chunks.push(Chunk {
            id,
            size,
            offset: abs_o,
            data_offset: abs_data,
            aligned_end: abs_aligned,
            children,
        });
        let next = align4(data_end as u64) as usize;
        if next <= o {
            return Err(Error::Format("chunk cursor did not advance".into()));
        }
        o = next;
    }
    Ok(chunks)
}

pub fn parse_file(data: &[u8]) -> Result<ChunkTree, Error> {
    if data.len() as u64 > 512 * 1024 * 1024 {
        return Err(Error::Format("file exceeds 512 MiB safety limit".into()));
    }
    Ok(ChunkTree {
        chunks: parse_chunks(data, 0, data.len(), 0)?,
        file_size: data.len() as u64,
    })
}

/// Streaming top-level chunk map for large installation packages (EV-0312).
pub fn map_toplevel<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    file_size: u64,
    max_chunks: usize,
) -> Result<Vec<Chunk>, Error> {
    use std::io::SeekFrom;
    let mut chunks = Vec::new();
    let mut o = 0u64;
    let mut hdr = [0u8; 8];
    while o + 8 <= file_size {
        if chunks.len() >= max_chunks {
            return Err(Error::Format(format!(
                "toplevel chunk count exceeds limit {max_chunks} at offset {o}"
            )));
        }
        reader
            .seek(SeekFrom::Start(o))
            .map_err(|e| Error::Format(format!("seek {o}: {e}")))?;
        reader
            .read_exact(&mut hdr)
            .map_err(|e| Error::Format(format!("read header at {o}: {e}")))?;
        if hdr.iter().all(|&b| b == 0) {
            break;
        }
        let id_bytes: [u8; 4] = hdr[0..4].try_into().unwrap();
        if !is_fourcc(&id_bytes) {
            return Err(Error::Format(format!(
                "invalid chunk id at {o}: {:?}",
                String::from_utf8_lossy(&id_bytes)
            )));
        }
        let size = u32::from_be_bytes(hdr[4..8].try_into().unwrap());
        let data_offset = o + 8;
        let data_end = data_offset
            .checked_add(size as u64)
            .ok_or_else(|| Error::Format("chunk size overflow".into()))?;
        if data_end > file_size {
            return Err(Error::Format(format!(
                "chunk {} size {size} overflows file at {o}",
                String::from_utf8_lossy(&id_bytes)
            )));
        }
        let aligned_end = align4(data_end);
        chunks.push(Chunk {
            id: String::from_utf8_lossy(&id_bytes).into_owned(),
            size,
            offset: o,
            data_offset,
            aligned_end,
            children: Vec::new(),
        });
        if aligned_end <= o {
            return Err(Error::Format("chunk cursor did not advance".into()));
        }
        o = aligned_end;
    }
    Ok(chunks)
}

pub fn text_payload(data: &[u8], chunk: &Chunk) -> Result<String, Error> {
    let raw = payload(data, chunk)?;
    let end_trim = raw
        .iter()
        .rposition(|&b| b != 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    Ok(String::from_utf8_lossy(&raw[..end_trim]).into_owned())
}

pub fn payload<'a>(data: &'a [u8], chunk: &Chunk) -> Result<&'a [u8], Error> {
    let start = chunk.data_offset as usize;
    let end = start
        .checked_add(chunk.size as usize)
        .ok_or_else(|| Error::Format("payload overflow".into()))?;
    if end > data.len() {
        return Err(Error::Format("payload out of bounds".into()));
    }
    Ok(&data[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_xpfh_etit_blob_header() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"XPFH");
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"ETIT");
        buf.extend_from_slice(&9u32.to_be_bytes());
        buf.extend_from_slice(b"univ-test");
        buf.extend_from_slice(&[0, 0, 0]);
        buf.extend_from_slice(b"BLOB");
        buf.extend_from_slice(&4u32.to_be_bytes());
        buf.extend_from_slice(b"ABCD");
        let tree = parse_file(&buf).unwrap();
        assert_eq!(tree.chunks.len(), 3);
        assert_eq!(tree.chunks[0].id, "XPFH");
        assert_eq!(tree.chunks[1].id, "ETIT");
        assert_eq!(text_payload(&buf, &tree.chunks[1]).unwrap(), "univ-test");
        assert_eq!(tree.chunks[2].id, "BLOB");
        assert_eq!(payload(&buf, &tree.chunks[2]).unwrap(), b"ABCD");
    }

    #[test]
    fn parses_nested_xpih_header() {
        // XPIH containing XMDL "Genos"
        let mut inner = Vec::new();
        inner.extend_from_slice(b"XMDL");
        inner.extend_from_slice(&5u32.to_be_bytes());
        inner.extend_from_slice(b"Genos");
        inner.extend_from_slice(&[0, 0, 0]);
        let mut buf = Vec::new();
        buf.extend_from_slice(b"XPIH");
        buf.extend_from_slice(&(inner.len() as u32).to_be_bytes());
        buf.extend_from_slice(&inner);
        let tree = parse_file(&buf).unwrap();
        assert_eq!(tree.chunks[0].id, "XPIH");
        assert_eq!(tree.chunks[0].children[0].id, "XMDL");
        assert_eq!(
            text_payload(&buf, &tree.chunks[0].children[0]).unwrap(),
            "Genos"
        );
    }
}
