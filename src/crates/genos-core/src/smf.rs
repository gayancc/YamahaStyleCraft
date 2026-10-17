//! Independently authored preservation layer. Unknown chunks and event payloads
//! remain opaque. midly is a second SMF structural validator, not a Style parser.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TICK: u64 = 1_000_000_000;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Chunk {
    pub tag: [u8; 4],
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: u64,
    pub tick: u64,
    pub order: u64,
    pub data: Vec<u8>,
}
impl Event {
    /// Payload of one complete SMF SysEx event, excluding status and SMF length.
    /// This does not validate or interpret the manufacturer's packet format.
    pub fn sysex_payload(&self) -> Option<&[u8]> {
        if !matches!(self.data.first(), Some(0xf0 | 0xf7)) {
            return None;
        }
        let mut p = 1;
        let n = vlq(&self.data, &mut p).ok()? as usize;
        self.data.get(p..p + n).filter(|_| p + n == self.data.len())
    }
    pub fn channel(&self) -> Option<u8> {
        (self.data[0] < 0xf0).then_some((self.data[0] & 15) + 1)
    }
    pub fn is_on(&self) -> bool {
        self.data[0] & 0xf0 == 0x90 && self.data[2] > 0
    }
    pub fn is_off(&self) -> bool {
        self.data[0] & 0xf0 == 0x80 || (self.data[0] & 0xf0 == 0x90 && self.data[2] == 0)
    }
    pub fn meta(&self, kind: u8) -> Option<&[u8]> {
        if self.data.get(..2) != Some(&[0xff, kind]) {
            return None;
        }
        let mut p = 2;
        let n = vlq(&self.data, &mut p).ok()? as usize;
        self.data.get(p..p + n)
    }
}
#[derive(Clone, Debug)]
pub struct Style {
    pub chunks: Vec<Chunk>,
    pub tracks: Vec<Vec<Event>>,
    pub track_chunks: Vec<usize>,
    pub ppq: u16,
    pub format: u16,
}
fn bad(s: &str) -> Error {
    Error::Format(s.into())
}
pub fn chunks(bytes: &[u8]) -> Result<Vec<Chunk>> {
    let mut p = 0;
    let mut out = Vec::new();
    while p < bytes.len() {
        if bytes.len() - p < 8 {
            return Err(bad("truncated chunk header"));
        }
        let tag = bytes[p..p + 4].try_into().unwrap();
        let len = u32::from_be_bytes(bytes[p + 4..p + 8].try_into().unwrap()) as usize;
        p += 8;
        let end = p
            .checked_add(len)
            .filter(|e| *e <= bytes.len())
            .ok_or_else(|| bad("truncated chunk payload"))?;
        out.push(Chunk {
            tag,
            data: bytes[p..end].to_vec(),
        });
        p = end;
    }
    Ok(out)
}
pub fn pack(chunks: &[Chunk]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for c in chunks {
        let n = u32::try_from(c.data.len()).map_err(|_| bad("chunk too large"))?;
        out.extend(c.tag);
        out.extend(n.to_be_bytes());
        out.extend(&c.data);
        if out.len() > MAX_BYTES {
            return Err(bad("output exceeds 64 MiB"));
        }
    }
    Ok(out)
}
pub fn vlq(b: &[u8], p: &mut usize) -> Result<u32> {
    let mut n = 0;
    for _ in 0..4 {
        let x = *b.get(*p).ok_or_else(|| bad("truncated VLQ"))?;
        *p += 1;
        n = (n << 7) | u32::from(x & 127);
        if x & 128 == 0 {
            return Ok(n);
        }
    }
    Err(bad("VLQ longer than four bytes"))
}
pub fn write_vlq(n: u64, out: &mut Vec<u8>) -> Result<()> {
    if n > 0x0fff_ffff {
        return Err(bad("delta exceeds MIDI VLQ limit"));
    }
    let mut buf = [0u8; 4];
    let mut p = 3;
    let mut x = n;
    buf[p] = (x & 127) as u8;
    while {
        x >>= 7;
        x > 0
    } {
        p -= 1;
        buf[p] = ((x & 127) as u8) | 128;
    }
    out.extend_from_slice(&buf[p..]);
    Ok(())
}
pub fn parse_track(b: &[u8], next: &mut u64) -> Result<Vec<Event>> {
    let mut p = 0;
    let mut tick = 0u64;
    let mut running = None;
    let mut out = Vec::new();
    let mut eot = false;
    while p < b.len() {
        if eot {
            return Err(bad("event bytes after End Of Track"));
        }
        tick = tick
            .checked_add(vlq(b, &mut p)? as u64)
            .filter(|t| *t <= MAX_TICK)
            .ok_or_else(|| bad("tick limit exceeded"))?;
        let first = *b.get(p).ok_or_else(|| bad("missing status"))?;
        let status = if first >= 128 {
            p += 1;
            first
        } else {
            running.ok_or_else(|| bad("missing running status"))?
        };
        let mut data = vec![status];
        match status {
            0x80..=0xef => {
                running = Some(status);
                let len = if matches!(status >> 4, 0xc | 0xd) {
                    1
                } else {
                    2
                };
                let d = b
                    .get(p..p + len)
                    .ok_or_else(|| bad("truncated channel event"))?;
                if d.iter().any(|x| *x >= 128) {
                    return Err(bad("invalid MIDI data byte"));
                }
                data.extend(d);
                p += len;
            }
            0xff | 0xf0 | 0xf7 => {
                running = None;
                if status == 0xff {
                    data.push(*b.get(p).ok_or_else(|| bad("missing meta type"))?);
                    p += 1;
                }
                let start = p;
                let n = vlq(b, &mut p)? as usize;
                let end = p
                    .checked_add(n)
                    .filter(|e| *e <= b.len())
                    .ok_or_else(|| bad("truncated meta/SysEx"))?;
                data.extend_from_slice(&b[start..end]);
                p = end;
                if status == 0xff && data[1] == 0x2f {
                    if n != 0 {
                        return Err(bad("invalid End Of Track"));
                    }
                    eot = true;
                }
            }
            _ => return Err(bad("unsupported SMF system status")),
        }
        *next += 1;
        out.push(Event {
            id: *next,
            tick,
            order: *next,
            data,
        });
        if out.len() > 2_000_000 {
            return Err(bad("event limit exceeded"));
        }
    }
    if !eot {
        return Err(bad("missing End Of Track"));
    }
    Ok(out)
}
impl Style {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_BYTES || bytes.len() < 14 {
            return Err(bad("file size outside supported range"));
        }
        let cs = chunks(bytes)?;
        let h = cs
            .first()
            .filter(|c| c.tag == *b"MThd" && c.data.len() >= 6)
            .ok_or_else(|| bad("missing MThd"))?;
        let format = u16::from_be_bytes([h.data[0], h.data[1]]);
        let count = u16::from_be_bytes([h.data[2], h.data[3]]);
        let ppq = u16::from_be_bytes([h.data[4], h.data[5]]);
        if ppq == 0 || ppq & 0x8000 != 0 {
            return Err(Error::Unsupported("SMPTE/zero division".into()));
        }
        if format > 1 {
            return Err(Error::Unsupported("SMF format 2".into()));
        }
        let track_chunks: Vec<_> = cs
            .iter()
            .enumerate()
            .filter(|(_, c)| c.tag == *b"MTrk")
            .map(|(i, _)| i)
            .collect();
        if count as usize != track_chunks.len() || count == 0 || (format == 0 && count != 1) {
            return Err(bad("track count/header mismatch"));
        }
        let mut next = 0;
        let mut tracks = Vec::new();
        for i in &track_chunks {
            tracks.push(parse_track(&cs[*i].data, &mut next)?);
        }
        // Feed only the MIDI prefix to midly; Yamaha suffix chunks are our responsibility.
        let midi = pack(&cs[..=*track_chunks.last().unwrap()])?;
        midly::Smf::parse(&midi).map_err(|e| bad(&format!("SMF validation: {e}")))?;
        Ok(Self {
            chunks: cs,
            tracks,
            track_chunks,
            ppq,
            format,
        })
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        pack(&self.chunks)
    }
}
pub fn encode_track(events: &[Event]) -> Result<Vec<u8>> {
    let mut es = events.to_vec();
    es.sort_by_key(|e| (e.tick, e.order, e.id));
    let mut out = Vec::new();
    let mut previous = 0;
    let end = es.iter().map(|e| e.tick).max().unwrap_or(0);
    for e in es.iter().filter(|e| e.meta(0x2f).is_none()) {
        write_vlq(e.tick - previous, &mut out)?;
        out.extend(&e.data);
        previous = e.tick;
    }
    write_vlq(end - previous, &mut out)?;
    out.extend([0xff, 0x2f, 0]);
    Ok(out)
}
