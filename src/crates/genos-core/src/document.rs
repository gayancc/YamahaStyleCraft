use crate::{
    Error, Result,
    smf::{self, Style},
};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};

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
impl Document {
    /// Active SMF timing metadata at a tick, including changes in other tracks.
    pub fn timing_at(&self, tick: u64) -> (f64, u8, u16) {
        let mut events: Vec<_> = self
            .style
            .tracks
            .iter()
            .flatten()
            .filter(|e| e.tick <= tick)
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
        let mut routes = Vec::new();
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
                    let labels = records
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
                    let mut offset = group_offset + 8;
                    for r in records {
                        if r.tag == *b"Ctb2" {
                            if r.data.len() == 47 && r.data[0] < 16 && r.data[9] < 16 {
                                routes.push(Route {
                                    id: routes.len(),
                                    sections: labels.clone(),
                                    source: r.data[0] + 1,
                                    destination: r.data[9] + 1,
                                    bytes: r.data.clone(),
                                    source_root: r.data[18],
                                    source_chord: r.data[19],
                                    middle_ntr: r.data[28],
                                    middle_ntt: r.data[29] & 127,
                                    middle_ntt_bass: r.data[29] & 128 != 0,
                                    chunk,
                                    offset: offset + 8,
                                });
                            } else {
                                warnings.push(format!(
                                    "Unsupported Ctb2 record ({} bytes) retained opaque.",
                                    r.data.len()
                                ));
                            }
                        } else if r.tag == *b"Ctab" {
                            warnings.push("SFF1 Ctab retained opaque; conversion and rule editing unavailable.".into());
                        }
                        offset += 8 + r.data.len();
                    }
                }
                group_offset += 8 + g.data.len();
            }
        }
        Ok(Self {
            style,
            sections,
            notes,
            routes,
            warnings,
        })
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
            warnings: self.warnings.clone(),
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
