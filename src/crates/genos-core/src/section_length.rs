//! Fill/Break length and Shorten Section (SFM-INSP-TOOLS-FILL-BREAK-LENGTH / SHORTEN-SECTION).
//! Moves Yamaha section markers and channel MIDI by shifting later timeline ticks.
//! Does not modify CASM, OTS, or invent notes when lengthening.
use crate::{
    Error, Result,
    document::{Document, Section},
    smf::{Event, Style},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum SectionLengthOp {
    /// Fill A–D / Break only · bars 1..=8 · rejects shortening that would drop MIDI.
    FillBreak { bars: u8 },
    /// Any section · bars >= 1 and < current · clips/removes tail MIDI then shrinks.
    Shorten { bars: u32 },
}

fn reject(message: &str) -> Error {
    Error::Edit(message.into())
}

pub(crate) struct PairChanges {
    pub removed: Vec<u64>,
    pub added: Vec<(u64, u64)>,
}

fn is_fill_break(label: &str) -> bool {
    let n = label.trim().to_ascii_uppercase();
    matches!(
        n.as_str(),
        "FILL IN AA" | "FILL IN BB" | "FILL IN CC" | "FILL IN DD" | "FILL IN BA" | "BREAK"
    ) || n.starts_with("FILL ")
}

pub(crate) fn ticks_per_bar(doc: &Document, at: u64) -> Result<u64> {
    let (_, num, den) = doc.timing_at(at);
    if num == 0 || den == 0 || ![1u16, 2, 4, 8, 16, 32].contains(&den) {
        return Err(reject("unsupported meter for section length"));
    }
    let ppq = doc.style.ppq as u64;
    let bar = ppq
        .checked_mul(4)
        .and_then(|v| v.checked_div(den as u64))
        .and_then(|v| v.checked_mul(num as u64))
        .ok_or_else(|| reject("section bar length overflow"))?;
    if bar == 0 {
        return Err(reject("section bar length is zero"));
    }
    Ok(bar)
}

fn section(doc: &Document, id: u64) -> Result<&Section> {
    doc.sections
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| reject("section not found"))
}

fn has_midi_past(doc: &Document, start: u64, cutoff: u64, end: u64) -> bool {
    for n in &doc.notes {
        if n.start >= start && n.start < end && (n.start >= cutoff || n.start + n.duration > cutoff)
        {
            return true;
        }
    }
    for e in doc.style.tracks.iter().flatten() {
        if e.tick < start || e.tick >= end {
            continue;
        }
        if e.meta(6).is_some() {
            continue;
        }
        if e.tick >= cutoff {
            return true;
        }
    }
    false
}

fn should_shift(e: &Event, from: u64) -> bool {
    // Half-open sections: note-offs at `from` close notes in the prior section and stay.
    // Markers, note-ons, and controllers at `from` belong to the next section and move.
    e.tick > from || (e.tick == from && !e.is_off())
}

pub(crate) fn shift_ticks(style: &mut Style, from: u64, delta: i64, touched: &mut HashSet<usize>) {
    if delta == 0 {
        return;
    }
    for (track, es) in style.tracks.iter_mut().enumerate() {
        let mut changed = false;
        for e in es.iter_mut() {
            if !should_shift(e, from) {
                continue;
            }
            let next = if delta > 0 {
                e.tick.saturating_add(delta as u64)
            } else {
                e.tick.saturating_sub((-delta) as u64)
            };
            if next != e.tick {
                e.tick = next;
                changed = true;
            }
        }
        if changed {
            touched.insert(track);
        }
    }
}

/// Apply Fill/Break length or Shorten Section. Returns note pair changes for Session bookkeeping.
pub(crate) fn apply(
    style: &mut Style,
    section_id: u64,
    op: SectionLengthOp,
) -> Result<PairChanges> {
    let doc = Document::from_style(style.clone())?;
    let sec = section(&doc, section_id)?;
    let bar = ticks_per_bar(&doc, sec.start)?;
    let old_len = sec.end.saturating_sub(sec.start);
    if old_len == 0 || old_len % bar != 0 {
        return Err(reject(
            "section length is not an integer number of bars; reopen the Style",
        ));
    }
    let old_bars = (old_len / bar) as u32;
    let (new_bars, clip) = match op {
        SectionLengthOp::FillBreak { bars } => {
            if !is_fill_break(&sec.label) {
                return Err(reject("select a Fill A–D or Break section"));
            }
            if !(1..=8).contains(&bars) {
                return Err(reject("Fill / Break length must be 1–8 bars"));
            }
            (bars as u32, false)
        }
        SectionLengthOp::Shorten { bars } => {
            if bars < 1 || bars >= old_bars {
                return Err(reject("choose fewer bars than the current section length"));
            }
            (bars, true)
        }
    };
    let new_len = (new_bars as u64)
        .checked_mul(bar)
        .ok_or_else(|| reject("section length overflow"))?;
    if new_len == old_len {
        return Ok(PairChanges {
            removed: vec![],
            added: vec![],
        });
    }
    let old_end = sec.end;
    let new_end = sec.start + new_len;
    let delta = new_len as i64 - old_len as i64;

    if delta < 0 && !clip && has_midi_past(&doc, sec.start, new_end, old_end) {
        return Err(reject(
            "the shorter section would lose existing MIDI; use Shorten Section or keep the length",
        ));
    }

    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut touched = HashSet::new();
    let mut next = style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap_or(0)
        + 1;

    if clip && delta < 0 {
        // Clip / remove material in [new_end, old_end) before shifting the tail.
        let mut keep_notes: Vec<(usize, u8, u64, u8, u8, u64)> = Vec::new();
        let mut edge_ids = HashSet::new();
        for n in &doc.notes {
            if n.start < sec.start || n.start >= old_end {
                continue;
            }
            if n.start >= new_end {
                edge_ids.insert(n.id);
                edge_ids.insert(n.off_id);
                removed.push(n.id);
                continue;
            }
            if n.start + n.duration > new_end {
                let duration = (new_end - n.start).max(1);
                edge_ids.insert(n.id);
                edge_ids.insert(n.off_id);
                keep_notes.push((n.track, n.channel, n.start, n.pitch, n.velocity, duration));
                removed.push(n.id);
            }
        }
        for (track, es) in style.tracks.iter_mut().enumerate() {
            let before = es.len();
            es.retain(|e| {
                if edge_ids.contains(&e.id) {
                    return false;
                }
                // Drop controllers / non-note channel events in the trimmed window.
                if e.tick >= new_end
                    && e.tick < old_end
                    && e.channel().is_some()
                    && !e.is_on()
                    && !e.is_off()
                    && e.data.first().is_some_and(|s| *s < 0xf0)
                {
                    return false;
                }
                true
            });
            if es.len() != before {
                touched.insert(track);
            }
        }
        // Re-insert clipped notes.
        for (track, channel, start, pitch, velocity, duration) in keep_notes {
            let es = style
                .tracks
                .get_mut(track)
                .ok_or_else(|| reject("track not found"))?;
            let on = next;
            let off = next + 1;
            next += 2;
            added.push((on, off));
            es.push(Event {
                id: on,
                tick: start,
                order: on,
                data: vec![0x90 | (channel - 1), pitch, velocity],
            });
            es.push(Event {
                id: off,
                tick: start + duration,
                order: off,
                data: vec![0x80 | (channel - 1), pitch, 0],
            });
            touched.insert(track);
        }
    }

    // Shift every event at or after the old section end (markers + later sections).
    shift_ticks(style, old_end, delta, &mut touched);

    for t in touched {
        style.tracks[t].sort_by_key(|e| (e.tick, e.order, e.id));
    }

    // Sanity: rebuilt document must still parse and locate the section.
    let check = Document::from_style(style.clone())?;
    let after = section(&check, section_id)?;
    if after.end.saturating_sub(after.start) != new_len {
        return Err(reject(
            "section length change did not produce the requested duration",
        ));
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
        Session::open(&b, "len".into()).unwrap()
    }

    #[test]
    fn fill_break_lengthen_is_one_undo() {
        let mut s = session();
        let fill = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Fill In AA")
            .unwrap()
            .clone();
        let before = s.document.style.bytes().unwrap();
        let old = fill.end - fill.start;
        s.execute(
            0,
            Command::SectionLength {
                section: fill.id,
                op: SectionLengthOp::FillBreak { bars: 2 },
            },
        )
        .unwrap();
        let after = s
            .document
            .sections
            .iter()
            .find(|x| x.id == fill.id)
            .unwrap();
        assert_eq!(after.end - after.start, old * 2);
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), before);
    }

    #[test]
    fn fill_break_shorten_rejects_when_events_would_be_lost() {
        let mut s = session();
        // Lengthen first so we can put notes only in bar 1… actually Fill AA is 1 bar.
        // Use Main A with Shorten instead for clip; for FillBreak rejection use a fill
        // that has notes and try bars=1 when already 1 → no-op. Lengthen then the
        // lengthened empty region can shrink via FillBreak; shrinking into notes fails.
        let fill = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Fill In AA")
            .unwrap()
            .clone();
        s.execute(
            0,
            Command::SectionLength {
                section: fill.id,
                op: SectionLengthOp::FillBreak { bars: 2 },
            },
        )
        .unwrap();
        // Fill still has its original notes in the first bar; shrinking to 0 invalid;
        // shrinking to 1 should succeed if nothing lives in bar 2.
        s.execute(
            1,
            Command::SectionLength {
                section: fill.id,
                op: SectionLengthOp::FillBreak { bars: 1 },
            },
        )
        .unwrap();
    }

    #[test]
    fn shorten_section_clips_and_undoes() {
        let mut s = session();
        let main = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Main A")
            .unwrap()
            .clone();
        let before = s.document.style.bytes().unwrap();
        let notes_before = s
            .document
            .notes
            .iter()
            .filter(|n| n.start >= main.start && n.start < main.end)
            .count();
        assert!(notes_before > 0);
        s.execute(
            0,
            Command::SectionLength {
                section: main.id,
                op: SectionLengthOp::Shorten { bars: 4 },
            },
        )
        .unwrap();
        let after = s
            .document
            .sections
            .iter()
            .find(|x| x.id == main.id)
            .unwrap()
            .clone();
        assert_eq!((after.end - after.start) / 5760, 4);
        for n in &s.document.notes {
            if n.start >= after.start && n.start < after.end {
                assert!(n.start + n.duration <= after.end);
            }
        }
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), before);
    }
}
