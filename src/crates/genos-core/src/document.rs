use crate::{
    Error, Result,
    add_section::ORDER,
    casm::supported_ctb2_size,
    smf::{self, Style},
};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: u64,
    pub off_id: u64,
    pub track: usize,
    pub channel: u8,
    pub pitch: u8,
    pub velocity: u8,
    pub start: u64,
    pub duration: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: u64,
    pub label: String,
    pub start: u64,
    pub end: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub id: usize,
    pub sections: Vec<String>,
    pub source: u8,
    pub destination: u8,
    pub bytes: Vec<u8>,
    pub source_root: u8,
    pub source_chord: u8,
    pub middle_ntr: u8,
    pub middle_ntt: u8,
    pub middle_ntt_bass: bool,
    #[serde(skip)]
    pub chunk: usize,
    #[serde(skip)]
    pub offset: usize,
}

impl Route {
    fn from_ctb2(
        id: usize,
        sections: &[String],
        bytes: Vec<u8>,
        chunk: usize,
        offset: usize,
    ) -> Self {
        Self {
            id,
            sections: sections.to_vec(),
            source: bytes[0] + 1,
            destination: bytes[9] + 1,
            source_root: bytes[18],
            source_chord: bytes[19],
            middle_ntr: bytes[28],
            middle_ntt: bytes[29] & 127,
            middle_ntt_bass: bytes[29] & 128 != 0,
            bytes,
            chunk,
            offset,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ChunkInfo {
    pub tag: String,
    pub bytes: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub name: String,
    pub revision: u64,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub ppq: u16,
    pub format: String,
    pub tempo: f64,
    pub numerator: u8,
    pub denominator: u16,
    pub sections: Vec<Section>,
    pub notes: Vec<Note>,
    pub events: Vec<smf::Event>,
    pub routes: Vec<Route>,
    pub chunks: Vec<ChunkInfo>,
    pub warnings: Vec<String>,
    /// Editor workspace restored from `.gss` `uiState` (SFM-002).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_state: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantize_original: Option<serde_json::Value>,
}
#[derive(Clone)]
pub struct Document {
    pub style: Style,
    pub sections: Vec<Section>,
    pub notes: Vec<Note>,
    pub routes: Vec<Route>,
    pub warnings: Vec<String>,
    firmware_cseg: HashMap<&'static str, Option<(usize, std::ops::Range<usize>)>>,
    firmware_indirect_sections: HashSet<&'static str>,
    legacy_playback: Option<HashMap<&'static str, Vec<Route>>>,
    firmware_markers: Option<HashMap<String, &'static str>>,
}

// Firmware uses a 65-byte staging slot including the eight-byte chunk header.
const CTB2_STAGED_PAYLOAD: usize = 57;

fn legacy_cseg_routes(records: &[smf::Chunk], labels: &[String]) -> Option<Vec<Route>> {
    // acc_SLLoadChgStyle stages one converted record per source, in file order.
    const LEGACY_NTT: [u8; 6] = [0, 1, 2, 0x81, 3, 5];
    let mut slots: [Option<Vec<u8>>; 16] = Default::default();
    for record in records {
        let data = &record.data;
        match &record.tag {
            b"Sdec" => {}
            b"Ctab" if data.len() >= 26 && data[0] < 16 && data[9] < 16 => {
                let mut bytes = data[..20].to_vec();
                bytes.extend([0, 127]);
                let mut region: [u8; 6] = data[20..26].try_into().unwrap();
                region[1] = LEGACY_NTT.get(usize::from(region[1])).copied().unwrap_or(0);
                for _ in 0..3 {
                    bytes.extend(region);
                }
                bytes.extend([0; 6]);
                bytes.extend_from_slice(&data[26..data.len().min(CTB2_STAGED_PAYLOAD - 20)]);
                slots[usize::from(data[0])] = Some(bytes);
            }
            b"Cntt" if data.len() == 2 && data[0] < 16 => {
                if let Some(bytes) = &mut slots[usize::from(data[0])] {
                    for offset in [23, 29, 35] {
                        bytes[offset] = data[1];
                    }
                }
            }
            b"Ctb2" if supported_ctb2_size(data.len()) => {}
            _ => return None,
        }
    }
    Some(
        slots
            .into_iter()
            .flatten()
            .map(|bytes| Route::from_ctb2(0, labels, bytes, 0, 0))
            .collect(),
    )
}
pub fn musical_marker(s: &str) -> bool {
    let n = s.trim_matches(char::from(0)).trim().to_ascii_uppercase();
    if n == "BREAK" || n == "FILL" || n == "MAIN" || n == "INTRO" || n == "ENDING" {
        return true;
    }
    // Accept spaced and compact Yamaha markers (Main A, Fill In AA, IntroA, Ending I).
    ["MAIN", "FILL IN", "FILL", "INTRO", "ENDING", "END"]
        .iter()
        .any(|prefix| {
            n.starts_with(&format!("{prefix} "))
                || (n.starts_with(prefix)
                    && n.len() > prefix.len()
                    && n.as_bytes()
                        .get(prefix.len())
                        .is_some_and(|c| c.is_ascii_alphanumeric()))
        })
}
// Sdec finds names anywhere in its payload. A trailing label takes effect
// only after another data record.
fn firmware_sdec_descriptions(records: &[smf::Chunk]) -> impl Iterator<Item = &[u8]> {
    let through_data = records
        .iter()
        .rposition(|record| record.tag != *b"Sdec")
        .map_or(&records[..0], |last| &records[..=last]);
    through_data
        .iter()
        .filter(|record| record.tag == *b"Sdec")
        .map(|record| record.data.as_slice())
}

pub(crate) fn firmware_sdec_sections(
    records: &[smf::Chunk],
) -> impl Iterator<Item = &'static str> + '_ {
    ORDER
        .into_iter()
        .filter(|&name| firmware_sdec_descriptions(records).any(|data| sdec_contains(data, name)))
}

fn sdec_contains(data: &[u8], name: &str) -> bool {
    data.windows(name.len())
        .any(|bytes| bytes == name.as_bytes())
}

#[derive(Default)]
struct CsegClaims {
    claimed: HashSet<&'static str>,
    pending: Vec<&'static str>,
}

impl CsegClaims {
    fn scan(&mut self, records: &[smf::Chunk], marked: &HashSet<&'static str>) -> bool {
        let mut final_description_claimed = false;
        for description in firmware_sdec_descriptions(records) {
            let before = self.pending.len();
            for name in ORDER {
                if marked.contains(name)
                    && sdec_contains(description, name)
                    && self.claimed.insert(name)
                {
                    self.pending.push(name);
                }
            }
            final_description_claimed = self.pending.len() > before;
        }
        final_description_claimed
    }
}

impl Document {
    pub(crate) fn firmware_section_name(&self, section: &str) -> Option<&'static str> {
        self.firmware_markers.as_ref()?.get(section).copied()
    }

    pub(crate) fn firmware_section_has_indirect_binding(&self, section: &str) -> bool {
        self.firmware_section_name(section)
            .is_some_and(|name| self.firmware_indirect_sections.contains(name))
    }

    pub(crate) fn firmware_cseg_group(
        &self,
        section: &str,
    ) -> Option<&(usize, std::ops::Range<usize>)> {
        self.firmware_cseg
            .get(self.firmware_section_name(section)?)?
            .as_ref()
    }

    /// Ordinary SFF2 loads one CSEG per section and keeps its last record per source.
    pub(crate) fn firmware_cseg_route(&self, section: &str, source: u8) -> Option<Option<&Route>> {
        let markers = self.firmware_markers.as_ref()?;
        let Some(&canonical) = markers.get(section) else {
            return Some(None);
        };
        if let Some(legacy) = &self.legacy_playback {
            return Some(
                legacy
                    .get(canonical)
                    .and_then(|routes| routes.iter().find(|route| route.source == source)),
            );
        }
        let Some(binding) = self.firmware_cseg.get(canonical) else {
            return Some(None);
        };
        let (chunk, range) = binding.as_ref()?;
        Some(self.routes.iter().rev().find(|route| {
            route.source == source && route.chunk == *chunk && range.contains(&route.offset)
        }))
    }

    /// Active SMF timing metadata at a tick, including changes in other tracks.
    pub fn timing_at(&self, tick: u64) -> (f64, u8, u16) {
        let mut events: Vec<_> = self
            .style
            .tracks
            .iter()
            .flatten()
            .filter(|e| e.tick <= tick && (e.meta(0x51).is_some() || e.meta(0x58).is_some()))
            .collect();
        events.sort_by_key(|e| (e.tick, e.order));
        let mut tempo = 500_000u32;
        let mut meter = (4, 4);
        for event in events {
            if let Some(b) = event.meta(0x51).filter(|b| b.len() == 3) {
                let value = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
                if value > 0 {
                    tempo = value;
                }
            }
            if let Some(b) = event
                .meta(0x58)
                .filter(|b| b.len() == 4 && b[0] > 0 && b[1] < 8)
            {
                meter = (b[0], 1u16 << b[1]);
            }
        }
        (60_000_000.0 / tempo as f64, meter.0, meter.1)
    }
    pub fn parse(b: &[u8]) -> Result<Self> {
        Self::from_style(Style::parse(b)?)
    }
    pub fn from_style(style: Style) -> Result<Self> {
        let mut warnings = Vec::new();
        let mut sections = Vec::new();
        let mut firmware_markers = HashMap::new();
        let mut notes = Vec::new();
        let last = style
            .tracks
            .iter()
            .flatten()
            .map(|e| e.tick)
            .max()
            .unwrap_or(0);
        for e in style.tracks.iter().flatten() {
            if let Some(b) = e.meta(6) {
                let label = String::from_utf8_lossy(b)
                    .trim_matches(char::from(0))
                    .trim()
                    .to_string();
                if musical_marker(&label) {
                    if let Some(name) = ORDER
                        .into_iter()
                        .find(|name| b.starts_with(name.as_bytes()))
                    {
                        firmware_markers.insert(label.clone(), name);
                    }
                    sections.push(Section {
                        id: e.id,
                        label,
                        start: e.tick,
                        end: last,
                    });
                }
            }
        }
        sections.sort_by_key(|s| (s.start, s.id));
        if sections.windows(2).any(|w| w[0].start == w[1].start) {
            return Err(Error::Unsupported(
                "multiple section markers at the same tick".into(),
            ));
        }
        for i in 0..sections.len().saturating_sub(1) {
            sections[i].end = sections[i + 1].start;
        }
        if sections.is_empty() {
            sections.push(Section {
                id: 0,
                label: "Pattern".into(),
                start: 0,
                end: last.max(1),
            });
            warnings
                .push("No recognized Yamaha section markers; displaying the MIDI pattern.".into());
        }
        for (track, es) in style.tracks.iter().enumerate() {
            let mut pending: HashMap<(u8, u8), VecDeque<&smf::Event>> = HashMap::new();
            let mut orphan = 0;
            let mut unmatched = 0;
            for e in es {
                if e.is_on() {
                    pending
                        .entry((e.channel().unwrap(), e.data[1]))
                        .or_default()
                        .push_back(e);
                } else if e.is_off() {
                    let ch = e.channel().unwrap();
                    let p = e.data[1];
                    if let Some(on) = pending.get_mut(&(ch, p)).and_then(|q| q.pop_front()) {
                        notes.push(Note {
                            id: on.id,
                            off_id: e.id,
                            track,
                            channel: ch,
                            pitch: p,
                            velocity: on.data[2],
                            start: on.tick,
                            duration: e.tick.saturating_sub(on.tick).max(1),
                        });
                    } else {
                        orphan += 1;
                    }
                }
            }
            for q in pending.values() {
                unmatched += q.len();
            }
            if orphan > 0 || unmatched > 0 {
                warnings.push(format!("Track {}: {orphan} unmatched note-offs, {unmatched} unclosed note-ons retained unchanged.",track+1));
            }
        }
        notes.sort_by_key(|n| (n.start, n.track, n.id));
        let marked_sections: HashSet<_> = firmware_markers.values().copied().collect();
        let mut routes = Vec::new();
        let mut firmware_cseg = HashMap::new();
        let mut firmware_indirect_sections = HashSet::new();
        let mut claims = CsegClaims::default();
        let format = style
            .tracks
            .iter()
            .flatten()
            .filter_map(|e| e.meta(6))
            .find(|marker| matches!(*marker, b"SFF1" | b"SFF2"));
        let ordinary = !style
            .chunks
            .iter()
            .any(|c| matches!(&c.tag, b"Ghst" | b"Rsec" | b"Rtrk"));
        let ordinary_sff2 = ordinary && format == Some(b"SFF2");
        let mut legacy_playback = (ordinary && format == Some(b"SFF1")).then(HashMap::new);
        for (chunk, c) in style
            .chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| c.tag == *b"CASM")
        {
            let groups = smf::chunks(&c.data)?;
            let mut group_offset = 0;
            for g in groups {
                if g.tag == *b"CSEG" {
                    let records = smf::chunks(&g.data)?;
                    let mut labels = records
                        .iter()
                        .filter(|r| r.tag == *b"Sdec")
                        .flat_map(|r| {
                            String::from_utf8_lossy(&r.data)
                                .trim_matches(char::from(0))
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    // The SFF2 loader consumes these legacy records without
                    // staging them or letting them claim a section.
                    let ignored_legacy = |r: &smf::Chunk| {
                        matches!((&r.tag, r.data.len()), (b"Ctab", 26) | (b"Cntt", 2))
                    };
                    if let Some(playback) = &mut legacy_playback {
                        let converted = records
                            .split_first()
                            .filter(|(first, _)| first.tag == *b"Sdec")
                            .and_then(|(_, records)| legacy_cseg_routes(records, &labels));
                        match converted {
                            Some(routes) if !routes.is_empty() => {
                                let earlier_claims = claims.pending.len();
                                if claims.scan(&records, &marked_sections) {
                                    if earlier_claims > 0 {
                                        firmware_indirect_sections
                                            .extend(claims.pending.iter().copied());
                                    }
                                    let range = group_offset + 8..group_offset + 8 + g.data.len();
                                    for name in claims.pending.drain(..) {
                                        playback.insert(name, routes.clone());
                                        firmware_cseg.insert(name, Some((chunk, range.clone())));
                                    }
                                }
                            }
                            Some(_) => {}
                            None => {
                                firmware_indirect_sections.extend(claims.pending.drain(..));
                                warnings.push(
                                    "Unsupported SFF1 Ctab/Cntt group retained opaque.".into(),
                                );
                            }
                        }
                    }
                    if ordinary_sff2
                        && records
                            .iter()
                            .any(|r| r.tag != *b"Sdec" && !ignored_legacy(r))
                    {
                        let supported = records.first().is_some_and(|r| r.tag == *b"Sdec")
                            && records[1..].iter().all(|r| {
                                r.tag == *b"Sdec"
                                    || ignored_legacy(r)
                                    || (r.tag == *b"Ctb2"
                                        && supported_ctb2_size(r.data.len())
                                        && r.data[0] < 16
                                        && r.data[9] < 16)
                            });
                        let earlier_claims = claims.pending.len();
                        let final_description_claimed = claims.scan(&records, &marked_sections);
                        if supported {
                            // Claims point at the current output cursor. Only a new
                            // claim in the last effective Sdec commits this group's data.
                            if final_description_claimed {
                                let range = group_offset + 8..group_offset + 8 + g.data.len();
                                if earlier_claims > 0 {
                                    firmware_indirect_sections
                                        .extend(claims.pending.iter().copied());
                                }
                                for section in &sections {
                                    if firmware_markers
                                        .get(&section.label)
                                        .is_some_and(|name| claims.pending.contains(name))
                                        && !labels.contains(&section.label)
                                    {
                                        labels.push(section.label.clone());
                                    }
                                }
                                for name in claims.pending.drain(..) {
                                    firmware_cseg.insert(name, Some((chunk, range.clone())));
                                }
                            }
                        } else {
                            // The unsupported group makes the output cursor unknown.
                            // Keep those claims on the existing opaque-route fallback.
                            firmware_indirect_sections
                                .extend(claims.pending[..earlier_claims].iter().copied());
                            firmware_cseg.extend(claims.pending.drain(..).map(|name| (name, None)));
                        }
                    }
                    let mut offset = group_offset + 8;
                    for mut r in records {
                        let size = r.data.len();
                        if r.tag == *b"Ctb2" {
                            if supported_ctb2_size(size)
                                && (ordinary_sff2 || size <= CTB2_STAGED_PAYLOAD)
                                && r.data[0] < 16
                                && r.data[9] < 16
                            {
                                // Cap the route view as the firmware does. The Style
                                // chunk retains the complete on-disk payload.
                                r.data.truncate(CTB2_STAGED_PAYLOAD);
                                routes.push(Route::from_ctb2(
                                    routes.len(),
                                    &labels,
                                    r.data,
                                    chunk,
                                    offset + 8,
                                ));
                            } else {
                                warnings.push(format!(
                                    "Unsupported Ctb2 record ({} bytes) retained opaque.",
                                    r.data.len()
                                ));
                            }
                        } else if r.tag == *b"Ctab"
                            && legacy_playback.is_none()
                            && !(ordinary_sff2 && ignored_legacy(&r))
                        {
                            warnings.push("SFF1 Ctab retained opaque; conversion and rule editing unavailable.".into());
                        }
                        offset += 8 + size;
                    }
                }
                group_offset += 8 + g.data.len();
            }
        }
        // These section pointers reached the end of the written route data.
        firmware_indirect_sections.extend(claims.pending);
        let firmware_markers = ((ordinary_sff2 || legacy_playback.is_some())
            && style.chunks.iter().any(|chunk| chunk.tag == *b"CASM"))
        .then_some(firmware_markers);
        Ok(Self {
            style,
            sections,
            notes,
            routes,
            warnings,
            firmware_cseg,
            firmware_indirect_sections,
            legacy_playback,
            firmware_markers,
        })
    }
    /// Whether `source` has a CASM route in `section`, resolved like playback: the firmware
    /// section group first, then the document's routes.
    pub fn source_routed(&self, section: &str, source: u8) -> bool {
        match self.firmware_cseg_route(section, source) {
            Some(resolved) => resolved.is_some(),
            None => self
                .routes
                .iter()
                .any(|r| r.source == source && r.sections.iter().any(|s| s == section)),
        }
    }

    /// Sections whose CASM leaves tracks with notes unrouted: (section, channel, notes).
    /// The Genos does not play such a track in that section.
    pub fn unrouted_tracks(&self) -> Vec<(String, u8, usize)> {
        let mut found = Vec::new();
        for section in &self.sections {
            let routed: Vec<bool> = (1..=16u8)
                .map(|ch| self.source_routed(&section.label, ch))
                .collect();
            let has_casm = routed.iter().any(|r| *r)
                || self
                    .routes
                    .iter()
                    .any(|r| r.sections.contains(&section.label));
            if !has_casm {
                continue;
            }
            for channel in 1..=16u8 {
                let notes = self
                    .notes
                    .iter()
                    .filter(|n| {
                        n.channel == channel && n.start >= section.start && n.start < section.end
                    })
                    .count();
                if notes > 0 && !routed[usize::from(channel - 1)] {
                    found.push((section.label.clone(), channel, notes));
                }
            }
        }
        found
    }

    pub fn view(
        &self,
        name: String,
        revision: u64,
        dirty: bool,
        can_undo: bool,
        can_redo: bool,
    ) -> View {
        let events: Vec<_> = self.style.tracks.iter().flatten().cloned().collect();
        let (tempo, numerator, denominator) = self.timing_at(0);
        let format = events
            .iter()
            .filter_map(|e| e.meta(6))
            .find_map(|s| {
                if s == b"SFF2" {
                    Some("SFF2")
                } else if s == b"SFF1" {
                    Some("SFF1")
                } else {
                    None
                }
            })
            .unwrap_or("MIDI / Style");
        View {
            name,
            revision,
            dirty,
            can_undo,
            can_redo,
            ppq: self.style.ppq,
            format: format.into(),
            tempo,
            numerator,
            denominator,
            sections: self.sections.clone(),
            notes: self.notes.clone(),
            events,
            routes: self.routes.clone(),
            ui_state: None,
            quantize_original: None,
            chunks: self
                .style
                .chunks
                .iter()
                .map(|c| ChunkInfo {
                    tag: String::from_utf8_lossy(&c.tag).into(),
                    bytes: c.data.len(),
                })
                .collect(),
            warnings: self
                .warnings
                .iter()
                .cloned()
                .chain(self.unrouted_tracks().into_iter().map(|(section, channel, notes)| {
                    format!(
                        "{section}: channel {channel} has {notes} notes but no CASM route in this section; the Genos will not play them."
                    )
                }))
                .collect(),
        }
    }
}

#[cfg(test)]
mod marker_tests {
    use super::musical_marker;

    #[test]
    fn musical_marker_accepts_yamaha_aliases() {
        for label in [
            "Main A",
            "MAIN B",
            "Fill In AA",
            "Fill BA",
            "Intro A",
            "IntroA",
            "INTRO 1",
            "Ending A",
            "Ending I",
            "END A",
            "Break",
            "BREAK",
        ] {
            assert!(musical_marker(label), "{label}");
        }
        assert!(!musical_marker("SFF2"));
        assert!(!musical_marker("Chorus"));
    }
}

#[cfg(test)]
mod firmware_tests {
    use super::{Document, smf};
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, fs, path::Path};

    #[derive(Deserialize)]
    struct Replay {
        fixture_sha256: String,
        whole_file_style: BTreeMap<String, SectionRecords>,
        legacy_sff1: BTreeMap<String, LegacyCase>,
        legacy_sff1_groups: BTreeMap<String, LegacyGroupCase>,
        native_sizes: BTreeMap<usize, NativeSizeCase>,
    }

    #[derive(Deserialize)]
    struct SectionRecords {
        payloads: BTreeMap<u8, Vec<u8>>,
    }

    #[derive(Deserialize)]
    struct LegacyCase {
        records: Vec<u8>,
        payload: Option<Vec<u8>>,
    }

    #[derive(Deserialize)]
    struct LegacyGroupCase {
        groups: Vec<u8>,
        sections: BTreeMap<String, BTreeMap<u8, Vec<u8>>>,
    }

    #[derive(Deserialize)]
    struct NativeSizeCase {
        input: Vec<u8>,
        payload: Vec<u8>,
    }

    #[derive(Deserialize)]
    struct LabelCase {
        mode: u8,
        midi: String,
        records: Vec<u8>,
        bound: BTreeMap<usize, u8>,
    }

    #[derive(Deserialize)]
    struct CorpusCase {
        path: String,
        sha256: String,
        scope: String,
        sections: BTreeMap<String, BTreeMap<u8, Vec<u8>>>,
        error: Option<String>,
        casm_sha256: Option<String>,
    }

    #[test]
    #[ignore = "requires the local Genos Sdec finalization replay"]
    fn sdec_finalization_matches_firmware_replay() {
        #[derive(Deserialize)]
        struct Case {
            mode: u8,
            casm_chunks: Vec<Vec<u8>>,
            sections: BTreeMap<String, BTreeMap<u8, Vec<u8>>>,
        }
        let evidence = std::env::var_os("GENOS_SDEC_FINALIZATION_REPLAY").unwrap();
        let cases: BTreeMap<String, Case> =
            serde_json::from_slice(&fs::read(evidence).unwrap()).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY");
        let base = smf::Style::parse(&fs::read(fixture).unwrap()).unwrap();
        let casm = base
            .chunks
            .iter()
            .position(|chunk| chunk.tag == *b"CASM")
            .unwrap();
        let chord = crate::midi::LiveChord {
            root: 0,
            quality: "Major".into(),
            bass: None,
            bass_quality: None,
        };
        assert_eq!(cases.len(), 34);
        for (name, case) in cases {
            let mut style = base.clone();
            let format = style.tracks[0]
                .iter_mut()
                .find(|event| event.meta(6) == Some(b"SFF2"))
                .unwrap();
            *format.data.last_mut().unwrap() = b'0' + case.mode;
            style.chunks[style.track_chunks[0]].data = smf::encode_track(&style.tracks[0]).unwrap();
            style.chunks.splice(
                casm..=casm,
                case.casm_chunks.into_iter().map(|data| smf::Chunk {
                    tag: *b"CASM",
                    data,
                }),
            );
            let doc = Document::parse(&style.bytes().unwrap()).unwrap();
            for (section, expected) in case.sections {
                let id = doc
                    .sections
                    .iter()
                    .find(|item| item.label == section)
                    .unwrap()
                    .id;
                for source in 1..=16 {
                    let actual = doc.firmware_cseg_route(&section, source).unwrap();
                    assert_eq!(
                        actual.map(|route| &route.bytes),
                        expected.get(&source),
                        "{name} / {section} / source {source}"
                    );
                    if let Some(payload) = expected.get(&source) {
                        assert_eq!(
                            crate::live_preview::follow_pitch(&doc, id, source, 60, &chord),
                            Some(60 - payload[18]),
                            "{name} / {section} / source {source}",
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "requires the local Genos corpus replay and retained styles"]
    fn retained_style_corpus_matches_firmware_replay() {
        let evidence = std::env::var_os("GENOS_STYLE_CORPUS_REPLAY").unwrap();
        let cases: Vec<CorpusCase> = serde_json::from_slice(&fs::read(evidence).unwrap()).unwrap();
        let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs");
        assert_eq!(cases.len(), 8);
        let mut compared = BTreeMap::new();
        for case in cases {
            let bytes = fs::read(docs.join(&case.path)).unwrap();
            assert_eq!(format!("{:x}", Sha256::digest(&bytes)), case.sha256);
            let doc = Document::parse(&bytes).unwrap();
            match case.scope.as_str() {
                "whole" => assert!(case.error.is_none()),
                "casm-only" => {
                    assert!(case.error.is_some());
                    let casm = doc
                        .style
                        .chunks
                        .iter()
                        .find(|chunk| chunk.tag == *b"CASM")
                        .unwrap();
                    assert_eq!(
                        format!("{:x}", Sha256::digest(&casm.data)),
                        case.casm_sha256.unwrap()
                    );
                }
                scope => panic!("unknown replay scope {scope}"),
            }
            assert_eq!(case.sections.len(), 17);
            let slots = compared.entry(case.scope).or_insert(0);
            for (section, expected) in case.sections {
                for source in 1..=16 {
                    let actual = doc
                        .firmware_cseg_route(&section, source)
                        .unwrap_or_else(|| {
                            panic!("{}: no resolved playback data for {section}", case.path)
                        });
                    assert_eq!(
                        actual.map(|route| &route.bytes),
                        expected.get(&source),
                        "{} / {section} / source {source}",
                        case.path,
                    );
                    *slots += 1;
                }
            }
        }
        assert_eq!(compared.get("whole"), Some(&(8 * 17 * 16)));
        assert_eq!(
            compared.get("casm-only"),
            None,
            "whole-file replay regressed"
        );
    }

    #[test]
    #[ignore = "requires the local Genos section-label replay and retained style"]
    fn section_labels_match_firmware_replay() {
        let evidence = std::env::var_os("GENOS_SECTION_LABEL_REPLAY").unwrap();
        let cases: Vec<LabelCase> = serde_json::from_slice(&fs::read(evidence).unwrap()).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY");
        let base = smf::Style::parse(&fs::read(fixture).unwrap()).unwrap();
        let casm = base
            .chunks
            .iter()
            .position(|chunk| chunk.tag == *b"CASM")
            .unwrap();
        let track = base.track_chunks[0];
        assert_eq!(cases.len(), 40);
        for case in cases {
            let mut style = base.clone();
            let format = style.tracks[0]
                .iter_mut()
                .find(|event| event.meta(6) == Some(b"SFF2"))
                .unwrap();
            *format.data.last_mut().unwrap() = b'0' + case.mode;
            let marker = style.tracks[0]
                .iter_mut()
                .find(|event| event.meta(6) == Some(b"Main A"))
                .unwrap();
            marker.data.truncate(2);
            marker.data.push(case.midi.len().try_into().unwrap());
            marker.data.extend_from_slice(case.midi.as_bytes());
            style.chunks[track].data = smf::encode_track(&style.tracks[0]).unwrap();
            style.chunks[casm].data = smf::pack(&[smf::Chunk {
                tag: *b"CSEG",
                data: case.records,
            }])
            .unwrap();
            let doc = Document::parse(&style.bytes().unwrap()).unwrap();
            for (id, label) in [(0, case.midi.trim_matches('\0').trim()), (1, "Main B")] {
                let actual = doc
                    .firmware_cseg_route(label, 3)
                    .flatten()
                    .map(|route| route.destination);
                let expected = case.bound.get(&id).copied();
                assert_eq!(
                    actual, expected,
                    "SFF{}, {:?}, slot {id}",
                    case.mode, case.midi
                );
            }
        }
    }

    #[test]
    #[ignore = "requires the local Genos firmware replay and retained style"]
    fn whole_style_casm_matches_firmware_replay() {
        let evidence = std::env::var_os("GENOS_CASM_WHOLE_FILE_REPLAY").unwrap();
        let replay: Replay = serde_json::from_slice(&fs::read(evidence).unwrap()).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY");
        let bytes = fs::read(fixture).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            replay.fixture_sha256
        );
        let doc = Document::parse(&bytes).unwrap();
        assert_eq!(replay.whole_file_style.len(), 12);
        for (section, expected) in replay.whole_file_style {
            for source in 1..=16 {
                let route = doc
                    .firmware_cseg_route(&section, source)
                    .unwrap_or_else(|| panic!("no firmware CSEG resolution for {section}"));
                assert_eq!(
                    route.map(|route| &route.bytes),
                    expected.payloads.get(&source),
                    "{section} source {source}"
                );
            }
        }

        let mut style = doc.style;
        let casm = style
            .chunks
            .iter()
            .position(|chunk| chunk.tag == *b"CASM")
            .unwrap();
        assert_eq!(replay.native_sizes.len(), 27);
        for (size, case) in replay.native_sizes {
            assert_eq!(size, case.input.len());
            let mut earlier = vec![0; 47];
            earlier[0] = case.input[0];
            earlier[9] = 8;
            let records = smf::pack(&[
                smf::Chunk {
                    tag: *b"Sdec",
                    data: b"Main A".to_vec(),
                },
                smf::Chunk {
                    tag: *b"Ctb2",
                    data: earlier,
                },
                smf::Chunk {
                    tag: *b"Ctb2",
                    data: case.input,
                },
            ])
            .unwrap();
            style.chunks[casm].data = smf::pack(&[smf::Chunk {
                tag: *b"CSEG",
                data: records,
            }])
            .unwrap();
            let original = style.bytes().unwrap();
            let parsed = Document::parse(&original).unwrap();
            let route = parsed.firmware_cseg_route("Main A", 3).flatten().unwrap();
            assert_eq!(route.bytes, case.payload, "native size {size}");
            assert_eq!(parsed.style.bytes().unwrap(), original);
        }
        let track = style.track_chunks[0];
        let marker = style.tracks[0]
            .iter_mut()
            .find(|event| event.meta(6) == Some(b"SFF2"))
            .unwrap();
        *marker.data.last_mut().unwrap() = b'1';
        style.chunks[track].data = smf::encode_track(&style.tracks[0]).unwrap();
        assert_eq!(replay.legacy_sff1.len(), 32);
        for (name, case) in replay.legacy_sff1 {
            let mut records = smf::pack(&[smf::Chunk {
                tag: *b"Sdec",
                data: b"Main A".to_vec(),
            }])
            .unwrap();
            records.extend(case.records);
            style.chunks[casm].data = smf::pack(&[smf::Chunk {
                tag: *b"CSEG",
                data: records,
            }])
            .unwrap();
            let parsed = Document::parse(&style.bytes().unwrap()).unwrap();
            let route = parsed.firmware_cseg_route("Main A", 3).unwrap();
            assert_eq!(
                route.map(|route| &route.bytes),
                case.payload.as_ref(),
                "{name}"
            );
        }

        assert_eq!(replay.legacy_sff1_groups.len(), 4);
        for (name, case) in replay.legacy_sff1_groups {
            style.chunks[casm].data = case.groups;
            let parsed = Document::parse(&style.bytes().unwrap()).unwrap();
            for (section, expected) in case.sections {
                for source in 1..=16 {
                    let route = parsed.firmware_cseg_route(&section, source).unwrap();
                    assert_eq!(
                        route.map(|route| &route.bytes),
                        expected.get(&source),
                        "{name}, {section}, source {source}"
                    );
                }
            }
        }
    }
}
