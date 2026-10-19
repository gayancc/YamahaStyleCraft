//! Independent notes-only SMF exchange. No SFM session/canonicalization compatibility claim.
use crate::{
    Error, Result,
    document::{Document, Note},
    smf::{self, Chunk, Event, Style},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_MIDI_BYTES: usize = 8 * 1024 * 1024;
const MAX_NOTES: usize = 50_000;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhraseNote {
    pub start: u64,
    pub pitch: u8,
    pub duration: u64,
    pub velocity: u8,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub source_ppq: u16,
    pub target_ppq: u16,
    pub source_channel: u8,
    pub target_channel: u8,
    pub section: String,
    pub original_notes: usize,
    pub imported_notes: usize,
    pub ignored_events: usize,
    pub end_tick: u64,
    pub unchanged: bool,
    pub note_preview: Vec<PhraseNote>,
    /// When set, import replaces multiple source channels in one undo step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<ChannelPreview>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPreview {
    pub channel: u8,
    pub original_notes: usize,
    pub imported_notes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelPhrase {
    pub channel: u8,
    pub notes: Vec<PhraseNote>,
}
pub struct Prepared {
    pub preview: Preview,
    pub notes: Vec<PhraseNote>,
    /// Multi-channel replace payload (SFM-121). Empty for single-source exchange.
    pub phrases: Vec<ChannelPhrase>,
}
fn reject(message: &str) -> Error {
    Error::Edit(message.into())
}

fn scope(
    doc: &Document,
    section: u64,
    channel: u8,
) -> Result<(&crate::document::Section, Vec<&Note>)> {
    if !(1..=16).contains(&channel) {
        return Err(reject("source channel must be 1..16"));
    }
    let s = doc
        .sections
        .iter()
        .find(|s| s.id == section)
        .ok_or_else(|| reject("section not found"))?;
    if s.end <= s.start {
        return Err(reject("section has no duration"));
    }
    let mut notes = Vec::new();
    for n in doc.notes.iter().filter(|n| n.channel == channel) {
        if n.start < s.end && n.start + n.duration > s.start {
            if n.start < s.start || n.start + n.duration > s.end {
                return Err(reject(
                    "source notes cross the section boundary; resolve them before exchange",
                ));
            }
            notes.push(n);
        }
    }
    let edges: HashSet<_> = notes.iter().flat_map(|n| [n.id, n.off_id]).collect();
    let ticks: HashMap<_, _> = doc
        .style
        .tracks
        .iter()
        .flatten()
        .map(|e| (e.id, e.tick))
        .collect();
    for n in &notes {
        if ticks[&n.off_id] <= n.start {
            return Err(reject(
                "source has zero-duration notes; resolve them before exchange",
            ));
        }
    }
    for e in doc
        .style
        .tracks
        .iter()
        .flatten()
        .filter(|e| e.channel() == Some(channel) && (e.is_on() || e.is_off()))
    {
        if e.tick >= s.start && e.tick < s.end && !edges.contains(&e.id) {
            return Err(reject(
                "source has unpaired note events; resolve them before exchange",
            ));
        }
    }
    let phrase = relative(&notes, s.start);
    validate(&phrase, s.end - s.start)?;
    Ok((s, notes))
}
fn relative(notes: &[&Note], start: u64) -> Vec<PhraseNote> {
    let mut result: Vec<_> = notes
        .iter()
        .map(|n| PhraseNote {
            start: n.start - start,
            pitch: n.pitch,
            duration: n.duration,
            velocity: n.velocity,
        })
        .collect();
    result.sort();
    result
}
fn validate(notes: &[PhraseNote], length: u64) -> Result<()> {
    if notes.len() > MAX_NOTES {
        return Err(reject("phrase exceeds 50000 notes"));
    }
    let mut ordered: Vec<_> = notes.iter().collect();
    ordered.sort();
    let mut ends = HashMap::new();
    for n in ordered {
        if n.pitch > 127
            || !(1..=127).contains(&n.velocity)
            || n.duration == 0
            || n.start
                .checked_add(n.duration)
                .is_none_or(|end| end > length)
        {
            return Err(reject(
                "imported note is outside MIDI or section bounds; notes are not clipped",
            ));
        }
        if ends.get(&n.pitch).is_some_and(|end| *end > n.start) {
            return Err(reject(
                "overlapping same-pitch notes are ambiguous for MIDI exchange",
            ));
        }
        ends.insert(n.pitch, n.start + n.duration);
    }
    Ok(())
}

/// Exports a single source phrase at tick zero with the original PPQ and timing map.
/// No voices, controllers, SysEx, CASM, or other source notes are sent to the DAW.
pub fn export(doc: &Document, section: u64, channel: u8) -> Result<Vec<u8>> {
    let (s, notes) = scope(doc, section, channel)?;
    if notes.is_empty() {
        return Err(reject("selected source has no notes to export"));
    }
    let mut events = Vec::new();
    let mut add = |tick, data| {
        let id = events.len() as u64 + 1;
        events.push(Event {
            id,
            tick,
            order: id,
            data,
        });
    };
    let mut all: Vec<_> = doc.style.tracks.iter().flatten().collect();
    all.sort_by_key(|e| (e.tick, e.order));
    for kind in [0x51, 0x58] {
        let baseline = all
            .iter()
            .rev()
            .find(|e| e.tick <= s.start && e.meta(kind).is_some());
        let fallback = if kind == 0x51 {
            vec![0xff, 0x51, 3, 7, 161, 32]
        } else {
            vec![0xff, 0x58, 4, 4, 2, 24, 8]
        };
        add(0, baseline.map_or(fallback, |e| e.data.clone()));
    }
    for e in all.iter().filter(|e| {
        e.tick > s.start && e.tick < s.end && (e.meta(0x51).is_some() || e.meta(0x58).is_some())
    }) {
        add(e.tick - s.start, e.data.clone());
    }
    // Off before on at a shared tick; same-pitch overlaps were rejected above.
    let mut note_events = Vec::new();
    for n in notes {
        note_events.push((
            n.start - s.start,
            1,
            vec![0x90 | (channel - 1), n.pitch, n.velocity],
        ));
        note_events.push((
            n.start + n.duration - s.start,
            0,
            vec![0x80 | (channel - 1), n.pitch, 0],
        ));
    }
    note_events.sort_by_key(|(tick, kind, _)| (*tick, *kind));
    for (tick, _, data) in note_events {
        add(tick, data);
    }
    add(s.end - s.start, vec![0xff, 0x2f, 0]);
    let ppq = doc.style.ppq.to_be_bytes();
    let bytes = smf::pack(&[
        Chunk {
            tag: *b"MThd",
            data: vec![0, 0, 0, 1, ppq[0], ppq[1]],
        },
        Chunk {
            tag: *b"MTrk",
            data: smf::encode_track(&events)?,
        },
    ])?;
    if bytes.len() > MAX_MIDI_BYTES {
        return Err(reject("exchange MIDI exceeds 8 MiB"));
    }
    Ok(bytes)
}

pub fn prepare(doc: &Document, section: u64, channel: u8, bytes: &[u8]) -> Result<Prepared> {
    let (s, original) = scope(doc, section, channel)?;
    if bytes.len() > MAX_MIDI_BYTES {
        return Err(reject("exchange MIDI exceeds 8 MiB"));
    }
    let imported = Document::parse(bytes)?;
    if imported
        .style
        .chunks
        .iter()
        .any(|c| c.tag != *b"MThd" && c.tag != *b"MTrk")
    {
        return Err(reject(
            "choose a Standard MIDI file; Style/vendor chunks are not accepted for note exchange",
        ));
    }
    if imported.notes.is_empty() {
        return Err(reject(
            "MIDI has no complete notes; use the editor to delete notes explicitly",
        ));
    }
    if imported.notes.len() > MAX_NOTES {
        return Err(reject("phrase exceeds 50000 notes"));
    }
    let source_channel = imported.notes[0].channel;
    if imported.notes.iter().any(|n| n.channel != source_channel) {
        return Err(reject(
            "MIDI contains multiple note channels; export one source phrase from your DAW",
        ));
    }
    let edge_count = imported
        .style
        .tracks
        .iter()
        .flatten()
        .filter(|e| e.is_on() || e.is_off())
        .count();
    if edge_count != imported.notes.len() * 2 {
        return Err(reject("MIDI contains unpaired note events"));
    }
    let mut notes = Vec::new();
    let ticks: HashMap<_, _> = imported
        .style
        .tracks
        .iter()
        .flatten()
        .map(|e| (e.id, e.tick))
        .collect();
    for n in &imported.notes {
        let off_tick = ticks[&n.off_id];
        if off_tick <= n.start {
            return Err(reject("MIDI contains a zero-duration note"));
        }
        let start = crate::scale_tick(n.start, imported.style.ppq, doc.style.ppq);
        let end = crate::scale_tick(off_tick, imported.style.ppq, doc.style.ppq);
        notes.push(PhraseNote {
            start,
            pitch: n.pitch,
            duration: end.saturating_sub(start),
            velocity: n.velocity,
        });
    }
    notes.sort();
    validate(&notes, s.end - s.start)?;
    // Detect overlaps in source precision too: rounding may otherwise conceal them.
    let source_notes: Vec<_> = imported
        .notes
        .iter()
        .map(|n| PhraseNote {
            start: n.start,
            pitch: n.pitch,
            duration: n.duration,
            velocity: n.velocity,
        })
        .collect();
    validate(&source_notes, smf::MAX_TICK)?;
    let preview = Preview {
        source_ppq: imported.style.ppq,
        target_ppq: doc.style.ppq,
        source_channel,
        target_channel: channel,
        section: s.label.clone(),
        original_notes: original.len(),
        imported_notes: notes.len(),
        ignored_events: imported
            .style
            .tracks
            .iter()
            .flatten()
            .filter(|e| !e.is_on() && !e.is_off() && e.meta(0x2f).is_none())
            .count(),
        end_tick: notes
            .iter()
            .map(|n| n.start + n.duration)
            .max()
            .unwrap_or(0),
        unchanged: notes == relative(&original, s.start),
        note_preview: notes.iter().take(64).cloned().collect(),
        channels: vec![],
    };
    Ok(Prepared {
        preview,
        notes,
        phrases: vec![],
    })
}

fn section_channels(doc: &Document, section: u64) -> Result<(&crate::document::Section, Vec<u8>)> {
    let s = doc
        .sections
        .iter()
        .find(|x| x.id == section)
        .ok_or_else(|| reject("section not found"))?;
    if s.end <= s.start {
        return Err(reject("section has no duration"));
    }
    let mut set = HashSet::new();
    for n in &doc.notes {
        if n.start >= s.start && n.start < s.end {
            set.insert(n.channel);
        }
    }
    if set.is_empty() {
        return Err(reject("section has no notes to exchange"));
    }
    let mut channels: Vec<_> = set.into_iter().collect();
    channels.sort_unstable();
    Ok((s, channels))
}

/// Type-1 SMF: conductor timing track + one track per source channel that has notes.
pub fn export_section(doc: &Document, section: u64) -> Result<Vec<u8>> {
    let (s, channels) = section_channels(doc, section)?;
    let mut chunks = Vec::new();
    let ppq = doc.style.ppq.to_be_bytes();
    let ntrks = (1 + channels.len()) as u16;
    chunks.push(Chunk {
        tag: *b"MThd",
        data: vec![0, 1, (ntrks >> 8) as u8, ntrks as u8, ppq[0], ppq[1]],
    });
    // Conductor track: tempo/meter only.
    let mut conductor = Vec::new();
    let add = |events: &mut Vec<Event>, tick, data: Vec<u8>| {
        let id = events.len() as u64 + 1;
        events.push(Event {
            id,
            tick,
            order: id,
            data,
        });
    };
    let mut all: Vec<_> = doc.style.tracks.iter().flatten().collect();
    all.sort_by_key(|e| (e.tick, e.order));
    for kind in [0x51u8, 0x58] {
        let baseline = all
            .iter()
            .rev()
            .find(|e| e.tick <= s.start && e.meta(kind).is_some());
        let fallback = if kind == 0x51 {
            vec![0xff, 0x51, 3, 7, 161, 32]
        } else {
            vec![0xff, 0x58, 4, 4, 2, 24, 8]
        };
        add(
            &mut conductor,
            0,
            baseline.map_or(fallback, |e| e.data.clone()),
        );
    }
    for e in all.iter().filter(|e| {
        e.tick > s.start && e.tick < s.end && (e.meta(0x51).is_some() || e.meta(0x58).is_some())
    }) {
        add(&mut conductor, e.tick - s.start, e.data.clone());
    }
    add(&mut conductor, s.end - s.start, vec![0xff, 0x2f, 0]);
    chunks.push(Chunk {
        tag: *b"MTrk",
        data: smf::encode_track(&conductor)?,
    });
    for channel in channels {
        let (_, notes) = scope(doc, section, channel)?;
        let mut events = Vec::new();
        let name = format!("Source {channel}");
        let mut name_bytes = vec![0xff, 0x03, name.len() as u8];
        name_bytes.extend(name.bytes());
        add(&mut events, 0, name_bytes);
        let mut note_events = Vec::new();
        for n in notes {
            note_events.push((
                n.start - s.start,
                1u8,
                vec![0x90 | (channel - 1), n.pitch, n.velocity],
            ));
            note_events.push((
                n.start + n.duration - s.start,
                0u8,
                vec![0x80 | (channel - 1), n.pitch, 0],
            ));
        }
        note_events.sort_by_key(|(tick, kind, _)| (*tick, *kind));
        for (tick, _, data) in note_events {
            add(&mut events, tick, data);
        }
        add(&mut events, s.end - s.start, vec![0xff, 0x2f, 0]);
        chunks.push(Chunk {
            tag: *b"MTrk",
            data: smf::encode_track(&events)?,
        });
    }
    let bytes = smf::pack(&chunks)?;
    if bytes.len() > MAX_MIDI_BYTES {
        return Err(reject("exchange MIDI exceeds 8 MiB"));
    }
    Ok(bytes)
}

/// Import a multi-track (or multi-channel) MIDI back into every matched source in the section.
pub fn prepare_section(doc: &Document, section: u64, bytes: &[u8]) -> Result<Prepared> {
    let (s, target_channels) = section_channels(doc, section)?;
    if bytes.len() > MAX_MIDI_BYTES {
        return Err(reject("exchange MIDI exceeds 8 MiB"));
    }
    let imported = Document::parse(bytes)?;
    if imported
        .style
        .chunks
        .iter()
        .any(|c| c.tag != *b"MThd" && c.tag != *b"MTrk")
    {
        return Err(reject(
            "choose a Standard MIDI file; Style/vendor chunks are not accepted for note exchange",
        ));
    }
    if imported.notes.is_empty() {
        return Err(reject(
            "MIDI has no complete notes; use the editor to delete notes explicitly",
        ));
    }
    if imported.notes.len() > MAX_NOTES {
        return Err(reject("phrase exceeds 50000 notes"));
    }
    let ticks: HashMap<_, _> = imported
        .style
        .tracks
        .iter()
        .flatten()
        .map(|e| (e.id, e.tick))
        .collect();
    let mut by_channel: HashMap<u8, Vec<PhraseNote>> = HashMap::new();
    for n in &imported.notes {
        let off_tick = ticks[&n.off_id];
        if off_tick <= n.start {
            return Err(reject("MIDI contains a zero-duration note"));
        }
        let start = crate::scale_tick(n.start, imported.style.ppq, doc.style.ppq);
        let end = crate::scale_tick(off_tick, imported.style.ppq, doc.style.ppq);
        by_channel.entry(n.channel).or_default().push(PhraseNote {
            start,
            pitch: n.pitch,
            duration: end.saturating_sub(start),
            velocity: n.velocity,
        });
    }
    let mut phrases = Vec::new();
    let mut channels_preview = Vec::new();
    let mut total_imported = 0usize;
    let mut total_original = 0usize;
    let mut unchanged = true;
    let mut preview_notes = Vec::new();
    for channel in &target_channels {
        let (_, original) = scope(doc, section, *channel)?;
        total_original += original.len();
        let mut notes = by_channel.remove(channel).unwrap_or_default();
        notes.sort();
        validate(&notes, s.end - s.start)?;
        if notes != relative(&original, s.start) {
            unchanged = false;
        }
        total_imported += notes.len();
        channels_preview.push(ChannelPreview {
            channel: *channel,
            original_notes: original.len(),
            imported_notes: notes.len(),
        });
        if preview_notes.len() < 64 {
            let room = 64 - preview_notes.len();
            preview_notes.extend(notes.iter().take(room).cloned());
        }
        phrases.push(ChannelPhrase {
            channel: *channel,
            notes,
        });
    }
    let unmatched: Vec<_> = by_channel.keys().copied().collect();
    if !unmatched.is_empty() {
        return Err(reject(&format!(
            "MIDI has unmatched note channel(s) {:?}; export the section again or remove extra tracks",
            unmatched
        )));
    }
    let ignored = imported
        .style
        .tracks
        .iter()
        .flatten()
        .filter(|e| !e.is_on() && !e.is_off() && e.meta(0x2f).is_none() && e.meta(0x03).is_none())
        .count();
    let end_tick = phrases
        .iter()
        .flat_map(|p| p.notes.iter())
        .map(|n| n.start + n.duration)
        .max()
        .unwrap_or(0);
    Ok(Prepared {
        preview: Preview {
            source_ppq: imported.style.ppq,
            target_ppq: doc.style.ppq,
            source_channel: 0,
            target_channel: 0,
            section: s.label.clone(),
            original_notes: total_original,
            imported_notes: total_imported,
            ignored_events: ignored,
            end_tick,
            unchanged,
            note_preview: preview_notes,
            channels: channels_preview,
        },
        notes: vec![],
        phrases,
    })
}

pub(crate) fn replace_section(
    style: &mut Style,
    section: u64,
    phrases: Vec<ChannelPhrase>,
) -> Result<PairChanges> {
    if phrases.is_empty() {
        return Err(reject("no phrases to replace"));
    }
    let mut removed = Vec::new();
    let mut added = Vec::new();
    for phrase in phrases {
        if phrase.notes.is_empty() {
            let doc = Document::from_style(style.clone())?;
            let (_, original) = scope(&doc, section, phrase.channel)?;
            if original.is_empty() {
                continue;
            }
            let edge_ids: HashSet<_> = original.iter().flat_map(|n| [n.id, n.off_id]).collect();
            removed.extend(original.iter().map(|n| n.id));
            for es in &mut style.tracks {
                es.retain(|e| !edge_ids.contains(&e.id));
            }
            continue;
        }
        let changes = replace(style, section, phrase.channel, phrase.notes)?;
        removed.extend(changes.removed);
        added.extend(changes.added);
    }
    Ok(PairChanges { removed, added })
}

pub(crate) struct PairChanges {
    pub removed: Vec<u64>,
    pub added: Vec<(u64, u64)>,
}
pub(crate) fn replace(
    style: &mut Style,
    section: u64,
    channel: u8,
    mut notes: Vec<PhraseNote>,
) -> Result<PairChanges> {
    let doc = Document::from_style(style.clone())?;
    let (s, original) = scope(&doc, section, channel)?;
    if notes.is_empty() {
        return Err(reject("empty exchange import is not allowed"));
    }
    validate(&notes, s.end - s.start)?;
    notes.sort();
    if notes == relative(&original, s.start) {
        return Ok(PairChanges {
            removed: vec![],
            added: vec![],
        });
    }
    let track = original.first().map_or(0, |n| n.track);
    let removed: Vec<_> = original.iter().map(|n| n.id).collect();
    let edge_ids: HashSet<_> = original.iter().flat_map(|n| [n.id, n.off_id]).collect();
    for es in &mut style.tracks {
        es.retain(|e| !edge_ids.contains(&e.id));
    }
    let mut next = doc
        .style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap_or(0)
        + 1;
    let mut edges = Vec::new();
    for n in notes {
        edges.push((s.start + n.start, 1, n.pitch, n.velocity));
        edges.push((s.start + n.start + n.duration, 0, n.pitch, 0));
    }
    edges.sort();
    let mut active = HashMap::new();
    let mut added = Vec::new();
    for (tick, kind, pitch, velocity) in edges {
        let id = next;
        next += 1;
        if kind == 1 {
            active.insert(pitch, id);
        } else {
            added.push((
                active
                    .remove(&pitch)
                    .ok_or_else(|| reject("invalid phrase pairing"))?,
                id,
            ));
        }
        style.tracks[track].push(Event {
            id,
            tick,
            order: id,
            data: vec![
                (if kind == 1 { 0x90 } else { 0x80 }) | (channel - 1),
                pitch,
                velocity,
            ],
        });
    }
    Ok(PairChanges { removed, added })
}
