//! Add a Yamaha section the Style does not contain yet (a Genos panel slot with no data).
//!
//! Evidence for the design (docs/development/fl-compliance/rea-2026-10-20/SECTIONS-EVIDENCE.md):
//! - StyleMagicYA's Sections Manager shows missing sections as empty slots that are filled by copying
//!   another section or importing one; its help says section operations also rewrite the section's
//!   `fn:` text description. Every section in a real Style starts with two events at one tick: the
//!   section marker (`Main A`) and the text event (`fn:Main A`).
//! - SFM Style Studio's section setup gives each section a length in bars: 1-8 for fills and the
//!   break, 1-32 otherwise.
//! - Real Styles (969 files surveyed) write sections in the order below.
//!
//! The new section is an empty region inserted at its position by shifting every later tick, the same
//! shifting Fill/Break length uses. No notes are invented. Its name joins an existing section's CASM
//! segment, so it shares that section's routes. Optionally the material of an existing section is
//! copied in through the existing, tested copy routine. Hardware acceptance is not verified.
use crate::{
    Error, Result,
    copy_material::{self, CopyFit, CopyScope, PairChanges},
    document::Document,
    smf::{self, Chunk, Event, Style},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Yamaha marker names in the order real Styles write them (Intro B, C, A is Yamaha's own order,
/// and the break fill `Fill In BA` comes last). A new section is placed by this order.
const ORDER: [&str; 15] = [
    "Main A",
    "Main B",
    "Main C",
    "Main D",
    "Fill In AA",
    "Fill In BB",
    "Fill In CC",
    "Fill In DD",
    "Intro B",
    "Intro C",
    "Intro A",
    "Ending A",
    "Ending B",
    "Ending C",
    "Fill In BA",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddSection {
    /// One of the Yamaha marker names (for example `Intro B`, `Main D`, `Fill In BA`).
    pub label: String,
    /// Length in bars: 1..=8 for fills and the break, 1..=32 otherwise.
    pub bars: u32,
    /// Existing section whose CASM routes the new section shares.
    pub casm_from: u64,
    /// Existing section whose notes and controllers are copied into the new section.
    #[serde(default)]
    pub copy_from: Option<u64>,
    /// How copied material is fitted to the new length (default: keep timing).
    #[serde(default)]
    pub fit: Option<CopyFit>,
}

fn reject(message: &str) -> Error {
    Error::Edit(message.into())
}

fn norm(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Position of a marker name in the real Style order, or `None` when it is not a standard section.
pub(crate) fn rank(label: &str) -> Option<usize> {
    let n = norm(label);
    ORDER.iter().position(|o| norm(o) == n)
}

fn is_fill_or_break(label: &str) -> bool {
    norm(label).starts_with("fill in")
}

fn meta_event(kind: u8, text: &[u8], tick: u64, id: u64) -> Result<Event> {
    let mut data = vec![0xff, kind];
    smf::write_vlq(text.len() as u64, &mut data)?;
    data.extend_from_slice(text);
    Ok(Event {
        id,
        tick,
        order: id,
        data,
    })
}

/// Add `label` to the CASM segment that covers `source_label`, so the new section shares its routes.
fn share_casm(style: &mut Style, source_label: &str, label: &str) -> Result<()> {
    for ci in 0..style.chunks.len() {
        if style.chunks[ci].tag != *b"CASM" {
            continue;
        }
        let mut groups = smf::chunks(&style.chunks[ci].data)?;
        for gi in 0..groups.len() {
            if groups[gi].tag != *b"CSEG" {
                continue;
            }
            let mut records = smf::chunks(&groups[gi].data)?;
            let Some(sdec) = records.iter_mut().find(|r| r.tag == *b"Sdec") else {
                continue;
            };
            let text = std::str::from_utf8(&sdec.data)
                .map_err(|_| reject("CASM section labels are not UTF-8"))?
                .trim_end_matches('\0')
                .to_string();
            let mut names: Vec<String> = text.split(',').map(|s| s.trim().to_string()).collect();
            if !names.iter().any(|n| norm(n) == norm(source_label)) {
                continue;
            }
            if names.iter().any(|n| norm(n) == norm(label)) {
                return Err(reject("the CASM already lists this section"));
            }
            names.push(label.to_string());
            *sdec = Chunk {
                tag: *b"Sdec",
                data: names.join(",").into_bytes(),
            };
            groups[gi].data = smf::pack(&records)?;
            style.chunks[ci].data = smf::pack(&groups)?;
            return Ok(());
        }
    }
    Err(reject(
        "the chosen section has no CASM routes to share; pick another section",
    ))
}

/// Insert the new section into `style`. Fails without changing `style` on any rejection.
pub(crate) fn apply(style: &mut Style, spec: AddSection) -> Result<PairChanges> {
    let spec_label = spec.label.split_whitespace().collect::<Vec<_>>().join(" ");
    let new_rank = rank(&spec_label).ok_or_else(|| reject("not a standard Yamaha section name"))?;
    let label = ORDER[new_rank].to_string();
    let max_bars = if is_fill_or_break(&label) { 8 } else { 32 };
    if !(1..=max_bars).contains(&spec.bars) {
        return Err(reject(if max_bars == 8 {
            "Fill / Break length must be 1–8 bars"
        } else {
            "section length must be 1–32 bars"
        }));
    }
    let doc = Document::from_style(style.clone())?;
    if doc.sections.len() == 1 && doc.sections[0].label == "Pattern" {
        return Err(reject("this file has no Yamaha section markers"));
    }
    if doc.sections.iter().any(|s| norm(&s.label) == norm(&label)) {
        return Err(reject("the Style already has this section"));
    }
    let source = doc
        .sections
        .iter()
        .find(|s| s.id == spec.casm_from)
        .ok_or_else(|| reject("section to share CASM routes with was not found"))?
        .clone();
    if let Some(copy) = spec.copy_from
        && !doc.sections.iter().any(|s| s.id == copy)
    {
        return Err(reject("section to copy material from was not found"));
    }

    // Position: before the first section that follows it in the real order, else at the very end.
    let insert_at = doc
        .sections
        .iter()
        .find(|s| rank(&s.label).is_some_and(|r| r > new_rank))
        .map(|s| s.start)
        .or_else(|| doc.sections.last().map(|s| s.end))
        .ok_or_else(|| reject("no section to place the new section against"))?;
    let bar = crate::section_length::ticks_per_bar(&doc, insert_at)?;
    let delta = (spec.bars as u64)
        .checked_mul(bar)
        .ok_or_else(|| reject("section length overflow"))?;
    let marker_track = style
        .tracks
        .iter()
        .position(|es| {
            es.iter().any(|e| {
                e.meta(6).is_some_and(|b| {
                    let text = String::from_utf8_lossy(b);
                    crate::document::musical_marker(text.trim_matches(char::from(0)).trim())
                })
            })
        })
        .ok_or_else(|| reject("no track carries the section markers"))?;

    let mut candidate = style.clone();
    let next = candidate
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap_or(0)
        + 1;
    let mut touched = HashSet::new();
    crate::section_length::shift_ticks(&mut candidate, insert_at, delta as i64, &mut touched);
    // Every Yamaha section starts with its marker and, at the same tick, its `fn:` description.
    let marker = meta_event(6, label.as_bytes(), insert_at, next)?;
    let description = meta_event(1, format!("fn:{label}\0").as_bytes(), insert_at, next + 1)?;
    candidate.tracks[marker_track].push(marker);
    candidate.tracks[marker_track].push(description);
    candidate.tracks[marker_track].sort_by_key(|e| (e.tick, e.order, e.id));
    for t in touched {
        candidate.tracks[t].sort_by_key(|e| (e.tick, e.order, e.id));
    }
    share_casm(&mut candidate, &source.label, &label)?;

    let after_insert = Document::from_style(candidate.clone())?;
    let added = after_insert
        .sections
        .iter()
        .find(|s| norm(&s.label) == norm(&label))
        .ok_or_else(|| reject("the new section was not found after insertion"))?
        .clone();
    if added.start != insert_at || added.end - added.start != delta {
        return Err(reject("the new section does not have the requested length"));
    }
    for old in &doc.sections {
        let kept = after_insert
            .sections
            .iter()
            .find(|s| s.id == old.id)
            .ok_or_else(|| reject("an existing section disappeared"))?;
        if kept.end - kept.start != old.end - old.start {
            return Err(reject("an existing section changed length"));
        }
    }
    if after_insert.notes.len() != doc.notes.len() {
        return Err(reject("note count changed while adding a section"));
    }

    let changes = match spec.copy_from {
        Some(copy) => copy_material::apply(
            &mut candidate,
            copy,
            added.id,
            CopyScope::Section,
            spec.fit.unwrap_or(CopyFit::Keep),
        )?,
        None => PairChanges {
            removed: Vec::new(),
            added: Vec::new(),
        },
    };
    *style = candidate;
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Command, Session};
    use std::{fs, path::Path};

    const BAR: u64 = 5760;

    /// The retained fixture is a real, sparse Style: it has no Fill In DD, Ending B or Ending C.
    fn fixture() -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
        )
        .unwrap()
    }

    /// A Style without `names`: their markers and `fn:` texts are removed and their names dropped
    /// from every CASM section list, as in a Style that never had those sections.
    fn style_without(names: &[&str]) -> Vec<u8> {
        let mut style = Style::parse(&fixture()).unwrap();
        for es in style.tracks.iter_mut() {
            es.retain(|e| {
                let text = |kind| {
                    e.meta(kind)
                        .map(|b| String::from_utf8_lossy(b).trim_matches('\0').to_string())
                };
                let marker = text(6).unwrap_or_default();
                let description = text(1).unwrap_or_default();
                !names.iter().any(|n| {
                    norm(n) == norm(&marker)
                        || description
                            .strip_prefix("fn:")
                            .is_some_and(|d| norm(d) == norm(n))
                })
            });
        }
        for ci in 0..style.chunks.len() {
            if style.chunks[ci].tag != *b"CASM" {
                continue;
            }
            let mut groups = smf::chunks(&style.chunks[ci].data).unwrap();
            for g in groups.iter_mut().filter(|g| g.tag == *b"CSEG") {
                let mut records = smf::chunks(&g.data).unwrap();
                for r in records.iter_mut().filter(|r| r.tag == *b"Sdec") {
                    let text = String::from_utf8_lossy(&r.data)
                        .trim_end_matches('\0')
                        .to_string();
                    let kept: Vec<_> = text
                        .split(',')
                        .map(str::trim)
                        .filter(|n| !names.iter().any(|d| norm(d) == norm(n)))
                        .collect();
                    r.data = kept.join(",").into_bytes();
                }
                g.data = smf::pack(&records).unwrap();
            }
            style.chunks[ci].data = smf::pack(&groups).unwrap();
        }
        // `bytes()` serializes the chunks, so write the edited tracks back into them.
        for t in 0..style.tracks.len() {
            style.chunks[style.track_chunks[t]].data = smf::encode_track(&style.tracks[t]).unwrap();
        }
        style.bytes().unwrap()
    }

    fn open(bytes: &[u8]) -> Session {
        Session::open(bytes, "add".into()).unwrap()
    }

    fn id_of(s: &Session, label: &str) -> u64 {
        s.document
            .sections
            .iter()
            .find(|x| x.label == label)
            .unwrap()
            .id
    }

    fn add(label: &str, bars: u32, casm_from: u64) -> Command {
        Command::AddSection {
            label: label.into(),
            bars,
            casm_from,
            copy_from: None,
            fit: None,
        }
    }

    fn sdec_names(s: &Session) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        for c in s.document.style.chunks.iter().filter(|c| c.tag == *b"CASM") {
            for g in smf::chunks(&c.data)
                .unwrap()
                .into_iter()
                .filter(|g| g.tag == *b"CSEG")
            {
                for r in smf::chunks(&g.data)
                    .unwrap()
                    .into_iter()
                    .filter(|r| r.tag == *b"Sdec")
                {
                    out.push(
                        String::from_utf8_lossy(&r.data)
                            .trim_end_matches('\0')
                            .split(',')
                            .map(|n| n.trim().to_string())
                            .collect(),
                    );
                }
            }
        }
        out
    }

    #[test]
    fn rank_follows_the_real_style_order_and_rejects_unknown_names() {
        assert!(rank("Main A") < rank("Main D"));
        assert!(rank("Main D") < rank("Fill In AA"));
        assert!(rank("Fill In DD") < rank("Intro B"));
        assert!(rank("Intro B") < rank("Intro C") && rank("Intro C") < rank("Intro A"));
        assert!(rank("Intro A") < rank("Ending A") && rank("Ending C") < rank("Fill In BA"));
        assert_eq!(rank("main  d"), rank("Main D"));
        assert_eq!(rank("Main E"), None);
        assert_eq!(rank("Fill In AB"), None);
    }

    #[test]
    fn the_fixture_is_sparse_in_exactly_the_expected_slots() {
        let s = open(&fixture());
        for missing in ["Fill In DD", "Ending B", "Ending C"] {
            assert!(s.document.sections.iter().all(|x| x.label != missing));
        }
    }

    #[test]
    fn adds_a_missing_section_in_real_order_with_marker_and_description() {
        let mut s = open(&fixture());
        let before = s.document.clone();
        let ending_a = id_of(&s, "Ending A");
        s.execute(0, add("Ending B", 4, ending_a)).unwrap();
        let added = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Ending B")
            .unwrap();
        assert_eq!(added.end - added.start, 4 * BAR);
        // real order: right after Ending A, right before Fill In BA
        let ending_a = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Ending A")
            .unwrap();
        let break_fill = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Fill In BA")
            .unwrap();
        assert_eq!(ending_a.end, added.start);
        assert_eq!(added.end, break_fill.start);
        // both events a Yamaha section starts with exist at the section start
        let at_start: Vec<_> = s
            .document
            .style
            .tracks
            .iter()
            .flatten()
            .filter(|e| e.tick == added.start)
            .collect();
        assert!(at_start.iter().any(|e| e.meta(6) == Some(&b"Ending B"[..])));
        assert!(
            at_start
                .iter()
                .any(|e| e.meta(1) == Some(&b"fn:Ending B\0"[..]))
        );
        // every previous section keeps its length; every note is kept
        for old in &before.sections {
            let now = s.document.sections.iter().find(|x| x.id == old.id).unwrap();
            assert_eq!(now.end - now.start, old.end - old.start, "{}", old.label);
        }
        assert_eq!(s.document.notes.len(), before.notes.len());
        // notes after the insertion point moved by exactly the inserted length
        let delta = 4 * BAR;
        for (a, b) in before.notes.iter().zip(&s.document.notes) {
            assert_eq!(a.id, b.id);
            let expected = if a.start >= break_fill.start - delta {
                a.start + delta
            } else {
                a.start
            };
            assert_eq!(b.start, expected);
        }
        // the new name shares Ending A's CASM segment
        assert!(sdec_names(&s).iter().any(|g| {
            g.contains(&"Ending A".to_string()) && g.contains(&"Ending B".to_string())
        }));
        // re-parsing the saved bytes yields the same sections
        let reopened = Document::parse(&s.document.style.bytes().unwrap()).unwrap();
        let shape = |d: &Document| {
            d.sections
                .iter()
                .map(|x| (x.label.clone(), x.start, x.end))
                .collect::<Vec<_>>()
        };
        assert_eq!(shape(&reopened), shape(&s.document));
    }

    #[test]
    fn adds_in_the_middle_and_undo_restores_the_exact_bytes() {
        let mut s = open(&fixture());
        let before = s.document.style.bytes().unwrap();
        let fill_cc = id_of(&s, "Fill In CC");
        s.execute(0, add("Fill In DD", 1, fill_cc)).unwrap();
        let dd = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Fill In DD")
            .unwrap();
        let intro_b = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Intro B")
            .unwrap();
        assert_eq!(dd.end - dd.start, BAR);
        assert_eq!(dd.end, intro_b.start);
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), before);
    }

    #[test]
    fn appends_a_section_that_belongs_at_the_end() {
        let mut s = open(&style_without(&["Fill In BA"]));
        let before = s.document.sections.last().unwrap().clone();
        assert_eq!(before.label, "Ending A");
        s.execute(0, add("Fill In BA", 1, before.id)).unwrap();
        let last = s.document.sections.last().unwrap();
        assert_eq!(last.label, "Fill In BA");
        assert_eq!(last.start, before.end);
        assert_eq!(last.end - last.start, BAR);
        let reopened = Document::parse(&s.document.style.bytes().unwrap()).unwrap();
        assert_eq!(reopened.sections.last().unwrap().label, "Fill In BA");
        assert_eq!(
            reopened.sections.last().unwrap().end - reopened.sections.last().unwrap().start,
            BAR
        );
    }

    #[test]
    fn can_start_from_a_copy_of_an_existing_section_in_one_undo_step() {
        let mut s = open(&fixture());
        let before = s.document.style.bytes().unwrap();
        let ending_a = id_of(&s, "Ending A");
        let src = s
            .document
            .sections
            .iter()
            .find(|x| x.id == ending_a)
            .unwrap()
            .clone();
        let bars = ((src.end - src.start) / BAR) as u32;
        let src_notes = s
            .document
            .notes
            .iter()
            .filter(|n| n.start >= src.start && n.start < src.end)
            .count();
        assert!(src_notes > 0);
        s.execute(
            0,
            Command::AddSection {
                label: "Ending B".into(),
                bars,
                casm_from: ending_a,
                copy_from: Some(ending_a),
                fit: None,
            },
        )
        .unwrap();
        let added = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Ending B")
            .unwrap();
        let copied = s
            .document
            .notes
            .iter()
            .filter(|n| n.start >= added.start && n.start < added.end)
            .count();
        assert_eq!(copied, src_notes);
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), before);
    }

    #[test]
    fn rejects_duplicates_unknown_names_bad_lengths_and_missing_sources() {
        let mut s = open(&fixture());
        let main_a = id_of(&s, "Main A");
        let mut run = |label: &str, bars: u32, casm_from: u64, copy_from: Option<u64>| {
            s.execute(
                0,
                Command::AddSection {
                    label: label.into(),
                    bars,
                    casm_from,
                    copy_from,
                    fit: None,
                },
            )
        };
        assert!(run("Main A", 4, main_a, None).is_err(), "duplicate");
        assert!(run("Main E", 4, main_a, None).is_err(), "unknown name");
        assert!(run("Ending B", 0, main_a, None).is_err(), "zero bars");
        assert!(run("Ending B", 33, main_a, None).is_err(), "too long");
        assert!(run("Fill In DD", 9, main_a, None).is_err(), "fill too long");
        assert!(
            run("Ending B", 4, u64::MAX, None).is_err(),
            "missing casm source"
        );
        assert!(
            run("Ending B", 4, main_a, Some(u64::MAX)).is_err(),
            "missing copy source"
        );
    }
}
