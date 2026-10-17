//! Preserve-first Genos Effect1 header metadata and typed overrides.
//! EV-0108 style_header_effects read_style_header_effects/authored_effect_packets;
//! official Genos v2.00 Data List Effect1 table and parameter-change framing.
//! Original SysEx (including bulk counts/checksums) is never rewritten or replayed.
use crate::{
    Error, Result,
    document::Document,
    genos_system_effect,
    smf::{self, Event, Style},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

fn bad(message: &str) -> Error {
    Error::Unsupported(message.into())
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Block {
    Reverb,
    Chorus,
}
impl Block {
    pub fn name(self) -> &'static str {
        match self {
            Self::Reverb => "reverb",
            Self::Chorus => "chorus",
        }
    }
    fn layout(self) -> (u8, u8, usize) {
        match self {
            Self::Reverb => (0, 12, 14),
            Self::Chorus => (32, 44, 15),
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Address {
    pub msb: u8,
    pub lsb: u8,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub block: Block,
    pub effect: Option<Address>,
    pub return_level: Option<u8>,
}
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Values {
    pub msb: Option<u8>,
    pub lsb: Option<u8>,
    pub return_level: Option<u8>,
    pub late_changes: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub boundary: u64,
    pub reverb: Values,
    pub chorus: Values,
    pub edit_problem: Option<String>,
}
impl Header {
    pub fn values(&self, block: Block) -> &Values {
        match block {
            Block::Reverb => &self.reverb,
            Block::Chorus => &self.chorus,
        }
    }
    fn values_mut(&mut self, block: Block) -> &mut Values {
        match block {
            Block::Reverb => &mut self.reverb,
            Block::Chorus => &mut self.chorus,
        }
    }
    fn clear(&mut self) {
        self.reverb = Values::default();
        self.chorus = Values::default();
    }
    fn problem(&mut self, text: &str) {
        self.edit_problem.get_or_insert_with(|| text.into());
    }
}
fn overlaps(low: u8, count: usize, start: u8, size: usize) -> bool {
    usize::from(low) < usize::from(start) + size
        && usize::from(start) < usize::from(low) + count.max(1)
}
fn universal_reset(p: &[u8]) -> bool {
    p.len() == 5 && p[0] == 0x7e && p[2] == 9 && matches!(p[3], 1 | 3)
        || p.len() >= 9 && p[0] == 0x41 && p[2..7] == [0x42, 0x12, 0x40, 0, 0x7f]
}
/// Read only explicit, trusted values; unknown/reset/partial values stay unknown.
pub fn inspect(doc: &Document) -> Result<Header> {
    let boundary = doc
        .sections
        .first()
        .ok_or_else(|| bad("No Style sections"))?
        .start;
    if doc.style.format != 0
        || doc.style.tracks.len() != 1
        || boundary == 0
        || !doc
            .style
            .tracks
            .iter()
            .flatten()
            .any(|e| e.meta(6) == Some(b"SFF2"))
        || doc
            .style
            .tracks
            .iter()
            .flatten()
            .any(|e| e.meta(6) == Some(b"SFF1"))
    {
        return Err(bad(
            "Style effect authoring requires a Type-0 SFF2 initial header",
        ));
    }
    let mut header = Header {
        boundary,
        ..Header::default()
    };
    let mut events: Vec<_> = doc.style.tracks.iter().flatten().collect();
    events.sort_by_key(|e| (e.tick, e.order, e.id));
    for event in events {
        if event.tick < boundary && event.is_on() {
            header.problem(
                "Notes precede the first Style section; initial effect scope is ambiguous",
            );
        }
        if !matches!(event.data.first(), Some(0xf0 | 0xf7)) {
            continue;
        }
        let Some(p) = event.sysex_payload() else {
            header.problem("Malformed SMF SysEx prevents header authoring");
            continue;
        };
        let in_header = event.tick < boundary;
        if event.data[0] != 0xf0 || p.last() != Some(&0xf7) {
            if in_header {
                header.clear();
                header.problem("Split or incomplete initial SysEx prevents effect authoring");
            } else {
                header.reverb.late_changes = true;
                header.chorus.late_changes = true;
            }
            continue;
        }
        if universal_reset(p) {
            if in_header {
                header.clear();
            } else {
                header.reverb.late_changes = true;
                header.chorus.late_changes = true;
            }
            continue;
        }
        if p.len() < 7 || p[0] != 0x43 || p[2] != 0x4c {
            continue;
        }
        let mode = p[1] >> 4;
        let device = p[1] & 15;
        let (address, data, valid) = match mode {
            1 => (
                &p[3..6],
                &p[6..p.len() - 1],
                p.len() > 7 && p[..p.len() - 1].iter().all(|v| *v < 128),
            ),
            0 if p.len() >= 10 => {
                let data = &p[8..p.len() - 2];
                let count = (usize::from(p[3]) << 7) | usize::from(p[4]);
                (
                    &p[5..8],
                    data,
                    p[..p.len() - 1].iter().all(|v| *v < 128)
                        && count == data.len()
                        && (1..=1024).contains(&count)
                        && p[3..p.len() - 1]
                            .iter()
                            .map(|v| usize::from(*v))
                            .sum::<usize>()
                            % 128
                            == 0,
                )
            }
            _ => {
                if in_header && mode == 0 {
                    header.problem("Malformed XG bulk header packet");
                }
                continue;
            }
        };
        if address == [0, 0, 0x7e] || address == [0, 0, 0x7f] {
            if in_header {
                header.clear();
                if !valid || device != 0 || mode != 1 || data != [0] {
                    header.problem("Unverified XG system reset prevents effect authoring");
                }
            } else {
                header.reverb.late_changes = true;
                header.chorus.late_changes = true;
            }
            continue;
        }
        if address[..2] != [2, 1] {
            continue;
        }
        let low = address[2];
        for block in [Block::Reverb, Block::Chorus] {
            let (start, ret, size) = block.layout();
            let touched = overlaps(low, data.len(), start, size);
            if !in_header {
                if touched {
                    header.values_mut(block).late_changes = true;
                }
                continue;
            }
            let known = mode == 1
                && ((low == start && matches!(data.len(), 2)
                    || low == start && data.len() == size)
                    || low == ret && data.len() == 1)
                || mode == 0 && low == start && data.len() == size;
            let trusted = valid && device == 0 && known;
            // Single-register source writes may leave metadata partial. A new
            // complete typed pair can override them without inventing defaults.
            let writable_layout = mode == 1
                && (data.len() == 1 || low == start && (data.len() == 2 || data.len() == size))
                || mode == 0 && known;
            if touched && (!valid || device != 0 || !writable_layout) {
                header.problem("Invalid, foreign-device or unsupported Effect1 header layout; original data preserved");
            }
            let values = header.values_mut(block);
            if overlaps(low, data.len(), start, 2) {
                values.msb = None;
                values.lsb = None;
                if trusted
                    && low <= start
                    && usize::from(low) + data.len() >= usize::from(start) + 2
                {
                    let offset = usize::from(start - low);
                    values.msb = Some(data[offset]);
                    values.lsb = Some(data[offset + 1]);
                }
            }
            if overlaps(low, data.len(), ret, 1) {
                values.return_level = None;
                if trusted && low <= ret && usize::from(ret - low) < data.len() {
                    values.return_level = Some(data[usize::from(ret - low)]);
                }
            }
        }
    }
    Ok(header)
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSetup {
    pub header: Header,
    pub packets: Vec<Vec<u8>>,
}
/// Rebuild only explicit typed fields. Never transmit original source SysEx.
pub fn preview_setup(doc: &Document) -> Result<PreviewSetup> {
    let header = inspect(doc)?;
    if let Some(problem) = &header.edit_problem {
        return Err(bad(problem));
    }
    let mut packets = Vec::new();
    for block in [Block::Reverb, Block::Chorus] {
        let value = header.values(block);
        if value.msb.is_some() && value.lsb.is_some() || value.return_level.is_some() {
            packets.extend(genos_system_effect::system_effect_packets(
                block.name(),
                value.msb,
                value.lsb,
                value.return_level,
            )?);
        }
    }
    if packets.is_empty() {
        return Err(bad(
            "No explicit initial reverb/chorus type or Return values to preview",
        ));
    }
    Ok(PreviewSetup { header, packets })
}
pub(crate) fn apply(style: &mut Style, changes: Vec<Change>) -> Result<()> {
    let doc = Document::from_style(style.clone())?;
    let header = inspect(&doc)?;
    if let Some(problem) = header.edit_problem {
        return Err(bad(&problem));
    }
    if changes.is_empty() || changes.len() > 2 {
        return Err(bad("Choose one or two system-effect blocks"));
    }
    let mut seen = HashSet::new();
    let mut packets = Vec::new();
    for change in changes {
        if !seen.insert(change.block)
            || change.effect.is_none() && change.return_level.is_none()
            || change.effect.is_some_and(|a| a.msb > 127 || a.lsb > 127)
            || change.return_level.is_some_and(|v| v > 127)
        {
            return Err(bad("Invalid, empty or duplicate Style effect change"));
        }
        let current = header.values(change.block);
        let effect = change
            .effect
            .filter(|a| current.msb != Some(a.msb) || current.lsb != Some(a.lsb));
        let level = change
            .return_level
            .filter(|v| current.return_level != Some(*v));
        if effect.is_none() && level.is_none() {
            continue;
        }
        packets.extend(genos_system_effect::system_effect_packets(
            change.block.name(),
            effect.map(|a| a.msb),
            effect.map(|a| a.lsb),
            level,
        )?);
    }
    let next = style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap_or(0)
        + 1;
    for (next, wire) in (next..).zip(packets) {
        let mut data = vec![0xf0];
        smf::write_vlq((wire.len() - 1) as u64, &mut data)?;
        data.extend_from_slice(&wire[1..]);
        style.tracks[0].push(Event {
            id: next,
            tick: header.boundary - 1,
            order: next,
            data,
        });
    }
    Ok(())
}
