//! Copy notes and channel controllers between Style sections (SFM-026 parity).
//! CASM routes, SysEx, OTS, and project tempo/meter are not copied.
use crate::{
    Error, Result,
    document::{Document, Note, Section},
    exchange::PhraseNote,
    smf::{Event, Style},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum CopyScope {
    /// Every channel that has notes or channel events in the source section.
    Section,
    /// All source channels that feed the same destination part as `channel`, or that channel alone.
    Part { channel: u8 },
    /// A single source channel.
    Source { channel: u8 },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum CopyFit {
    /// Keep relative timing; reject if content exceeds the destination length.
    Keep,
    /// Tile the source phrase into the destination length (trim the last copy).
    Repeat,
    /// Scale relative timing to the destination length.
    Fit,
}

fn reject(message: &str) -> Error {
    Error::Edit(message.into())
}

fn section(doc: &Document, id: u64) -> Result<&Section> {
    doc.sections
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| reject("section not found"))
}

fn channels_for_scope(doc: &Document, src: &Section, scope: &CopyScope) -> Result<Vec<u8>> {
    match scope {
        CopyScope::Source { channel } => {
            if !(1..=16).contains(channel) {
                return Err(reject("source channel must be 1..16"));
            }
            Ok(vec![*channel])
        }
        CopyScope::Part { channel } => {
            if !(1..=16).contains(channel) {
                return Err(reject("part channel must be 1..16"));
            }
            let dest = doc
                .routes
                .iter()
                .find(|r| r.source == *channel && r.sections.contains(&src.label))
                .map(|r| r.destination)
                .unwrap_or(*channel);
            let mut set: HashSet<u8> = doc
                .routes
                .iter()
                .filter(|r| r.destination == dest && r.sections.contains(&src.label))
                .map(|r| r.source)
                .collect();
            if set.is_empty() {
                set.insert(*channel);
            }
            let mut list: Vec<_> = set.into_iter().collect();
            list.sort_unstable();
            Ok(list)
        }
        CopyScope::Section => {
            let mut set = HashSet::new();
            for n in &doc.notes {
                if n.start >= src.start && n.start < src.end {
                    set.insert(n.channel);
                }
            }
            for e in doc.style.tracks.iter().flatten() {
                if let Some(ch) = e.channel()
                    && e.tick >= src.start
                    && e.tick < src.end
                    && !e.is_on()
                    && !e.is_off()
                    && e.data.first().is_some_and(|s| *s < 0xf0)
                {
                    set.insert(ch);
                }
            }
            if set.is_empty() {
                return Err(reject("source section has nothing to copy"));
            }
            let mut list: Vec<_> = set.into_iter().collect();
            list.sort_unstable();
            Ok(list)
        }
    }
}

fn notes_in<'a>(doc: &'a Document, s: &Section, channel: u8) -> Result<Vec<&'a Note>> {
    let mut notes = Vec::new();
    for n in doc.notes.iter().filter(|n| n.channel == channel) {
        if n.start < s.end && n.start + n.duration > s.start {
            if n.start < s.start || n.start + n.duration > s.end {
                return Err(reject(
                    "source notes cross the section boundary; resolve them before copy",
                ));
            }
            notes.push(n);
        }
    }
    Ok(notes)
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

fn scale_len(tick: u64, src_len: u64, dest_len: u64) -> u64 {
    if src_len == 0 {
        return 0;
    }
    // Same half-even rounding as `scale_tick`, without truncating lengths to u16.
    let n = tick as u128 * dest_len as u128;
    let d = src_len as u128;
    let q = n / d;
    let r = n % d;
    (q + u128::from(r * 2 > d || (r * 2 == d && q % 2 == 1))) as u64
}

fn reject_overlaps(phrase: &[PhraseNote], length: u64) -> Result<()> {
    let mut ordered: Vec<_> = phrase.iter().collect();
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
                "copied note is outside MIDI or section bounds; notes are not clipped",
            ));
        }
        if ends.get(&n.pitch).is_some_and(|end| *end > n.start) {
            return Err(reject(
                "overlapping same-pitch notes are ambiguous for copy",
            ));
        }
        ends.insert(n.pitch, n.start + n.duration);
    }
    Ok(())
}

fn map_phrase(
    phrase: &[PhraseNote],
    src_len: u64,
    dest_len: u64,
    fit: &CopyFit,
) -> Result<Vec<PhraseNote>> {
    if src_len == 0 || dest_len == 0 {
        return Err(reject("section has no duration"));
    }
    let out = match fit {
        CopyFit::Keep => {
            for n in phrase {
                if n.start
                    .checked_add(n.duration)
                    .is_none_or(|end| end > dest_len)
                {
                    return Err(reject(
                        "source phrase is longer than the destination; choose Repeat or Fit",
                    ));
                }
            }
            phrase.to_vec()
        }
        CopyFit::Fit => {
            let mut out = Vec::with_capacity(phrase.len());
            for n in phrase {
                let start = scale_len(n.start, src_len, dest_len);
                let end = scale_len(n.start + n.duration, src_len, dest_len);
                let duration = end.saturating_sub(start).max(1);
                if start >= dest_len || start + duration > dest_len {
                    continue;
                }
                out.push(PhraseNote {
                    start,
                    pitch: n.pitch,
                    duration,
                    velocity: n.velocity,
                });
            }
            out
        }
        CopyFit::Repeat => {
            if phrase.is_empty() {
                return Ok(vec![]);
            }
            let mut out = Vec::new();
            let mut offset = 0u64;
            while offset < dest_len {
                let tile_end = (offset + src_len).min(dest_len);
                for n in phrase {
                    let start = offset + n.start;
                    if start >= tile_end {
                        break;
                    }
                    // Clamp to tile end so successive tiles cannot overlap same pitch.
                    let duration = n.duration.min(tile_end - start);
                    if duration == 0 {
                        continue;
                    }
                    out.push(PhraseNote {
                        start,
                        pitch: n.pitch,
                        duration,
                        velocity: n.velocity,
                    });
                }
                offset = offset.saturating_add(src_len);
                if src_len == 0 {
                    break;
                }
            }
            out
        }
    };
    reject_overlaps(&out, dest_len)?;
    Ok(out)
}

fn map_controller_tick(
    rel: u64,
    src_len: u64,
    dest_len: u64,
    fit: &CopyFit,
) -> Result<Option<u64>> {
    match fit {
        CopyFit::Keep => {
            if rel >= dest_len {
                return Err(reject(
                    "source controllers extend past the destination; choose Repeat or Fit",
                ));
            }
            Ok(Some(rel))
        }
        CopyFit::Fit => {
            let t = scale_len(rel, src_len, dest_len);
            Ok((t < dest_len).then_some(t))
        }
        CopyFit::Repeat => {
            if rel >= src_len || rel >= dest_len {
                return Ok(None);
            }
            Ok(Some(rel))
        }
    }
}

fn is_copyable_controller(e: &Event) -> bool {
    let Some(status) = e.data.first() else {
        return false;
    };
    if *status >= 0xf0 {
        return false;
    }
    if e.is_on() || e.is_off() {
        return false;
    }
    // Bank select is allowed; reserved channel mode messages are not.
    if status >> 4 == 0xb && e.data.get(1).is_some_and(|cc| matches!(cc, 120..=127)) {
        return false;
    }
    true
}

pub(crate) struct PairChanges {
    pub removed: Vec<u64>,
    pub added: Vec<(u64, u64)>,
}

/// Replace destination material for the selected scope with a mapped copy of the source.
pub(crate) fn apply(
    style: &mut Style,
    source_section: u64,
    destination_section: u64,
    scope: CopyScope,
    fit: CopyFit,
) -> Result<PairChanges> {
    if source_section == destination_section {
        return Err(reject("choose a different destination section"));
    }
    let doc = Document::from_style(style.clone())?;
    let src = section(&doc, source_section)?;
    let dest = section(&doc, destination_section)?;
    let channels = channels_for_scope(&doc, src, &scope)?;
    let src_len = src.end - src.start;
    let dest_len = dest.end - dest.start;
    let src_start = src.start;
    let dest_start = dest.start;
    let dest_end = dest.end;

    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut next = style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap_or(0)
        + 1;
    let mut touched = HashSet::new();

    // Capture source material before mutating.
    type PlannedChannel = (u8, Vec<PhraseNote>, Vec<(u64, Vec<u8>)>, usize);
    let mut planned: Vec<PlannedChannel> = Vec::new();
    for channel in channels {
        // Destination must not already have notes that cross the section edge.
        notes_in(&doc, dest, channel)?;
        let notes = notes_in(&doc, src, channel)?;
        let phrase = map_phrase(&relative(&notes, src_start), src_len, dest_len, &fit)?;
        let mut controllers = Vec::new();
        for e in doc.style.tracks.iter().flatten() {
            if e.channel() != Some(channel) || !is_copyable_controller(e) {
                continue;
            }
            if e.tick < src_start || e.tick >= src.end {
                continue;
            }
            let rel = e.tick - src_start;
            if let Some(mapped) = map_controller_tick(rel, src_len, dest_len, &fit)? {
                controllers.push((mapped, e.data.clone()));
            }
        }
        if matches!(fit, CopyFit::Repeat) && !controllers.is_empty() {
            let base = controllers.clone();
            let mut offset = src_len;
            while offset < dest_len {
                for (rel, data) in &base {
                    let tick = offset + rel;
                    if tick < dest_len {
                        controllers.push((tick, data.clone()));
                    }
                }
                offset = offset.saturating_add(src_len);
            }
        }
        let track = notes
            .first()
            .map(|n| n.track)
            .or_else(|| {
                doc.style.tracks.iter().enumerate().find_map(|(i, es)| {
                    es.iter()
                        .any(|e| {
                            e.channel() == Some(channel)
                                && e.tick >= src_start
                                && e.tick < src.end
                                && is_copyable_controller(e)
                        })
                        .then_some(i)
                })
            })
            .or_else(|| {
                doc.style
                    .tracks
                    .iter()
                    .position(|es| es.iter().any(|e| e.channel() == Some(channel)))
            });
        let track = match track {
            Some(t) => t,
            None if phrase.is_empty() && controllers.is_empty() => continue,
            None => {
                return Err(reject(
                    "no MIDI track found for a channel in the copy scope",
                ));
            }
        };
        planned.push((channel, phrase, controllers, track));
    }
    if planned.is_empty() {
        return Err(reject("source section has nothing to copy"));
    }

    // Clear destination notes by edge id (includes offs at dest_end) and copyable CCs.
    for (channel, _, _, _) in &planned {
        let channel = *channel;
        let dest_notes: Vec<_> = doc
            .notes
            .iter()
            .filter(|n| n.channel == channel && n.start >= dest_start && n.start < dest_end)
            .collect();
        removed.extend(dest_notes.iter().map(|n| n.id));
        let edge_ids: HashSet<_> = dest_notes.iter().flat_map(|n| [n.id, n.off_id]).collect();
        for (track, es) in style.tracks.iter_mut().enumerate() {
            let before = es.len();
            es.retain(|e| {
                if edge_ids.contains(&e.id) {
                    return false;
                }
                let same = e.channel() == Some(channel);
                let in_dest = e.tick >= dest_start && e.tick < dest_end;
                if same && in_dest && is_copyable_controller(e) {
                    return false;
                }
                true
            });
            if es.len() != before {
                touched.insert(track);
            }
        }
    }

    for (channel, phrase, controllers, track) in planned {
        let es = style
            .tracks
            .get_mut(track)
            .ok_or_else(|| reject("track not found"))?;
        // FIFO pairing on write (same as exchange::replace).
        let mut edges = Vec::new();
        for n in &phrase {
            edges.push((dest_start + n.start, 1u8, n.pitch, n.velocity));
            edges.push((dest_start + n.start + n.duration, 0u8, n.pitch, 0u8));
        }
        edges.sort();
        let mut active = HashMap::new();
        for (tick, kind, pitch, velocity) in edges {
            let id = next;
            next += 1;
            if kind == 1 {
                active.insert(pitch, id);
            } else {
                let on = active
                    .remove(&pitch)
                    .ok_or_else(|| reject("invalid phrase pairing"))?;
                added.push((on, id));
            }
            es.push(Event {
                id,
                tick,
                order: id,
                data: vec![
                    (if kind == 1 { 0x90 } else { 0x80 }) | (channel - 1),
                    pitch,
                    velocity,
                ],
            });
            touched.insert(track);
        }
        // Deduplicate controllers (last wins); BTreeMap keeps (tick, status, cc) stable.
        let mut by_key: BTreeMap<(u64, u8, u8), Vec<u8>> = BTreeMap::new();
        for (rel, data) in controllers {
            let key0 = data[0];
            let key1 = data.get(1).copied().unwrap_or(0);
            by_key.insert((rel, key0, key1), data);
        }
        for ((rel, _, _), data) in by_key {
            es.push(Event {
                id: next,
                tick: dest_start + rel,
                order: next,
                data,
            });
            next += 1;
            touched.insert(track);
        }
    }

    for t in touched {
        style.tracks[t].sort_by_key(|e| (e.tick, e.order, e.id));
    }
    Ok(PairChanges { removed, added })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Command, Session};
    use std::{fs, path::Path};

    fn session() -> Session {
        let b = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
        )
        .unwrap();
        Session::open(&b, "copy".into()).unwrap()
    }

    #[test]
    fn copy_source_keep_rejects_when_destination_is_shorter() {
        let mut s = session();
        let main = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Main A")
            .unwrap();
        let fill = s
            .document
            .sections
            .iter()
            .find(|x| x.label.starts_with("Fill") && (x.end - x.start) < (main.end - main.start))
            .unwrap();
        let ch = s
            .document
            .notes
            .iter()
            .find(|n| n.start >= main.start && n.start < main.end)
            .unwrap()
            .channel;
        let err = s.execute(
            0,
            Command::CopyMaterial {
                source_section: main.id,
                destination_section: fill.id,
                scope: CopyScope::Source { channel: ch },
                fit: CopyFit::Keep,
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn copy_source_fit_is_one_undo() {
        let mut s = session();
        let main = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Main A")
            .unwrap()
            .clone();
        let fill = s
            .document
            .sections
            .iter()
            .find(|x| x.label.starts_with("Fill"))
            .unwrap()
            .clone();
        let ch = s
            .document
            .notes
            .iter()
            .find(|n| n.start >= main.start && n.start < main.end)
            .unwrap()
            .channel;
        let before = s.document.style.bytes().unwrap();
        s.execute(
            0,
            Command::CopyMaterial {
                source_section: main.id,
                destination_section: fill.id,
                scope: CopyScope::Source { channel: ch },
                fit: CopyFit::Fit,
            },
        )
        .unwrap();
        assert!(s.view().can_undo);
        assert_ne!(s.document.style.bytes().unwrap(), before);
        let dest_notes: Vec<_> = s
            .document
            .notes
            .iter()
            .filter(|n| n.channel == ch && n.start >= fill.start && n.start < fill.end)
            .collect();
        assert!(!dest_notes.is_empty());
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), before);
    }

    #[test]
    fn copy_section_repeat_tiles_into_longer_destination() {
        let mut s = session();
        let fill = s
            .document
            .sections
            .iter()
            .find(|x| x.label.starts_with("Fill"))
            .unwrap()
            .clone();
        let main = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Main A")
            .unwrap()
            .clone();
        assert!((main.end - main.start) > (fill.end - fill.start));
        let fill_notes_before = s
            .document
            .notes
            .iter()
            .filter(|n| n.start >= fill.start && n.start < fill.end)
            .count();
        assert!(fill_notes_before > 0);
        s.execute(
            0,
            Command::CopyMaterial {
                source_section: fill.id,
                destination_section: main.id,
                scope: CopyScope::Section,
                fit: CopyFit::Repeat,
            },
        )
        .unwrap();
        let dest_notes = s
            .document
            .notes
            .iter()
            .filter(|n| n.start >= main.start && n.start < main.end)
            .count();
        assert!(dest_notes >= fill_notes_before);
        s.undo().unwrap();
    }
}
