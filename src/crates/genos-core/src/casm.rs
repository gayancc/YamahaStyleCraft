//! Section-local Ctb2 authoring, based on findings/10 and EV-0160.
//! Opaque bytes are retained; this module does not emulate keyboard playback.
use crate::{
    Error, Result,
    document::Document,
    smf::{self, Chunk, Style},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Region {
    Low,
    Middle,
    High,
}
impl Region {
    fn offset(&self) -> usize {
        match self {
            Self::Low => 22,
            Self::Middle => 28,
            Self::High => 34,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleField {
    Ntr,
    Ntt,
    Bass,
    HighKey,
    NoteLow,
    NoteHigh,
    Retrigger,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RouteChange {
    PartDefaults,
    RootEnabled {
        root: u8,
        enabled: bool,
    },
    AutoStart {
        enabled: bool,
    },
    Limits {
        region: Region,
        low: u8,
        high: u8,
    },
    Root {
        value: u8,
    },
    Chord {
        value: u8,
    },
    Zone {
        low: u8,
        high: u8,
    },
    Rule {
        region: Region,
        field: RuleField,
        value: u8,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SourceAction {
    Edit {
        source: u8,
        destination: u8,
        changes: Vec<RouteChange>,
        chords: Option<Vec<u8>>,
        #[serde(default)]
        exclusive: bool,
    },
    Clone {
        source: u8,
        destination: u8,
        target: u8,
    },
}
fn reject(message: &str) -> Error {
    Error::Edit(message.into())
}
fn labels(records: &[Chunk]) -> Result<Vec<String>> {
    let descriptions: Vec<_> = records.iter().filter(|r| r.tag == *b"Sdec").collect();
    if descriptions.len() != 1 {
        return Err(reject("CASM group must have one section description"));
    }
    let text = std::str::from_utf8(&descriptions[0].data)
        .map_err(|_| reject("CASM section labels are not UTF-8"))?;
    let names: Vec<_> = text
        .trim_end_matches('\0')
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    if names.iter().any(String::is_empty)
        || names
            .iter()
            .enumerate()
            .any(|(i, n)| names[..i].contains(n))
    {
        return Err(reject("ambiguous CASM section description"));
    }
    Ok(names)
}
fn set_labels(records: &mut [Chunk], names: &[String]) {
    records.iter_mut().find(|r| r.tag == *b"Sdec").unwrap().data = names.join(",").into_bytes();
}
fn route_matches(record: &Chunk, source: u8, destination: u8) -> bool {
    record.tag == *b"Ctb2"
        && record.data.len() == 47
        && record.data[0] == source - 1
        && record.data[9] == destination - 1
}
// Find a unique route, and split its CSEG if other sections share it.
type IsolatedRoute = (usize, Vec<Chunk>, usize, Vec<Chunk>, usize);
fn isolate(style: &mut Style, label: &str, source: u8, destination: u8) -> Result<IsolatedRoute> {
    if !(1..=16).contains(&source) || !(1..=16).contains(&destination) {
        return Err(reject("invalid route channels"));
    }
    let mut found = None;
    for (ci, chunk) in style
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| c.tag == *b"CASM")
    {
        let groups = smf::chunks(&chunk.data)?;
        for (gi, group) in groups.iter().enumerate().filter(|(_, g)| g.tag == *b"CSEG") {
            let records = smf::chunks(&group.data)?;
            let names = labels(&records)?;
            if names.iter().any(|n| n == label) {
                for (ri, record) in records.iter().enumerate() {
                    if route_matches(record, source, destination) {
                        if found.is_some() {
                            return Err(reject("ambiguous source route binding"));
                        }
                        found = Some((ci, groups.clone(), gi, records.clone(), ri, names.clone()));
                    }
                }
            }
        }
    }
    let (ci, mut groups, mut gi, mut records, ri, names) =
        found.ok_or_else(|| reject("verified section source route not found"))?;
    if names.len() > 1 {
        let mut siblings = records.clone();
        set_labels(
            &mut siblings,
            &names.into_iter().filter(|n| n != label).collect::<Vec<_>>(),
        );
        groups[gi].data = smf::pack(&siblings)?;
        set_labels(&mut records, &[label.to_string()]);
        groups.insert(
            gi + 1,
            Chunk {
                tag: *b"CSEG",
                data: smf::pack(&records)?,
            },
        );
        gi += 1;
    }
    Ok((ci, groups, gi, records, ri))
}
fn save(
    style: &mut Style,
    ci: usize,
    mut groups: Vec<Chunk>,
    gi: usize,
    records: Vec<Chunk>,
) -> Result<()> {
    groups[gi].data = smf::pack(&records)?;
    style.chunks[ci].data = smf::pack(&groups)?;
    Ok(())
}
fn set_bit(bytes: &mut [u8], chord: u8, enabled: bool) {
    let index = 17 - usize::from(chord / 8);
    let mask = 1 << (chord % 8);
    if enabled {
        bytes[index] |= mask;
    } else {
        bytes[index] &= !mask;
    }
}
fn change(bytes: &mut [u8], changes: &[RouteChange]) -> Result<()> {
    if changes.len() > 128 {
        return Err(reject("too many route changes"));
    }
    for change in changes {
        match *change {
            RouteChange::PartDefaults => {
                // Observed StyleMagic 3.7.1.3 default tables and saved Ctb2
                // records. Rows: Auto Start, NTR, NTT/Bass, high key, limits, RTR.
                let rows: [[u8; 7]; 8] =
                    serde_json::from_str(include_str!("../assets/casm-part-defaults.json"))
                        .map_err(|_| reject("invalid bundled CASM part defaults"))?;
                let row = bytes[9]
                    .checked_sub(8)
                    .and_then(|part| rows.get(usize::from(part)))
                    .ok_or_else(|| reject("CASM part defaults require destination 9–16"))?;
                // Preserve source, name, destination, editable flag, reserved
                // mask bits and opaque trailing fields.
                bytes[11] |= 15;
                bytes[12] = 255;
                bytes[13] = (bytes[13] & !7) | 3 | (row[0] << 2);
                bytes[14..18].fill(255);
                bytes[18..22].copy_from_slice(&[0, 2, 0, 127]);
                for offset in [22, 28, 34] {
                    bytes[offset..offset + 6].copy_from_slice(&row[1..]);
                }
            }
            RouteChange::RootEnabled { root, enabled } if root < 12 => {
                // StyleMagic 3.7.1.3 Apply 004cb2a8, word helper 0049dab8:
                // C..G are bits8..15 of the LE word at11; G#..B bits0..3.
                let index = 12 - usize::from(root / 8);
                let mask = 1 << (root % 8);
                if enabled {
                    bytes[index] |= mask;
                } else {
                    bytes[index] &= !mask;
                }
            }
            RouteChange::AutoStart { enabled } => {
                // Same Apply routine, byte helper 0049da8c; preserve chord bits.
                if enabled {
                    bytes[13] |= 4;
                } else {
                    bytes[13] &= !4;
                }
            }
            RouteChange::Limits {
                ref region,
                low,
                high,
            } if low <= high && high <= 127 => {
                let offset = region.offset();
                bytes[offset + 3] = low;
                bytes[offset + 4] = high;
            }
            RouteChange::Root { value } if value <= 11 => bytes[18] = value,
            RouteChange::Chord { value } if value <= 33 => bytes[19] = value,
            RouteChange::Zone { low, high } if low <= high && high <= 127 => {
                bytes[20] = low;
                bytes[21] = high;
            }
            RouteChange::Rule {
                ref region,
                ref field,
                value,
            } => {
                let o = region.offset();
                match field {
                    RuleField::Ntr if value <= 2 => {
                        if (bytes[o] == 2) != (value == 2) {
                            bytes[o + 1] &= 128;
                        }
                        bytes[o] = value;
                    }
                    RuleField::Ntt
                        if bytes[o] <= 2 && value <= if bytes[o] == 2 { 2 } else { 10 } =>
                    {
                        bytes[o + 1] = (bytes[o + 1] & 128) | value
                    }
                    RuleField::Bass if value <= 1 => {
                        bytes[o + 1] = (bytes[o + 1] & 127) | (value << 7)
                    }
                    RuleField::HighKey if value <= 11 => bytes[o + 2] = value,
                    RuleField::NoteLow if value <= 127 && value <= bytes[o + 4] => {
                        bytes[o + 3] = value
                    }
                    RuleField::NoteHigh if value <= 127 && value >= bytes[o + 3] => {
                        bytes[o + 4] = value
                    }
                    RuleField::Retrigger if value <= 4 => bytes[o + 5] = value,
                    _ => return Err(reject("unsupported or inverted CASM rule value")),
                }
            }
            _ => return Err(reject("unsupported or inverted CASM source value")),
        }
    }
    Ok(())
}
// Enumerate all destination siblings, failing closed on malformed/opaque routes.
fn siblings(style: &Style, label: &str, destination: u8) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    for c in style.chunks.iter().filter(|c| c.tag == *b"CASM") {
        for group in smf::chunks(&c.data)?.iter().filter(|g| g.tag == *b"CSEG") {
            let records = smf::chunks(&group.data)?;
            if !labels(&records)?.iter().any(|n| n == label) {
                continue;
            }
            for record in records {
                if record.tag == *b"Ctab"
                    || (record.tag == *b"Ctb2"
                        && (record.data.len() != 47
                            || record.data[0] >= 16
                            || record.data[9] >= 16))
                {
                    return Err(reject(
                        "all sibling routes must be verified before exclusive assignment or cloning",
                    ));
                }
                if record.tag == *b"Ctb2" && record.data[9] + 1 == destination {
                    let source = record.data[0] + 1;
                    if result.contains(&source) {
                        return Err(reject("duplicate sibling source route"));
                    }
                    result.push(source);
                }
            }
        }
    }
    Ok(result)
}

pub(crate) fn apply(
    style: &mut Style,
    section: u64,
    actions: Vec<SourceAction>,
) -> Result<Vec<(u64, u64)>> {
    if actions.is_empty() || actions.len() > 64 {
        return Err(reject("invalid source-track transaction size"));
    }
    let initial = Document::from_style(style.clone())?;
    let section = initial
        .sections
        .iter()
        .find(|s| s.id == section)
        .ok_or_else(|| reject("section not found"))?
        .clone();
    if initial
        .sections
        .iter()
        .filter(|s| s.label == section.label)
        .count()
        != 1
    {
        return Err(reject(
            "section labels must be unique for source-track authoring",
        ));
    }
    let mut cloned_pairs = Vec::new();
    for action in actions {
        match action {
            SourceAction::Edit {
                source,
                destination,
                changes,
                chords,
                exclusive,
            } => {
                if exclusive && chords.is_none() {
                    return Err(reject("exclusive assignment needs a chord selection"));
                }
                if let Some(ref chords) = chords
                    && (chords.len() > 34
                        || chords.iter().any(|c| *c > 33)
                        || chords
                            .iter()
                            .enumerate()
                            .any(|(i, c)| chords[..i].contains(c)))
                {
                    return Err(reject("invalid chord selection"));
                }
                // Validate every sibling before changing even the target candidate.
                let other_sources = if exclusive {
                    siblings(style, &section.label, destination)?
                } else {
                    Vec::new()
                };
                let (ci, groups, gi, mut records, ri) =
                    isolate(style, &section.label, source, destination)?;
                let before = records[ri].data.clone();
                change(&mut records[ri].data, &changes)?;
                if let Some(ref chords) = chords {
                    for chord in 0..34 {
                        set_bit(&mut records[ri].data, chord, chords.contains(&chord));
                    }
                }
                // Avoid splitting shared groups for a no-op edit.
                if records[ri].data != before {
                    save(style, ci, groups, gi, records)?;
                }
                if exclusive {
                    for sibling in other_sources.into_iter().filter(|s| *s != source) {
                        let (ci, groups, gi, mut records, ri) =
                            isolate(style, &section.label, sibling, destination)?;
                        let before = records[ri].data.clone();
                        for chord in chords.as_ref().unwrap() {
                            set_bit(&mut records[ri].data, *chord, false);
                        }
                        if records[ri].data != before {
                            save(style, ci, groups, gi, records)?;
                        }
                    }
                }
            }
            SourceAction::Clone {
                source,
                destination,
                target,
            } => {
                if !(1..=16).contains(&target) {
                    return Err(reject("invalid clone source channel"));
                }
                // Validate opaque/duplicate siblings before adding a new route.
                siblings(style, &section.label, destination)?;
                let current = Document::from_style(style.clone())?;
                let used_route = current
                    .routes
                    .iter()
                    .any(|r| r.sections.contains(&section.label) && r.source == target);
                let used_event = style.tracks.iter().flatten().any(|e| {
                    e.channel() == Some(target) && e.tick >= section.start && e.tick < section.end
                });
                let used_note = current.notes.iter().any(|n| {
                    n.channel == target
                        && n.start < section.end
                        && n.start + n.duration > section.start
                });
                if used_route || used_event || used_note {
                    return Err(reject("clone channel is used or reserved in this section"));
                }
                if current.notes.iter().any(|n| {
                    n.channel == source
                        && n.start < section.end
                        && n.start + n.duration > section.start
                        && (n.start < section.start || n.start + n.duration > section.end)
                }) {
                    return Err(reject(
                        "source notes cross the section boundary; clone rejected",
                    ));
                }
                let (ci, groups, gi, mut records, ri) =
                    isolate(style, &section.label, source, destination)?;
                let mut record = records[ri].clone();
                record.data[0] = target - 1;
                for chord in 0..34 {
                    set_bit(&mut record.data, chord, false);
                }
                records.push(record);
                save(style, ci, groups, gi, records)?;
                let note_ids: std::collections::HashSet<_> = current
                    .notes
                    .iter()
                    .filter(|n| {
                        n.channel == source && n.start >= section.start && n.start < section.end
                    })
                    .flat_map(|n| [n.id, n.off_id])
                    .collect();
                let mut next = style
                    .tracks
                    .iter()
                    .flatten()
                    .map(|e| e.id.max(e.order))
                    .max()
                    .unwrap_or(0)
                    + 1;
                // Copy bank/program history to establish the same source voice at entry.
                let mut baseline = std::collections::BTreeMap::new();
                for (track, events) in style.tracks.iter().enumerate() {
                    for e in events.iter().filter(|e| {
                        e.channel() == Some(source)
                            && e.tick < section.start
                            && (e.data[0] >> 4 == 0xc
                                || (e.data[0] >> 4 == 0xb && matches!(e.data[1], 0 | 32)))
                    }) {
                        let key = (
                            e.data[0] >> 4,
                            if e.data[0] >> 4 == 0xb { e.data[1] } else { 0 },
                        );
                        let candidate = (e.tick, track, e.order);
                        if baseline
                            .get(&key)
                            .is_none_or(|(prior, _)| candidate > *prior)
                        {
                            baseline.insert(key, (candidate, e.clone()));
                        }
                    }
                }
                let mut cloned_ids = std::collections::HashMap::new();
                for (track, events) in style.tracks.iter_mut().enumerate() {
                    let mut copied: Vec<_> = events
                        .iter()
                        .filter(|e| {
                            note_ids.contains(&e.id)
                                || (e.channel() == Some(source)
                                    && e.tick >= section.start
                                    && e.tick < section.end
                                    && matches!(e.data[0] >> 4, 0xb | 0xc | 0xe))
                        })
                        .cloned()
                        .collect();
                    let mut prefix = Vec::new();
                    for (_, event) in baseline.values() {
                        if track == 0
                            && !copied.iter().any(|e| {
                                e.tick == section.start
                                    && e.data[0] >> 4 == event.data[0] >> 4
                                    && (e.data[0] >> 4 != 0xb || e.data[1] == event.data[1])
                            })
                        {
                            let mut event = event.clone();
                            event.tick = section.start;
                            prefix.push(event);
                        }
                    }
                    prefix.append(&mut copied);
                    for mut e in prefix {
                        cloned_ids.insert(e.id, next);
                        e.id = next;
                        e.order = next;
                        next += 1;
                        e.data[0] = (e.data[0] & 0xf0) | (target - 1);
                        events.push(e);
                    }
                    events.sort_by_key(|e| (e.tick, e.order, e.id));
                }
                for note in current.notes.iter().filter(|n| note_ids.contains(&n.id)) {
                    cloned_pairs.push((cloned_ids[&note.id], cloned_ids[&note.off_id]));
                }
            }
        }
    }
    Ok(cloned_pairs)
}
