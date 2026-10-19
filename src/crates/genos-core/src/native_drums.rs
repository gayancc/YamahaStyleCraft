//! Genos initial-header scalar edits. Contract: retained EV-0108
//! style_drum_setup.read_native_drum_header/_packets; 84 original Style headers.
//! This is neither a generic XG/Genos2 writer nor a live MIDI audition path.
use crate::{
    Error, Result,
    document::Document,
    smf::{self, Event, Style},
    voice_catalog::{self, Address},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

fn bad(message: &str) -> Error {
    Error::Unsupported(message.into())
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Field {
    Volume,
    Pan,
    Pitch,
}
impl Field {
    fn parameter(self) -> u8 {
        match self {
            Self::Volume => 2,
            Self::Pan => 4,
            Self::Pitch => 0,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub note: u8,
    pub field: Field,
    pub value: i16,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub note: u8,
    pub used: bool,
    pub volume: Option<u8>,
    pub pan: Option<u8>,
    pub pitch: Option<i16>,
    pub fine: Option<i16>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub channel: u8,
    pub setup: u8,
    pub address: Address,
    pub keys: Vec<Key>,
    pub parameter_count: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub event: u64,
    pub setup: u8,
    pub note: u8,
    pub parameter: u8,
    pub data: Vec<u8>,
}
pub fn payload(event: &Event) -> Option<&[u8]> {
    event.sysex_payload()
}
fn parameter(p: &[u8], event: u64) -> Result<Parameter> {
    if p.len() < 8
        || p[0..3] != [0x43, 0x10, 0x4c]
        || !matches!(p[3], 0x30 | 0x31)
        || p.last() != Some(&0xf7)
        || p[..p.len() - 1].iter().any(|b| *b > 127)
        || !(13..=91).contains(&p[4])
    {
        return Err(bad("Unsupported initial drum parameter packet"));
    }
    let data = &p[6..p.len() - 1];
    let valid = match p[5] {
        0x70 => data.len() == 4 && matches!(data[0], 126 | 127) && (13..=91).contains(&data[3]),
        8..=10 => data.len() == 1 && data[0] <= 1,
        0..=15 => data.len() == 1,
        _ => false,
    };
    if !valid {
        return Err(bad("Unknown or malformed initial drum parameter"));
    }
    Ok(Parameter {
        event,
        setup: p[3] - 0x30,
        note: p[4],
        parameter: p[5],
        data: data.into(),
    })
}
/// Raw typed parameters for independent corpus comparisons, without claiming edit eligibility.
pub fn parameters(doc: &Document) -> Result<Vec<Parameter>> {
    let mut events: Vec<_> = doc.style.tracks.iter().flatten().collect();
    events.sort_by_key(|e| (e.tick, e.order, e.id));
    events
        .into_iter()
        .filter_map(|e| {
            payload(e)
                .filter(|p| {
                    p.len() > 3 && p[..3] == [0x43, 0x10, 0x4c] && matches!(p[3], 0x30 | 0x31)
                })
                .map(|p| parameter(p, e.id))
        })
        .collect()
}
struct Binding {
    boundary: u64,
    setup: u8,
    address: Address,
    records: Vec<Parameter>,
    state_tick: u64,
}
fn binding(doc: &Document, channel: u8) -> Result<Binding> {
    if doc.style.format != 0 || doc.style.tracks.len() != 1 {
        return Err(bad(
            "Genos per-key editing currently requires a Type-0 SFF2 Style",
        ));
    }
    if doc
        .warnings
        .iter()
        .any(|w| w.contains("Ctb2") || w.contains("Ctab"))
    {
        return Err(bad(
            "Unsupported CASM records prevent verified drum routing",
        ));
    }
    if ![9, 10].contains(&channel) {
        return Err(bad("Select rhythm source channel 9 or 10"));
    }
    let boundary = doc
        .sections
        .first()
        .ok_or_else(|| bad("No Style sections"))?
        .start;
    if boundary == 0
        || doc
            .style
            .tracks
            .iter()
            .flatten()
            .any(|e| e.meta(6) == Some(b"SFF1"))
        || !doc
            .style
            .tracks
            .iter()
            .flatten()
            .any(|e| e.meta(6) == Some(b"SFF2"))
    {
        return Err(bad(
            "Genos per-key editing requires an SFF2 initial header before the first section",
        ));
    }
    // A source must remain one unchanged rhythm destination in every section.
    for section in &doc.sections {
        let routes: Vec<_> = doc
            .routes
            .iter()
            .filter(|r| {
                r.sections.contains(&section.label)
                    && (r.source == channel || r.destination == channel)
            })
            .collect();
        if routes.len() != 1 || routes[0].source != channel || routes[0].destination != channel {
            return Err(bad(
                "Rhythm routing must remain one unchanged source and destination in every section",
            ));
        }
    }
    let mut es: Vec<_> = doc.style.tracks.iter().flatten().collect();
    es.sort_by_key(|e| (e.tick, e.order, e.id));
    let mut modes = HashMap::new();
    let mut records = Vec::new();
    let mut state_tick = 0;
    let mut voices = HashMap::new();
    let mut banks: HashMap<u8, [Option<u8>; 2]> = HashMap::new();
    for e in es {
        if e.tick < boundary && e.is_on() {
            return Err(bad("Notes in the initial header are unsupported"));
        }
        if let Some(ch) = e.channel() {
            let bank = e.data[0] >> 4 == 0xb && matches!(e.data[1], 0 | 32);
            let pc = e.data[0] >> 4 == 0xc;
            if [9, 10].contains(&ch) && (bank || pc) {
                if e.tick >= boundary {
                    if ch == channel {
                        return Err(bad(
                            "This rhythm channel changes kit or bank inside the Style",
                        ));
                    }
                    continue;
                }
                state_tick = state_tick.max(e.tick);
                if bank {
                    banks.entry(ch).or_insert([None, None])[usize::from(e.data[1] == 32)] =
                        Some(e.data[2]);
                }
                if pc {
                    if modes.get(&ch).is_some_and(|v| matches!(v, 2 | 3)) {
                        return Err(bad("Kit Program Change follows drum setup assignment"));
                    }
                    let b = banks.get(&ch).copied().unwrap_or([None, None]);
                    if let (Some(msb), Some(lsb)) = (b[0], b[1]) {
                        voices.insert(
                            ch,
                            Address {
                                msb,
                                lsb,
                                program: e.data[1],
                            },
                        );
                    } else {
                        voices.remove(&ch);
                    }
                }
            }
        }
        let Some(p) = payload(e) else {
            continue;
        };
        if e.data[0] != 0xf0 || p.last() != Some(&0xf7) {
            return Err(bad(
                "Split or incomplete SysEx prevents native drum editing",
            ));
        }
        let reset = matches!(
            p,
            [0x7e, 0x7f, 9, 1, 0xf7]
                | [0x7e, 0x7f, 9, 3, 0xf7]
                | [0x43, 0x10, 0x4c, 0, 0, 0x7e, 0, 0xf7]
                | [0x43, 0x10, 0x4c, 0, 0, 0x7f, 0, 0xf7]
        );
        if reset {
            if e.tick >= boundary {
                return Err(bad(
                    "System reset inside a section prevents native drum editing",
                ));
            }
            modes.clear();
            voices.clear();
            banks.clear();
            records.clear();
            state_tick = e.tick;
            continue;
        }
        if p.len() < 6 || p[0] != 0x43 || p[2] != 0x4c {
            if (p.first() == Some(&0x7e) && p.get(2) == Some(&9)) || p.first() == Some(&0x41) {
                return Err(bad(
                    "Unverified system-mode or foreign reset messages prevent drum editing",
                ));
            }
            continue;
        }
        let address = if p[1] >> 4 == 1 {
            p.get(3..6)
        } else {
            p.get(5..8)
        };
        let targeted =
            address.is_some_and(|a| matches!(a[0], 0x30 | 0x31) || a[0] == 8 && a[2] == 7);
        if address.is_some_and(|a| a[..2] == [0, 0] && (0x7d..=0x7f).contains(&a[2])) {
            return Err(bad("Unverified drum/system reset prevents native editing"));
        }
        if !targeted {
            continue;
        }
        if e.tick >= boundary || p[1] != 0x10 || p[..p.len() - 1].iter().any(|b| *b > 127) {
            return Err(bad(
                "Late, bulk, or nonstandard drum setup data is unsupported",
            ));
        }
        if p[3] == 8 {
            if p.len() != 8 || p[4] > 15 || p[6] > 3 {
                return Err(bad("Malformed drum setup assignment"));
            }
            if modes.contains_key(&(p[4] + 1)) {
                return Err(bad("Repeated drum setup assignment is unsupported"));
            }
            modes.insert(p[4] + 1, p[6]);
            state_tick = state_tick.max(e.tick);
        } else {
            let record = parameter(p, e.id)?;
            if modes.values().filter(|v| **v == record.setup + 2).count() != 1 {
                return Err(bad("Drum parameters precede a unique setup assignment"));
            }
            records.push(record);
        }
    }
    let setup = modes
        .get(&channel)
        .copied()
        .filter(|v| matches!(v, 2 | 3))
        .ok_or_else(|| bad("No explicit initial drum setup owner on this channel"))?
        - 2;
    if modes.values().filter(|v| **v == setup + 2).count() != 1 {
        return Err(bad("Several channels share this drum setup"));
    }
    let address = *voices
        .get(&channel)
        .filter(|a| matches!(a.msb, 126 | 127))
        .ok_or_else(|| bad("Initial factory drum kit address is incomplete or unsupported"))?;
    if voice_catalog::address_at(doc, boundary, channel) != Some(address) {
        return Err(bad(
            "The section-start kit differs from the initial drum kit",
        ));
    }
    if records.iter().filter(|r| r.setup == setup).count() > 512 {
        return Err(bad("Initial drum setup exceeds 512 parameters"));
    }
    Ok(Binding {
        boundary,
        setup,
        address,
        records,
        state_tick,
    })
}
pub fn inspect(doc: &Document, channel: u8) -> Result<Inspection> {
    let b = binding(doc, channel)?;
    let mut keys: BTreeMap<u8, Key> = (13..=91)
        .map(|note| {
            (
                note,
                Key {
                    note,
                    used: doc
                        .notes
                        .iter()
                        .any(|n| n.channel == channel && n.pitch == note),
                    volume: None,
                    pan: None,
                    pitch: None,
                    fine: None,
                },
            )
        })
        .collect();
    for r in b.records.iter().filter(|r| r.setup == b.setup) {
        let key = keys.get_mut(&r.note).unwrap();
        match r.parameter {
            0 => key.pitch = Some(i16::from(r.data[0]) - 64),
            1 => key.fine = Some(i16::from(r.data[0]) - 64),
            2 => key.volume = Some(r.data[0]),
            4 => key.pan = Some(r.data[0]),
            _ => {}
        }
    }
    Ok(Inspection {
        channel,
        setup: b.setup,
        address: b.address,
        keys: keys.into_values().collect(),
        parameter_count: b.records.iter().filter(|r| r.setup == b.setup).count(),
    })
}
pub(crate) fn apply(
    style: &mut Style,
    channel: u8,
    address: Address,
    changes: Vec<Change>,
) -> Result<()> {
    let doc = Document::from_style(style.clone())?;
    let b = binding(&doc, channel)?;
    if address != b.address {
        return Err(bad("Initial drum kit changed; reopen Drum Setup"));
    }
    if changes.is_empty() || changes.len() > 237 {
        return Err(bad("Choose 1..237 scalar drum changes"));
    }
    let mut seen = HashSet::new();
    let mut additions = Vec::new();
    let mut next = style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap_or(0)
        + 1;
    for change in changes {
        let range = match change.field {
            Field::Volume => 0..=127,
            Field::Pan => 1..=127,
            Field::Pitch => -64..=63,
        };
        if !(13..=91).contains(&change.note)
            || !range.contains(&change.value)
            || !seen.insert((change.note, change.field))
        {
            return Err(bad("Invalid or duplicate per-key scalar edit"));
        }
        if change.field == Field::Pitch
            && b.records
                .iter()
                .rev()
                .find(|r| r.setup == b.setup && r.note == change.note && r.parameter == 1)
                .is_some_and(|r| r.data[0] != 64)
        {
            return Err(bad(
                "Whole-semitone pitch editing requires neutral original fine tuning",
            ));
        }
        let raw = if change.field == Field::Pitch {
            (change.value + 64) as u8
        } else {
            change.value as u8
        };
        let previous = b.records.iter().rev().find(|r| {
            r.setup == b.setup && r.note == change.note && r.parameter == change.field.parameter()
        });
        if let Some(previous) = previous {
            if previous.data[0] == raw {
                continue;
            }
            let event = style
                .tracks
                .iter_mut()
                .flatten()
                .find(|e| e.id == previous.event)
                .unwrap();
            let mut p = 1;
            smf::vlq(&event.data, &mut p)?;
            event.data[p + 6] = raw;
        } else {
            if b.state_tick >= b.boundary - 1 {
                return Err(bad("No initial-header space after kit/setup assignment"));
            }
            additions.push(Event {
                id: next,
                tick: b.boundary - 1,
                order: next,
                data: vec![
                    0xf0,
                    8,
                    0x43,
                    0x10,
                    0x4c,
                    0x30 + b.setup,
                    change.note,
                    change.field.parameter(),
                    raw,
                    0xf7,
                ],
            });
            next += 1;
        }
    }
    if b.records.iter().filter(|r| r.setup == b.setup).count() + additions.len() > 512 {
        return Err(bad("Drum changes exceed the 512-parameter header limit"));
    }
    style.tracks[0].extend(additions);
    Ok(())
}

/// Remove per-key volume/pan/pitch overrides so kit defaults apply again (SFM Reset drum / Reset all).
/// `notes` empty = every key that currently has a scalar override on this setup.
pub fn reset_keys(
    style: &mut Style,
    channel: u8,
    address: Address,
    notes: Vec<u8>,
) -> Result<usize> {
    let doc = Document::from_style(style.clone())?;
    let b = binding(&doc, channel)?;
    if address != b.address {
        return Err(bad("Initial drum kit changed; reopen Drum Setup"));
    }
    let scalar_params = [
        Field::Pitch.parameter(),
        Field::Volume.parameter(),
        Field::Pan.parameter(),
    ];
    let targets: HashSet<u8> = if notes.is_empty() {
        b.records
            .iter()
            .filter(|r| r.setup == b.setup && scalar_params.contains(&r.parameter))
            .map(|r| r.note)
            .collect()
    } else {
        notes
            .into_iter()
            .filter(|n| (13..=91).contains(n))
            .collect()
    };
    if targets.is_empty() {
        return Err(bad("No drum scalar overrides to reset"));
    }
    let remove: HashSet<u64> = b
        .records
        .iter()
        .filter(|r| {
            r.setup == b.setup && targets.contains(&r.note) && scalar_params.contains(&r.parameter)
        })
        .map(|r| r.event)
        .collect();
    if remove.is_empty() {
        return Err(bad("No drum scalar overrides to reset"));
    }
    let count = remove.len();
    for track in &mut style.tracks {
        track.retain(|e| !remove.contains(&e.id));
    }
    Ok(count)
}
