//! Style Part Library helpers (SFM-120). Read-only inspection of user STY files;
//! phrase import reuses exchange::PhraseNote + ReplacePhrase (notes + optional voice).
//! Destination CASM/routes are never modified by library import.
use crate::{
    Error, Result,
    document::Document,
    edit::ControllerPoint,
    exchange::{ChannelPhrase, PhraseNote},
    smf::MAX_BYTES,
    voice_catalog::{self, Address},
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySection {
    pub id: u64,
    pub label: String,
    pub bars: f64,
    pub note_count: usize,
    pub channels: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStyle {
    pub path: String,
    pub name: String,
    pub format: String,
    pub ppq: u16,
    pub sections: Vec<LibrarySection>,
}

fn reject(message: &str) -> Error {
    Error::Edit(message.into())
}

const MAX_FOLDER_STYLES: usize = 200;

/// Scan a folder (non-recursive) for `.STY` files and return inspect summaries.
pub fn scan_folder(dir: &Path) -> Result<Vec<LibraryStyle>> {
    let meta = std::fs::metadata(dir).map_err(|e| reject(&e.to_string()))?;
    if !meta.is_dir() {
        return Err(reject("library path is not a folder"));
    }
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| reject(&e.to_string()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("sty"))
        })
        .collect();
    paths.sort();
    if paths.len() > MAX_FOLDER_STYLES {
        paths.truncate(MAX_FOLDER_STYLES);
    }
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        match inspect_file(&p) {
            Ok(style) => out.push(style),
            Err(_) => continue, // skip unreadable/corrupt STY in a folder browse
        }
    }
    Ok(out)
}

pub fn inspect_file(path: &Path) -> Result<LibraryStyle> {
    let meta = std::fs::metadata(path).map_err(|e| reject(&e.to_string()))?;
    if meta.len() > MAX_BYTES as u64 {
        return Err(reject("Style exceeds the 64 MiB input limit"));
    }
    let bytes = std::fs::read(path).map_err(|e| reject(&e.to_string()))?;
    let doc = Document::parse(&bytes)?;
    let (_, num, den) = doc.timing_at(0);
    let bar = ((doc.style.ppq as u64 * 4) / den.max(1) as u64) * num.max(1) as u64;
    let bar = bar.max(1);
    let format = if doc
        .style
        .tracks
        .iter()
        .flatten()
        .any(|e| e.meta(6) == Some(b"SFF2"))
    {
        "SFF2"
    } else if doc
        .style
        .tracks
        .iter()
        .flatten()
        .any(|e| e.meta(6) == Some(b"SFF1"))
    {
        "SFF1"
    } else {
        "MIDI"
    };
    let sections = doc
        .sections
        .iter()
        .map(|s| {
            let mut channels = std::collections::HashSet::new();
            let mut note_count = 0usize;
            for n in &doc.notes {
                if n.start >= s.start && n.start < s.end {
                    note_count += 1;
                    channels.insert(n.channel);
                }
            }
            let mut ch: Vec<_> = channels.into_iter().collect();
            ch.sort_unstable();
            LibrarySection {
                id: s.id,
                label: s.label.clone(),
                bars: (s.end - s.start) as f64 / bar as f64,
                note_count,
                channels: ch,
            }
        })
        .collect();
    Ok(LibraryStyle {
        path: path.display().to_string(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Style".into()),
        format: format.into(),
        ppq: doc.style.ppq,
        sections,
    })
}

/// All source channels' relative notes for a library section (notes-only complete part).
pub fn extract_section_phrases(path: &Path, section_id: u64) -> Result<(u64, Vec<ChannelPhrase>)> {
    let meta = std::fs::metadata(path).map_err(|e| reject(&e.to_string()))?;
    if meta.len() > MAX_BYTES as u64 {
        return Err(reject("Style exceeds the 64 MiB input limit"));
    }
    let bytes = std::fs::read(path).map_err(|e| reject(&e.to_string()))?;
    let doc = Document::parse(&bytes)?;
    let s = doc
        .sections
        .iter()
        .find(|x| x.id == section_id)
        .ok_or_else(|| reject("library section not found"))?;
    let mut by_ch: std::collections::BTreeMap<u8, Vec<PhraseNote>> =
        std::collections::BTreeMap::new();
    for n in &doc.notes {
        if n.start < s.start || n.start >= s.end {
            continue;
        }
        if n.start + n.duration > s.end {
            return Err(reject("library notes cross the section boundary"));
        }
        by_ch.entry(n.channel).or_default().push(PhraseNote {
            start: n.start - s.start,
            pitch: n.pitch,
            duration: n.duration,
            velocity: n.velocity,
        });
    }
    if by_ch.is_empty() {
        return Err(reject("library source has no notes in that section"));
    }
    Ok((
        s.end - s.start,
        by_ch
            .into_iter()
            .map(|(channel, notes)| ChannelPhrase { channel, notes })
            .collect(),
    ))
}

/// Rhythm source channels used by Yamaha style parts Rhy1/Rhy2 (SFM drum insert).
pub const DRUM_CHANNELS: [u8; 2] = [9, 10];

/// Bank/program active at the start of a library section for one channel.
pub fn extract_voice(path: &Path, section_id: u64, channel: u8) -> Result<Option<Address>> {
    let meta = std::fs::metadata(path).map_err(|e| reject(&e.to_string()))?;
    if meta.len() > MAX_BYTES as u64 {
        return Err(reject("Style exceeds the 64 MiB input limit"));
    }
    let bytes = std::fs::read(path).map_err(|e| reject(&e.to_string()))?;
    let doc = Document::parse(&bytes)?;
    let s = doc
        .sections
        .iter()
        .find(|x| x.id == section_id)
        .ok_or_else(|| reject("library section not found"))?;
    Ok(voice_catalog::address_at(&doc, s.start, channel))
}

/// Bank/program for every channel that has notes (and a voice) in the section.
pub fn extract_section_voices(
    path: &Path,
    section_id: u64,
    channels: &[u8],
) -> Result<Vec<(u8, Address)>> {
    let meta = std::fs::metadata(path).map_err(|e| reject(&e.to_string()))?;
    if meta.len() > MAX_BYTES as u64 {
        return Err(reject("Style exceeds the 64 MiB input limit"));
    }
    let bytes = std::fs::read(path).map_err(|e| reject(&e.to_string()))?;
    let doc = Document::parse(&bytes)?;
    let s = doc
        .sections
        .iter()
        .find(|x| x.id == section_id)
        .ok_or_else(|| reject("library section not found"))?;
    let mut out = Vec::new();
    for &ch in channels {
        if let Some(address) = voice_catalog::address_at(&doc, s.start, ch) {
            out.push((ch, address));
        }
    }
    Ok(out)
}

/// Relative-tick CC lanes for one library channel (excludes bank MSB/LSB).
pub fn extract_controllers(
    path: &Path,
    section_id: u64,
    channel: u8,
) -> Result<Vec<(u8, Vec<ControllerPoint>)>> {
    let meta = std::fs::metadata(path).map_err(|e| reject(&e.to_string()))?;
    if meta.len() > MAX_BYTES as u64 {
        return Err(reject("Style exceeds the 64 MiB input limit"));
    }
    let bytes = std::fs::read(path).map_err(|e| reject(&e.to_string()))?;
    let doc = Document::parse(&bytes)?;
    let s = doc
        .sections
        .iter()
        .find(|x| x.id == section_id)
        .ok_or_else(|| reject("library section not found"))?;
    Ok(controllers_in_section(&doc, s.start, s.end, channel))
}

/// Relative-tick CC lanes for every channel that has notes or CC in the section.
pub fn extract_section_controllers(
    path: &Path,
    section_id: u64,
    channels: &[u8],
) -> Result<Vec<(u8, u8, Vec<ControllerPoint>)>> {
    let meta = std::fs::metadata(path).map_err(|e| reject(&e.to_string()))?;
    if meta.len() > MAX_BYTES as u64 {
        return Err(reject("Style exceeds the 64 MiB input limit"));
    }
    let bytes = std::fs::read(path).map_err(|e| reject(&e.to_string()))?;
    let doc = Document::parse(&bytes)?;
    let s = doc
        .sections
        .iter()
        .find(|x| x.id == section_id)
        .ok_or_else(|| reject("library section not found"))?;
    let mut out = Vec::new();
    for &ch in channels {
        for (cc, points) in controllers_in_section(&doc, s.start, s.end, ch) {
            out.push((ch, cc, points));
        }
    }
    Ok(out)
}

fn controllers_in_section(
    doc: &Document,
    start: u64,
    end: u64,
    channel: u8,
) -> Vec<(u8, Vec<ControllerPoint>)> {
    let mut by_cc: BTreeMap<u8, BTreeMap<u64, u8>> = BTreeMap::new();
    for e in doc.style.tracks.iter().flatten() {
        if e.tick < start || e.tick >= end || e.data.len() != 3 {
            continue;
        }
        if e.channel() != Some(channel) || e.data[0] & 0xf0 != 0xb0 {
            continue;
        }
        let cc = e.data[1];
        if cc > 119 || matches!(cc, 0 | 32) {
            continue;
        }
        // Last event at a tick wins (matches ControllerPoints effective semantics).
        by_cc
            .entry(cc)
            .or_default()
            .insert(e.tick - start, e.data[2]);
    }
    by_cc
        .into_iter()
        .map(|(cc, ticks)| {
            let points = ticks
                .into_iter()
                .map(|(tick, value)| ControllerPoint { tick, value })
                .collect();
            (cc, points)
        })
        .collect()
}

/// Map relative controller points into the destination section with Keep/Fit/Repeat.
pub fn map_controllers(
    lanes: &[(u8, Vec<ControllerPoint>)],
    src_len: u64,
    dest_len: u64,
    dest_start: u64,
    mode: LibraryFit,
) -> Result<Vec<(u8, Vec<ControllerPoint>)>> {
    let mut out = Vec::with_capacity(lanes.len());
    for (cc, points) in lanes {
        let mapped = map_controller_points(points, src_len, dest_len, mode)?;
        if mapped.is_empty() {
            continue;
        }
        out.push((
            *cc,
            mapped
                .into_iter()
                .map(|p| ControllerPoint {
                    tick: dest_start.saturating_add(p.tick),
                    value: p.value,
                })
                .collect(),
        ));
    }
    Ok(out)
}

/// Map multi-channel relative controller lanes `(channel, cc, points)`.
pub fn map_section_controllers(
    lanes: &[(u8, u8, Vec<ControllerPoint>)],
    src_len: u64,
    dest_len: u64,
    dest_start: u64,
    mode: LibraryFit,
) -> Result<Vec<(u8, u8, Vec<ControllerPoint>)>> {
    let mut out = Vec::with_capacity(lanes.len());
    for (ch, cc, points) in lanes {
        let mapped = map_controller_points(points, src_len, dest_len, mode)?;
        if mapped.is_empty() {
            continue;
        }
        out.push((
            *ch,
            *cc,
            mapped
                .into_iter()
                .map(|p| ControllerPoint {
                    tick: dest_start.saturating_add(p.tick),
                    value: p.value,
                })
                .collect(),
        ));
    }
    Ok(out)
}

fn map_controller_points(
    points: &[ControllerPoint],
    src_len: u64,
    dest_len: u64,
    mode: LibraryFit,
) -> Result<Vec<ControllerPoint>> {
    match mode {
        LibraryFit::Keep => {
            for p in points {
                if p.tick >= dest_len {
                    return Err(reject(
                        "library controllers are longer than the destination; choose Fit or Repeat",
                    ));
                }
            }
            Ok(points.to_vec())
        }
        LibraryFit::Fit => {
            let mut out = Vec::with_capacity(points.len());
            let mut seen = std::collections::HashSet::new();
            for p in points {
                let tick = scale_len(p.tick, src_len, dest_len);
                if tick < dest_len && seen.insert(tick) {
                    out.push(ControllerPoint {
                        tick,
                        value: p.value,
                    });
                }
            }
            Ok(out)
        }
        LibraryFit::Repeat => {
            let mut out = Vec::new();
            let mut seen = std::collections::HashSet::new();
            let mut offset = 0u64;
            while offset < dest_len {
                for p in points {
                    let tick = offset.saturating_add(p.tick);
                    if tick < dest_len && seen.insert(tick) {
                        out.push(ControllerPoint {
                            tick,
                            value: p.value,
                        });
                    }
                }
                offset = offset.saturating_add(src_len);
                if src_len == 0 {
                    break;
                }
            }
            Ok(out)
        }
    }
}

/// Relative phrase notes + source section length from a library Style.
pub fn extract_phrase(path: &Path, section_id: u64, channel: u8) -> Result<(u64, Vec<PhraseNote>)> {
    let meta = std::fs::metadata(path).map_err(|e| reject(&e.to_string()))?;
    if meta.len() > MAX_BYTES as u64 {
        return Err(reject("Style exceeds the 64 MiB input limit"));
    }
    let bytes = std::fs::read(path).map_err(|e| reject(&e.to_string()))?;
    let doc = Document::parse(&bytes)?;
    let s = doc
        .sections
        .iter()
        .find(|x| x.id == section_id)
        .ok_or_else(|| reject("library section not found"))?;
    let scoped: Vec<_> = doc
        .notes
        .iter()
        .filter(|n| n.channel == channel && n.start >= s.start && n.start < s.end)
        .collect();
    if scoped.is_empty() {
        return Err(reject("library source has no notes in that section"));
    }
    for n in &scoped {
        if n.start + n.duration > s.end {
            return Err(reject("library notes cross the section boundary"));
        }
    }
    let len = s.end - s.start;
    let notes = scoped
        .into_iter()
        .map(|n| PhraseNote {
            start: n.start - s.start,
            pitch: n.pitch,
            duration: n.duration,
            velocity: n.velocity,
        })
        .collect();
    Ok((len, notes))
}

/// Extract only Rhy1/Rhy2 (CH9/10) phrases for drum-pattern insertion (SFM-LIB-DRUM-*).
pub fn extract_drum_phrases(path: &Path, section_id: u64) -> Result<(u64, Vec<ChannelPhrase>)> {
    let (len, phrases) = extract_section_phrases(path, section_id)?;
    let drums: Vec<_> = phrases
        .into_iter()
        .filter(|p| DRUM_CHANNELS.contains(&p.channel))
        .collect();
    if drums.is_empty() {
        return Err(reject(
            "library section has no notes on rhythm channels 9/10",
        ));
    }
    Ok((len, drums))
}

/// Tempo (µs/qn), PPQ, section length, and phrases for library audition (notes only).
/// Parses the Style once (no second read for phrase extraction).
pub fn preview_material(
    path: &Path,
    section_id: u64,
    channel: Option<u8>,
) -> Result<(u64, u16, u64, Vec<ChannelPhrase>)> {
    let meta = std::fs::metadata(path).map_err(|e| reject(&e.to_string()))?;
    if meta.len() > MAX_BYTES as u64 {
        return Err(reject("Style exceeds the 64 MiB input limit"));
    }
    let bytes = std::fs::read(path).map_err(|e| reject(&e.to_string()))?;
    let doc = Document::parse(&bytes)?;
    let (bpm, _, _) = doc.timing_at(0);
    let tempo_us = if bpm > 0.0 {
        (60_000_000.0 / bpm).round().max(1.0) as u64
    } else {
        500_000
    };
    let s = doc
        .sections
        .iter()
        .find(|x| x.id == section_id)
        .ok_or_else(|| reject("library section not found"))?;
    let len = s.end - s.start;
    let mut by_ch: std::collections::BTreeMap<u8, Vec<PhraseNote>> =
        std::collections::BTreeMap::new();
    for n in &doc.notes {
        if n.start < s.start || n.start >= s.end {
            continue;
        }
        if channel.is_some_and(|ch| n.channel != ch) {
            continue;
        }
        if n.start + n.duration > s.end {
            return Err(reject("library notes cross the section boundary"));
        }
        by_ch.entry(n.channel).or_default().push(PhraseNote {
            start: n.start - s.start,
            pitch: n.pitch,
            duration: n.duration,
            velocity: n.velocity,
        });
    }
    if by_ch.is_empty() {
        return Err(reject("library source has no notes in that section"));
    }
    let phrases = by_ch
        .into_iter()
        .map(|(ch, notes)| ChannelPhrase { channel: ch, notes })
        .collect();
    Ok((tempo_us, doc.style.ppq, len, phrases))
}

fn scale_len(tick: u64, src_len: u64, dest_len: u64) -> u64 {
    if src_len == 0 {
        return 0;
    }
    let n = tick as u128 * dest_len as u128;
    let d = src_len as u128;
    let q = n / d;
    let r = n % d;
    (q + u128::from(r * 2 > d || (r * 2 == d && q % 2 == 1))) as u64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryFit {
    /// Reject if phrase extends past destination.
    Keep,
    /// Proportionally stretch/compress to destination length.
    Fit,
    /// Tile the phrase through the destination (like CopyMaterial Repeat).
    Repeat,
}

impl LibraryFit {
    pub fn parse(label: &str) -> Result<Self> {
        match label {
            "keep" | "Keep" => Ok(Self::Keep),
            "fit" | "Fit" => Ok(Self::Fit),
            "repeat" | "Repeat" => Ok(Self::Repeat),
            _ => Err(reject("fit must be keep, fit, or repeat")),
        }
    }
}

/// Map a relative library phrase into a destination section length.
pub fn map_to_destination(
    phrase: &[PhraseNote],
    src_len: u64,
    dest_len: u64,
    fit: LibraryFit,
) -> Result<Vec<PhraseNote>> {
    if src_len == 0 || dest_len == 0 {
        return Err(reject("section has no duration"));
    }
    match fit {
        LibraryFit::Keep => {
            for n in phrase {
                if n.start.saturating_add(n.duration) > dest_len {
                    return Err(reject(
                        "library phrase is longer than the destination; choose Fit or Repeat",
                    ));
                }
            }
            Ok(phrase.to_vec())
        }
        LibraryFit::Fit => {
            let mut out = Vec::with_capacity(phrase.len());
            for n in phrase {
                let start = scale_len(n.start, src_len, dest_len);
                let end = scale_len(n.start + n.duration, src_len, dest_len);
                let duration = end.saturating_sub(start).max(1);
                if start < dest_len && start + duration <= dest_len {
                    out.push(PhraseNote {
                        start,
                        pitch: n.pitch,
                        duration,
                        velocity: n.velocity,
                    });
                }
            }
            Ok(out)
        }
        LibraryFit::Repeat => {
            let mut out = Vec::new();
            let mut offset = 0u64;
            while offset < dest_len {
                for n in phrase {
                    let start = offset.saturating_add(n.start);
                    if start >= dest_len {
                        continue;
                    }
                    let duration = n.duration.min(dest_len.saturating_sub(start)).max(1);
                    if start + duration <= dest_len {
                        out.push(PhraseNote {
                            start,
                            pitch: n.pitch,
                            duration,
                            velocity: n.velocity,
                        });
                    }
                }
                offset = offset.saturating_add(src_len);
            }
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Command, Session};
    use std::{fs, path::Path};

    fn fixture() -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
        )
        .unwrap()
    }

    fn session() -> Session {
        Session::open(&fixture(), "library".into()).unwrap()
    }

    #[test]
    fn inspect_lists_sections_with_channels() {
        let dir = std::env::temp_dir().join("genos-library-inspect.STY");
        fs::write(&dir, fixture()).unwrap();
        let lib = inspect_file(&dir).unwrap();
        assert!(!lib.sections.is_empty());
        assert!(lib.sections.iter().any(|s| s.note_count > 0));
        assert!(lib.ppq > 0);
        let _ = fs::remove_file(&dir);
    }

    #[test]
    fn extract_and_fit_maps_into_shorter_destination() {
        let dir = std::env::temp_dir().join("genos-library-fit.STY");
        fs::write(&dir, fixture()).unwrap();
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
            .find(|x| x.label.starts_with("Fill") && (x.end - x.start) < (main.end - main.start))
            .unwrap()
            .clone();
        let ch = s
            .document
            .notes
            .iter()
            .find(|n| n.start >= main.start && n.start < main.end)
            .unwrap()
            .channel;
        let (src_len, phrase) = extract_phrase(&dir, main.id, ch).unwrap();
        assert!(!phrase.is_empty());
        let dest_len = fill.end - fill.start;
        let mapped = map_to_destination(&phrase, src_len, dest_len, LibraryFit::Fit).unwrap();
        assert!(!mapped.is_empty());
        assert!(mapped.iter().all(|n| n.start + n.duration <= dest_len));
        let keep = map_to_destination(&phrase, src_len, dest_len, LibraryFit::Keep);
        assert!(keep.is_err());
        let before = s.document.style.bytes().unwrap();
        let notes = mapped;
        s.execute(
            0,
            Command::ReplacePhrase {
                section: fill.id,
                channel: ch,
                notes,
                voice: None,
                controllers: vec![],
            },
        )
        .unwrap();
        assert!(s.view().can_undo);
        assert_ne!(s.document.style.bytes().unwrap(), before);
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), before);
        let _ = fs::remove_file(&dir);
    }

    #[test]
    fn keep_rejects_when_phrase_exceeds_destination() {
        let phrase = vec![PhraseNote {
            start: 0,
            pitch: 60,
            duration: 500,
            velocity: 100,
        }];
        let err = map_to_destination(&phrase, 1000, 400, LibraryFit::Keep);
        assert!(err.is_err());
    }

    #[test]
    fn repeat_tiles_phrase_through_destination() {
        let phrase = vec![PhraseNote {
            start: 0,
            pitch: 36,
            duration: 50,
            velocity: 100,
        }];
        let out = map_to_destination(&phrase, 100, 350, LibraryFit::Repeat).unwrap();
        assert_eq!(out.len(), 4); // offsets 0,100,200,300
        assert_eq!(out[3].start, 300);
        assert!(out.iter().all(|n| n.start + n.duration <= 350));
    }

    #[test]
    fn extract_section_phrases_covers_multiple_channels() {
        let dir = std::env::temp_dir().join("genos-library-section.STY");
        fs::write(&dir, fixture()).unwrap();
        let (len, phrases) = extract_section_phrases(&dir, {
            let s = session();
            s.document
                .sections
                .iter()
                .find(|x| x.label == "Main A")
                .unwrap()
                .id
        })
        .unwrap();
        assert!(len > 0);
        assert!(phrases.len() >= 2);
        assert!(phrases.iter().all(|p| !p.notes.is_empty()));
        let _ = fs::remove_file(&dir);
    }

    #[test]
    fn scan_folder_finds_sty_summaries() {
        let dir = std::env::temp_dir().join("genos-library-scan");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let sty = dir.join("sample.STY");
        fs::write(&sty, fixture()).unwrap();
        fs::write(dir.join("readme.txt"), b"skip").unwrap();
        let list = scan_folder(&dir).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "sample.STY");
        assert!(!list[0].sections.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_drum_phrases_filters_to_rhy() {
        let dir = std::env::temp_dir().join("genos-library-drums.STY");
        fs::write(&dir, fixture()).unwrap();
        let main = session()
            .document
            .sections
            .iter()
            .find(|x| x.label == "Main A")
            .unwrap()
            .id;
        match extract_drum_phrases(&dir, main) {
            Ok((_, phrases)) => {
                assert!(!phrases.is_empty());
                assert!(phrases.iter().all(|p| DRUM_CHANNELS.contains(&p.channel)));
            }
            Err(e) => {
                // Fixture may lack CH9/10 notes; still proves the filter path.
                assert!(e.to_string().contains("rhythm channels"));
            }
        }
        let _ = fs::remove_file(&dir);
    }

    #[test]
    fn extract_and_map_controllers_fit_preserves_cc_values() {
        let dir = std::env::temp_dir().join("genos-library-cc.STY");
        fs::write(&dir, fixture()).unwrap();
        let s = session();
        let main = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Main A")
            .unwrap();
        let ch = s
            .document
            .notes
            .iter()
            .find(|n| n.start >= main.start && n.start < main.end)
            .unwrap()
            .channel;
        let lanes = extract_controllers(&dir, main.id, ch).unwrap();
        // Fixture may lack CC; mapping empty lanes must succeed.
        let dest_len = (main.end - main.start) / 2;
        let mapped =
            map_controllers(&lanes, main.end - main.start, dest_len, 0, LibraryFit::Fit).unwrap();
        assert_eq!(mapped.len(), lanes.len());
        for ((_, src), (_, dst)) in lanes.iter().zip(mapped.iter()) {
            assert!(dst.iter().all(|p| p.tick < dest_len));
            assert!(src.len() >= dst.len());
        }
        let _ = fs::remove_file(&dir);
    }

    #[test]
    fn extract_voice_reads_bank_program_at_section_start() {
        let dir = std::env::temp_dir().join("genos-library-voice.STY");
        fs::write(&dir, fixture()).unwrap();
        let s = session();
        let main = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Main A")
            .unwrap();
        let ch = s
            .document
            .notes
            .iter()
            .find(|n| n.start >= main.start && n.start < main.end)
            .unwrap()
            .channel;
        let voice = extract_voice(&dir, main.id, ch).unwrap();
        assert!(
            voice.is_some(),
            "fixture channel {ch} should expose MSB/LSB/PC at Main A"
        );
        let voices = extract_section_voices(&dir, main.id, &[ch]).unwrap();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].0, ch);
        assert_eq!(voices[0].1, voice.unwrap());
        let _ = fs::remove_file(&dir);
    }

    #[test]
    fn preview_material_builds_phrases() {
        let dir = std::env::temp_dir().join("genos-library-preview.STY");
        fs::write(&dir, fixture()).unwrap();
        let s = session();
        let main = s
            .document
            .sections
            .iter()
            .find(|x| x.label == "Main A")
            .unwrap();
        let ch = s
            .document
            .notes
            .iter()
            .find(|n| n.start >= main.start && n.start < main.end)
            .unwrap()
            .channel;
        let (tempo, ppq, len, phrases) = preview_material(&dir, main.id, Some(ch)).unwrap();
        assert!(tempo > 0 && ppq > 0 && len > 0);
        assert_eq!(phrases.len(), 1);
        assert!(!phrases[0].notes.is_empty());
        let plan =
            crate::midi::plan_from_phrases(&[(ch, phrases[0].notes.as_slice())], ppq, tempo, len)
                .unwrap();
        assert!(!plan.packets.is_empty());
        assert!(!plan.duration.is_zero());
        let _ = fs::remove_file(&dir);
    }
}
