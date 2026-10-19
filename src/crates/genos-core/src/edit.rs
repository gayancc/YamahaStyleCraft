use crate::{
    Error, Result,
    document::{Document, View},
    smf::{self, Event, Style},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteInput {
    pub track: usize,
    pub channel: u8,
    pub pitch: u8,
    pub velocity: u8,
    pub start: u64,
    pub duration: u64,
}
/// Channel message captured during MIDI record (CC/PC/pressure/bend) — SFM-044.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelEventInput {
    pub track: usize,
    pub tick: u64,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteUpdate {
    pub id: u64,
    pub start: u64,
    pub duration: u64,
    pub pitch: u8,
    pub velocity: u8,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerPoint {
    pub tick: u64,
    pub value: u8,
}
/// 14-bit pitch bend sample (0..=16383, center 8192).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PitchBendPoint {
    pub tick: u64,
    pub value: u16,
}
/// Section-scoped tempo automation point (SMF meta FF 51 · BPM 20..=400).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TempoPoint {
    pub tick: u64,
    pub bpm: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Command {
    /// A complete gesture/transform on one source part, committed once.
    PianoNotes {
        section: u64,
        channel: u8,
        #[serde(default)]
        updates: Vec<NoteUpdate>,
        #[serde(default)]
        add: Vec<NoteInput>,
        #[serde(default)]
        remove: Vec<u64>,
        /// Retimes owned CC/pitch-bend when note starts move (SFM Move controllers).
        #[serde(default = "default_true")]
        move_controllers: bool,
    },
    /// Explicit MIDI CC values; never a per-note volume property.
    ControllerPoints {
        section: u64,
        channel: u8,
        controller: u8,
        #[serde(default)]
        points: Vec<ControllerPoint>,
        #[serde(default)]
        remove: Vec<u64>,
    },
    SetStyleEffects {
        changes: Vec<crate::style_effects::Change>,
    },
    SetDrumScalars {
        channel: u8,
        address: crate::voice_catalog::Address,
        changes: Vec<crate::native_drums::Change>,
    },
    /// Remove per-key volume/pan/pitch overrides (empty `notes` = all overridden keys).
    ResetDrumScalars {
        channel: u8,
        address: crate::voice_catalog::Address,
        notes: Vec<u8>,
    },
    SetVoice {
        section: u64,
        channel: u8,
        address: crate::voice_catalog::Address,
    },
    AddNotes {
        section: u64,
        notes: Vec<NoteInput>,
        /// Optional CC/PC/bend events applied with the note batch (one undo).
        #[serde(default)]
        channel_events: Vec<ChannelEventInput>,
    },
    UpdateNotes {
        section: u64,
        ids: Vec<u64>,
        #[serde(default)]
        delta_tick: i64,
        #[serde(default)]
        delta_pitch: i16,
        velocity: Option<u8>,
        duration: Option<u64>,
        #[serde(default)]
        delta_duration: i64,
        /// When true and `delta_tick != 0`, shift owned CC/pitch-bend with note starts (SFM Move controllers).
        #[serde(default = "default_true")]
        move_controllers: bool,
    },
    DeleteNotes {
        ids: Vec<u64>,
    },
    Quantize {
        section: u64,
        ids: Vec<u64>,
        step: u64,
        #[serde(default = "default_true")]
        move_controllers: bool,
    },
    Casm {
        route: usize,
        field: String,
        value: u8,
    },
    SourceTracks {
        section: u64,
        actions: Vec<crate::casm::SourceAction>,
    },
    ReplacePhrase {
        section: u64,
        channel: u8,
        notes: Vec<crate::exchange::PhraseNote>,
        /// Optional bank/program from the library source (SFM-120 voice import).
        #[serde(default)]
        voice: Option<crate::voice_catalog::Address>,
        /// Absolute-tick CC lanes to replace in the destination section (SFM-120).
        #[serde(default)]
        controllers: Vec<(u8, Vec<ControllerPoint>)>,
    },
    ReplaceSectionPhrases {
        section: u64,
        phrases: Vec<crate::exchange::ChannelPhrase>,
        /// Per-channel voices from the library source (CASM never imported).
        #[serde(default)]
        voices: Vec<(u8, crate::voice_catalog::Address)>,
        /// Absolute-tick CC lanes keyed by (channel, controller).
        #[serde(default)]
        controllers: Vec<(u8, u8, Vec<ControllerPoint>)>,
    },
    CopyMaterial {
        source_section: u64,
        destination_section: u64,
        scope: crate::copy_material::CopyScope,
        fit: crate::copy_material::CopyFit,
    },
    SectionLength {
        section: u64,
        op: crate::section_length::SectionLengthOp,
    },
    SetEvent {
        id: u64,
        tick: u64,
        data: Vec<u8>,
    },
    AddEvent {
        section: u64,
        track: usize,
        tick: u64,
        data: Vec<u8>,
    },
    DeleteEvent {
        id: u64,
    },
    SetTempoMeter {
        tempo_bpm: f64,
        numerator: u8,
        denominator: u16,
    },
    /// Write one CC at each target section start (current or all sections) in one undo step.
    MixerCc {
        section: u64,
        channel: u8,
        controller: u8,
        value: u8,
        #[serde(default)]
        all_sections: bool,
    },
    /// Write several mixer CCs in one undo (Safe FX presets · SFM-MIXER-SAFE-FX).
    MixerCcBatch {
        section: u64,
        channel: u8,
        /// (controller, value) pairs; rejects bank select 0/32.
        changes: Vec<(u8, u8)>,
        #[serde(default)]
        all_sections: bool,
    },
    /// Static Mixer EQ High/Low via Genos NRPN packet (SFM-MIXER-TAB-EQ-HIGH-LOW).
    MixerEq {
        section: u64,
        channel: u8,
        band: crate::channel_eq::EqBand,
        value: u8,
        #[serde(default)]
        all_sections: bool,
    },
    /// Timed EQ High/Low automation as NRPN packets (SFM-EQ-AUTOMATION).
    MixerEqPoints {
        section: u64,
        channel: u8,
        band: crate::channel_eq::EqBand,
        #[serde(default)]
        points: Vec<ControllerPoint>,
        /// Remove existing EQ packets at these absolute ticks before writing.
        #[serde(default)]
        clear_ticks: Vec<u64>,
    },
    /// Timed pitch-bend automation (SFM-PITCH-BEND-LANE).
    PitchBendPoints {
        section: u64,
        channel: u8,
        #[serde(default)]
        points: Vec<PitchBendPoint>,
        #[serde(default)]
        remove: Vec<u64>,
    },
    /// Section-safe tempo automation as SMF meta FF 51 (SFM-TEMPO-LANE).
    /// Points must lie in `[section.start, section.end)`. Document tick-0
    /// baseline from `SetTempoMeter` is left intact when section.start > 0.
    TempoPoints {
        section: u64,
        #[serde(default)]
        points: Vec<TempoPoint>,
        #[serde(default)]
        remove: Vec<u64>,
    },
    /// Genos/Genos2 pitch-bend sensitivity via standard MIDI RPN 0,0 (SFM-PITCH-RANGE).
    /// Fingerprint from SFM `setRange`: CC101=0, CC100=0, CC6=semitones, CC101=127, CC100=127
    /// at section start. Style parts CH9–16 only (matches SFM plan gate).
    PitchBendRange {
        section: u64,
        channel: u8,
        /// Semitones 0..=24.
        semitones: u8,
        #[serde(default)]
        all_sections: bool,
    },
}
pub struct Session {
    pub document: Document,
    pub name: String,
    pub revision: u64,
    undo: Vec<Style>,
    redo: Vec<Style>,
    saved: Vec<u8>,
    pub project_extensions: serde_json::Map<String, serde_json::Value>,
}
fn reject(s: &str) -> Error {
    Error::Edit(s.into())
}
fn default_true() -> bool {
    true
}

/// One note start move used to retime owned CC / pitch-bend (SFM Move controllers).
struct NoteStartMove {
    channel: u8,
    from: u64,
    duration: u64,
    delta: i64,
    pitch: u8,
}

/// Shift CC (non-bank/NRPN) and pitch-bend events owned by moved note starts.
/// Skips section-start init tick, bank select, NRPN/RPN, and program change.
fn move_controllers_with_note_starts(
    style: &mut Style,
    section_start: u64,
    section_end: u64,
    moves: &[NoteStartMove],
    touched: &mut HashSet<usize>,
) -> Result<()> {
    if moves.is_empty() || moves.iter().all(|m| m.delta == 0) {
        return Ok(());
    }
    let mut ordered: Vec<&NoteStartMove> = moves.iter().filter(|m| m.delta != 0).collect();
    ordered.sort_by_key(|m| (m.from, m.pitch));
    let range_start = ordered.iter().map(|m| m.from).min().unwrap();
    let range_end = ordered
        .iter()
        .map(|m| m.from.saturating_add(m.duration))
        .max()
        .unwrap();
    let lo = section_end.saturating_sub(1).max(section_start);
    for (track, es) in style.tracks.iter_mut().enumerate() {
        let mut changed = false;
        for e in es.iter_mut() {
            if e.tick <= section_start || e.tick < range_start || e.tick >= range_end {
                continue;
            }
            if e.data.is_empty() {
                continue;
            }
            let status = e.data[0] & 0xf0;
            let ch = (e.data[0] & 0x0f) + 1;
            if status == 0xb0 {
                if e.data.len() < 3 {
                    continue;
                }
                // Bank select + NRPN/RPN (protect Mixer EQ packets).
                if matches!(e.data[1], 0 | 6 | 32 | 98 | 99 | 100 | 101) {
                    continue;
                }
            } else if status == 0xe0 {
                if e.data.len() < 3 {
                    continue;
                }
            } else {
                continue;
            }
            let mut owner: Option<&NoteStartMove> = None;
            for m in &ordered {
                if m.channel != ch {
                    continue;
                }
                if m.from > e.tick {
                    break;
                }
                owner = Some(m);
            }
            let Some(owner) = owner else { continue };
            let next = e
                .tick
                .checked_add_signed(owner.delta)
                .ok_or_else(|| reject("controller tick overflow"))?;
            e.tick = next.clamp(section_start.saturating_add(1).max(1), lo);
            changed = true;
        }
        if changed {
            touched.insert(track);
        }
    }
    Ok(())
}
impl Session {
    pub fn open(b: &[u8], name: String) -> Result<Self> {
        let document = Document::parse(b)?;
        Ok(Self {
            document,
            name,
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            saved: b.to_vec(),
            project_extensions: Default::default(),
        })
    }
    pub fn view(&self) -> View {
        let mut view = self.document.view(
            self.name.clone(),
            self.revision,
            self.document.style.bytes().is_ok_and(|b| b != self.saved),
            !self.undo.is_empty(),
            !self.redo.is_empty(),
        );
        view.ui_state = self.project_extensions.get("uiState").cloned();
        view
    }
    /// Persist editor workspace fields into the `.gss` project extensions (SFM-002).
    pub fn set_ui_state(&mut self, state: crate::project::ProjectUiState) {
        if let Ok(v) = serde_json::to_value(state) {
            self.project_extensions.insert("uiState".into(), v);
        }
    }
    pub fn mark_saved(&mut self) -> Result<()> {
        self.saved = self.document.style.bytes()?;
        Ok(())
    }
    pub fn mark_unsaved(&mut self) {
        self.saved.clear();
    }
    pub fn undo(&mut self) -> Result<()> {
        if let Some(style) = self.undo.pop() {
            let d = Document::from_style(style)?;
            self.redo.push(self.document.style.clone());
            self.document = d;
            self.revision += 1;
        }
        Ok(())
    }
    pub fn redo(&mut self) -> Result<()> {
        if let Some(style) = self.redo.pop() {
            let d = Document::from_style(style)?;
            self.undo.push(self.document.style.clone());
            self.document = d;
            self.revision += 1;
        }
        Ok(())
    }
    pub fn execute(&mut self, revision: u64, cmd: Command) -> Result<()> {
        if revision != self.revision {
            return Err(reject("stale revision; refresh the editor"));
        }
        let mut style = self.document.style.clone();
        let mut touched = HashSet::new();
        let mut expected_pairs: std::collections::HashMap<u64, u64> = self
            .document
            .notes
            .iter()
            .map(|n| (n.id, n.off_id))
            .collect();
        let mut next = style
            .tracks
            .iter()
            .flatten()
            .map(|e| e.id.max(e.order))
            .max()
            .unwrap_or(0)
            + 1;
        let bounds = |id| {
            self.document
                .sections
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| reject("section not found"))
        };
        let note_index: std::collections::HashMap<_, _> =
            self.document.notes.iter().map(|n| (n.id, n)).collect();
        let event_index: std::collections::HashMap<_, _> = self
            .document
            .style
            .tracks
            .iter()
            .enumerate()
            .flat_map(|(t, es)| es.iter().enumerate().map(move |(i, e)| (e.id, (t, i))))
            .collect();
        let find_note = |id| {
            note_index
                .get(&id)
                .copied()
                .ok_or_else(|| reject("note not found"))
        };
        let checked =
            |start: u64, duration: u64, pitch: u8, velocity: u8, section: u64| -> Result<()> {
                let s = bounds(section)?;
                if pitch > 127
                    || !(1..=127).contains(&velocity)
                    || duration == 0
                    || start < s.start
                    || start >= s.end
                    || start.checked_add(duration).is_none_or(|e| e > s.end)
                {
                    return Err(reject("note is outside MIDI or section bounds"));
                }
                Ok(())
            };
        match cmd {
            Command::PianoNotes {
                section,
                channel,
                updates,
                add,
                remove,
                move_controllers,
            } => {
                let s = bounds(section)?;
                if !(1..=16).contains(&channel)
                    || updates.len() + add.len() + remove.len() > 50_000
                    || updates.is_empty() && add.is_empty() && remove.is_empty()
                {
                    return Err(reject("invalid piano transaction scope or size"));
                }
                let mut ids = HashSet::new();
                let mut deleted = HashSet::new();
                let mut moves = Vec::new();
                for id in updates.iter().map(|n| n.id).chain(remove.iter().copied()) {
                    if !ids.insert(id) {
                        return Err(reject("duplicate piano note ID"));
                    }
                    let n = find_note(id)?;
                    if n.channel != channel
                        || n.start < s.start
                        || n.start >= s.end
                        || n.start + n.duration > s.end
                    {
                        return Err(reject("piano note is outside selected section/channel"));
                    }
                }
                if move_controllers {
                    for n in &updates {
                        let original = find_note(n.id)?;
                        let delta = n.start as i64 - original.start as i64;
                        if delta != 0 {
                            moves.push(NoteStartMove {
                                channel: original.channel,
                                from: original.start,
                                duration: original.duration,
                                delta,
                                pitch: original.pitch,
                            });
                        }
                    }
                }
                for n in updates {
                    checked(n.start, n.duration, n.pitch, n.velocity, section)?;
                    let original = find_note(n.id)?;
                    {
                        let (_, index) = event_index[&original.id];
                        let e = &mut style.tracks[original.track][index];
                        e.tick = n.start;
                        e.data[1] = n.pitch;
                        e.data[2] = n.velocity;
                    }
                    {
                        let (_, index) = event_index[&original.off_id];
                        let e = &mut style.tracks[original.track][index];
                        // View normalizes zero-length source notes to one tick for hit testing.
                        // A velocity/pitch/move patch must not lengthen that original edge.
                        e.tick = if n.duration == original.duration {
                            e.tick
                                .checked_add_signed(n.start as i64 - original.start as i64)
                                .ok_or_else(|| reject("note-off tick overflow"))?
                        } else {
                            n.start + n.duration
                        };
                        e.data[1] = n.pitch;
                    }
                    touched.insert(original.track);
                }
                for id in remove {
                    let n = find_note(id)?;
                    deleted.extend([n.id, n.off_id]);
                    expected_pairs.remove(&id);
                    touched.insert(n.track);
                }
                for events in &mut style.tracks {
                    events.retain(|e| !deleted.contains(&e.id));
                }
                for n in add {
                    if n.channel != channel {
                        return Err(reject("added note belongs to another channel"));
                    }
                    checked(n.start, n.duration, n.pitch, n.velocity, section)?;
                    let es = style
                        .tracks
                        .get_mut(n.track)
                        .ok_or_else(|| reject("invalid track"))?;
                    expected_pairs.insert(next, next + 1);
                    es.push(Event {
                        id: next,
                        order: next,
                        tick: n.start,
                        data: vec![0x90 | (channel - 1), n.pitch, n.velocity],
                    });
                    next += 1;
                    es.push(Event {
                        id: next,
                        order: next,
                        tick: n.start + n.duration,
                        data: vec![0x80 | (channel - 1), n.pitch, 0],
                    });
                    next += 1;
                    touched.insert(n.track);
                }
                move_controllers_with_note_starts(
                    &mut style,
                    s.start,
                    s.end,
                    &moves,
                    &mut touched,
                )?;
            }
            Command::ControllerPoints {
                section,
                channel,
                controller,
                points,
                remove,
            } => {
                let s = bounds(section)?;
                if !(1..=16).contains(&channel)
                    || controller > 119
                    || matches!(controller, 0 | 32)
                    || points.len() + remove.len() > 50_000
                    || points.is_empty() && remove.is_empty()
                {
                    return Err(reject("invalid MIDI controller transaction"));
                }
                let matches = |e: &Event| {
                    e.data.len() == 3
                        && e.data[0] == (0xb0 | (channel - 1))
                        && e.data[1] == controller
                        && e.tick >= s.start
                        && e.tick < s.end
                };
                let mut ticks = HashSet::new();
                for point in &points {
                    if point.value > 127
                        || point.tick < s.start
                        || point.tick >= s.end
                        || !ticks.insert(point.tick)
                    {
                        return Err(reject(
                            "controller point is duplicated or outside MIDI/section bounds",
                        ));
                    }
                }
                let mut removed = HashSet::new();
                for id in remove {
                    if !removed.insert(id) {
                        return Err(reject("duplicate controller ID"));
                    }
                    if !event_index
                        .get(&id)
                        .is_some_and(|(t, i)| matches(&self.document.style.tracks[*t][*i]))
                    {
                        return Err(reject("controller event is outside selected lane"));
                    }
                }
                let target_track = style
                    .tracks
                    .iter()
                    .position(|es| es.iter().any(matches))
                    .or_else(|| {
                        self.document
                            .notes
                            .iter()
                            .find(|n| n.channel == channel && n.start >= s.start && n.start < s.end)
                            .map(|n| n.track)
                    })
                    .or_else(|| {
                        style
                            .tracks
                            .iter()
                            .position(|es| es.iter().any(|e| e.channel() == Some(channel)))
                    })
                    .unwrap_or(0);
                for (track, es) in style.tracks.iter_mut().enumerate() {
                    if es.iter().any(|e| removed.contains(&e.id)) {
                        es.retain(|e| !removed.contains(&e.id));
                        touched.insert(track);
                    }
                }
                let mut effective: std::collections::HashMap<u64, (usize, usize, u64, u64)> =
                    std::collections::HashMap::new();
                for (track, es) in style.tracks.iter().enumerate() {
                    for (index, e) in es.iter().enumerate().filter(|(_, e)| matches(e)) {
                        let entry = effective
                            .entry(e.tick)
                            .or_insert((track, index, e.order, e.id));
                        if (e.order, e.id) > (entry.2, entry.3) {
                            *entry = (track, index, e.order, e.id);
                        }
                    }
                }
                for point in points {
                    // Modify the effective last event at a tie, preserving earlier payloads/order.
                    if let Some((track, index, _, _)) = effective.get(&point.tick) {
                        style.tracks[*track][*index].data[2] = point.value;
                        touched.insert(*track);
                    } else {
                        style.tracks[target_track].push(Event {
                            id: next,
                            order: next,
                            tick: point.tick,
                            data: vec![0xb0 | (channel - 1), controller, point.value],
                        });
                        next += 1;
                        touched.insert(target_track);
                    }
                }
            }
            Command::SetStyleEffects { changes } => {
                crate::style_effects::apply(&mut style, changes)?;
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
            }
            Command::SetDrumScalars {
                channel,
                address,
                changes,
            } => {
                crate::native_drums::apply(&mut style, channel, address, changes)?;
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
            }
            Command::ResetDrumScalars {
                channel,
                address,
                notes,
            } => {
                crate::native_drums::reset_keys(&mut style, channel, address, notes)?;
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
            }
            Command::SetVoice {
                section,
                channel,
                address,
            } => {
                crate::voice_catalog::set_voice(&mut style, section, channel, address)?;
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
            }
            Command::ReplacePhrase {
                section,
                channel,
                notes,
                voice,
                controllers,
            } => {
                let changes = crate::exchange::replace(&mut style, section, channel, notes)?;
                for id in changes.removed {
                    expected_pairs.remove(&id);
                }
                expected_pairs.extend(changes.added);
                if let Some(address) = voice {
                    crate::voice_catalog::set_voice(&mut style, section, channel, address)?;
                }
                let s = bounds(section)?;
                for (controller, points) in controllers {
                    next = replace_controller_lane(
                        &mut style,
                        s.start,
                        s.end,
                        channel,
                        controller,
                        &points,
                        next,
                        &mut touched,
                    )?;
                }
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
            }
            Command::ReplaceSectionPhrases {
                section,
                phrases,
                voices,
                controllers,
            } => {
                let changes = crate::exchange::replace_section(&mut style, section, phrases)?;
                for id in changes.removed {
                    expected_pairs.remove(&id);
                }
                expected_pairs.extend(changes.added);
                for (channel, address) in voices {
                    crate::voice_catalog::set_voice(&mut style, section, channel, address)?;
                }
                let s = bounds(section)?;
                for (channel, controller, points) in controllers {
                    next = replace_controller_lane(
                        &mut style,
                        s.start,
                        s.end,
                        channel,
                        controller,
                        &points,
                        next,
                        &mut touched,
                    )?;
                }
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
            }
            Command::CopyMaterial {
                source_section,
                destination_section,
                scope,
                fit,
            } => {
                let changes = crate::copy_material::apply(
                    &mut style,
                    source_section,
                    destination_section,
                    scope,
                    fit,
                )?;
                for id in changes.removed {
                    expected_pairs.remove(&id);
                }
                expected_pairs.extend(changes.added);
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
            }
            Command::SectionLength { section, op } => {
                let changes = crate::section_length::apply(&mut style, section, op)?;
                for id in changes.removed {
                    expected_pairs.remove(&id);
                }
                expected_pairs.extend(changes.added);
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
            }
            Command::SourceTracks { section, actions } => {
                let cloned_pairs = crate::casm::apply(&mut style, section, actions)?;
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
                // Check exact copied edge identities instead of accepting any
                // new pairs that the candidate parser happens to discover.
                expected_pairs.extend(cloned_pairs);
            }
            Command::AddNotes {
                section,
                notes,
                channel_events,
            } => {
                if notes.len() > 50_000 || channel_events.len() > 50_000 {
                    return Err(reject("invalid note batch size"));
                }
                if notes.is_empty() && channel_events.is_empty() {
                    return Err(reject("invalid note batch size"));
                }
                for n in notes {
                    checked(n.start, n.duration, n.pitch, n.velocity, section)?;
                    if !(1..=16).contains(&n.channel) {
                        return Err(reject("invalid channel"));
                    }
                    let es = style
                        .tracks
                        .get_mut(n.track)
                        .ok_or_else(|| reject("invalid track"))?;
                    expected_pairs.insert(next, next + 1);
                    es.push(Event {
                        id: next,
                        tick: n.start,
                        order: next,
                        data: vec![0x90 | (n.channel - 1), n.pitch, n.velocity],
                    });
                    next += 1;
                    es.push(Event {
                        id: next,
                        tick: n.start + n.duration,
                        order: next,
                        data: vec![0x80 | (n.channel - 1), n.pitch, 0],
                    });
                    next += 1;
                    touched.insert(n.track);
                }
                let s = bounds(section)?;
                for ev in channel_events {
                    validate_channel(&ev.data)?;
                    if ev.tick < s.start || ev.tick >= s.end {
                        return Err(reject("event outside section"));
                    }
                    let es = style
                        .tracks
                        .get_mut(ev.track)
                        .ok_or_else(|| reject("invalid track"))?;
                    es.push(Event {
                        id: next,
                        tick: ev.tick,
                        order: next,
                        data: ev.data,
                    });
                    next += 1;
                    touched.insert(ev.track);
                }
            }
            Command::UpdateNotes {
                section,
                ids,
                delta_tick,
                delta_pitch,
                velocity,
                duration,
                delta_duration,
                move_controllers,
            } => {
                check_ids(&ids)?;
                let s = bounds(section)?;
                let mut moves = Vec::new();
                if move_controllers && delta_tick != 0 {
                    for &id in &ids {
                        let n = find_note(id)?;
                        moves.push(NoteStartMove {
                            channel: n.channel,
                            from: n.start,
                            duration: n.duration,
                            delta: delta_tick,
                            pitch: n.pitch,
                        });
                    }
                }
                for id in ids {
                    let n = find_note(id)?;
                    let preserve_duration = duration.is_none() && delta_duration == 0;
                    let start = n
                        .start
                        .checked_add_signed(delta_tick)
                        .ok_or_else(|| reject("negative/overflow tick"))?;
                    let pitch = u8::try_from(n.pitch as i16 + delta_pitch)
                        .map_err(|_| reject("pitch outside range"))?;
                    let duration = duration
                        .unwrap_or(n.duration)
                        .checked_add_signed(delta_duration)
                        .ok_or_else(|| reject("invalid duration delta"))?;
                    let velocity = velocity.unwrap_or(n.velocity);
                    checked(start, duration, pitch, velocity, section)?;
                    {
                        let e = &mut style.tracks[n.track][event_index[&n.id].1];
                        e.tick = start;
                        e.data[1] = pitch;
                        e.data[2] = velocity;
                    }
                    {
                        let e = &mut style.tracks[n.track][event_index[&n.off_id].1];
                        e.tick = if preserve_duration {
                            e.tick
                                .checked_add_signed(delta_tick)
                                .ok_or_else(|| reject("note-off tick overflow"))?
                        } else {
                            start + duration
                        };
                        e.data[1] = pitch;
                    }
                    touched.insert(n.track);
                }
                move_controllers_with_note_starts(
                    &mut style,
                    s.start,
                    s.end,
                    &moves,
                    &mut touched,
                )?;
            }
            Command::DeleteNotes { ids } => {
                check_ids(&ids)?;
                let mut remove = HashSet::new();
                for id in ids {
                    let n = find_note(id)?;
                    remove.insert(n.id);
                    remove.insert(n.off_id);
                    expected_pairs.remove(&n.id);
                    touched.insert(n.track);
                }
                for es in &mut style.tracks {
                    es.retain(|e| !remove.contains(&e.id));
                }
            }
            Command::Quantize {
                section,
                ids,
                step,
                move_controllers,
            } => {
                check_ids(&ids)?;
                let s = bounds(section)?;
                if step == 0 || step > s.end - s.start {
                    return Err(reject("invalid quantize step"));
                }
                let mut moves = Vec::new();
                for &id in &ids {
                    let n = find_note(id)?;
                    let local = n
                        .start
                        .checked_sub(s.start)
                        .ok_or_else(|| reject("note outside section"))?;
                    let start =
                        s.start + ((local + step / 2) / step * step).min(s.end - s.start - 1);
                    let original_off =
                        self.document.style.tracks[n.track][event_index[&n.off_id].1].tick;
                    let duration = original_off.saturating_sub(n.start).min(s.end - start);
                    let dur = duration.max(1);
                    checked(start, dur, n.pitch, n.velocity, section)?;
                    if move_controllers {
                        let delta = start as i64 - n.start as i64;
                        if delta != 0 {
                            moves.push(NoteStartMove {
                                channel: n.channel,
                                from: n.start,
                                duration: n.duration,
                                delta,
                                pitch: n.pitch,
                            });
                        }
                    }
                    style.tracks[n.track][event_index[&n.id].1].tick = start;
                    style.tracks[n.track][event_index[&n.off_id].1].tick = start + dur;
                    touched.insert(n.track);
                }
                move_controllers_with_note_starts(
                    &mut style,
                    s.start,
                    s.end,
                    &moves,
                    &mut touched,
                )?;
            }
            Command::Casm {
                route,
                field,
                value,
            } => {
                let r = self
                    .document
                    .routes
                    .get(route)
                    .ok_or_else(|| reject("route not found"))?;
                let offset = match field.as_str() {
                    "root" if value <= 11 => 18,
                    "chord" if value <= 33 => 19,
                    "ntr" if value <= 1 => 28,
                    "ntt" if value <= 1 => 29,
                    _ => return Err(reject("unverified CASM field or unsupported value")),
                };
                if offset >= 28 && r.bytes[28] == 2 {
                    return Err(Error::Unsupported(
                        "Guitar rule editing requires additional validation".into(),
                    ));
                }
                let byte = &mut style.chunks[r.chunk].data[r.offset + offset];
                *byte = if offset == 29 {
                    (*byte & 0x80) | value
                } else {
                    value
                };
            }
            Command::SetEvent { id, tick, data } => {
                validate_channel(&data)?;
                let (t, es) = style
                    .tracks
                    .iter_mut()
                    .enumerate()
                    .find(|(_, es)| es.iter().any(|e| e.id == id))
                    .ok_or_else(|| reject("event not found"))?;
                let e = es.iter_mut().find(|e| e.id == id).unwrap();
                if e.is_on() || e.is_off() || e.data[0] >= 0xf0 {
                    return Err(reject(
                        "use the note editor for notes; metadata is read-only",
                    ));
                }
                if tick > smf::MAX_TICK {
                    return Err(reject("tick exceeds limit"));
                }
                let s = self
                    .document
                    .sections
                    .iter()
                    .find(|s| e.tick >= s.start && e.tick < s.end)
                    .ok_or_else(|| reject("header events are read-only"))?;
                if tick < s.start || tick >= s.end {
                    return Err(reject("event outside its original section"));
                }
                let kind = |d: &[u8]| {
                    if d[0] >> 4 == 0xb && matches!(d[1], 0 | 32) {
                        (d[0], d[1])
                    } else {
                        (d[0], 255)
                    }
                };
                if kind(&e.data) != kind(&data) {
                    return Err(reject("event source channel and typed kind cannot change"));
                }
                e.tick = tick;
                e.data = data;
                touched.insert(t);
            }
            Command::AddEvent {
                section,
                track,
                tick,
                data,
            } => {
                validate_channel(&data)?;
                let s = bounds(section)?;
                if tick < s.start || tick >= s.end {
                    return Err(reject("event outside section"));
                }
                let es = style
                    .tracks
                    .get_mut(track)
                    .ok_or_else(|| reject("track not found"))?;
                es.push(Event {
                    id: next,
                    tick,
                    order: next,
                    data,
                });
                touched.insert(track);
            }
            Command::DeleteEvent { id } => {
                let (t, es) = style
                    .tracks
                    .iter_mut()
                    .enumerate()
                    .find(|(_, es)| es.iter().any(|e| e.id == id))
                    .ok_or_else(|| reject("event not found"))?;
                let e = es.iter().find(|e| e.id == id).unwrap();
                validate_channel(&e.data)?;
                es.retain(|e| e.id != id);
                touched.insert(t);
            }
            Command::SetTempoMeter {
                tempo_bpm,
                numerator,
                denominator,
            } => {
                if !tempo_bpm.is_finite() || !(20.0..=400.0).contains(&tempo_bpm) {
                    return Err(reject("tempo must be a finite BPM between 20 and 400"));
                }
                if !(1..=16).contains(&numerator) {
                    return Err(reject("numerator must be 1..16"));
                }
                if !matches!(denominator, 1 | 2 | 4 | 8 | 16) {
                    return Err(reject("denominator must be 1, 2, 4, 8, or 16"));
                }
                crate::blank::apply_tempo_meter(&mut style, tempo_bpm, numerator, denominator)?;
                for (track, events) in style.tracks.iter().enumerate() {
                    if events != &self.document.style.tracks[track] {
                        touched.insert(track);
                    }
                }
            }
            Command::MixerCc {
                section,
                channel,
                controller,
                value,
                all_sections,
            } => {
                apply_mixer_ccs(
                    &mut style,
                    &self.document,
                    &mut next,
                    &mut touched,
                    section,
                    channel,
                    &[(controller, value)],
                    all_sections,
                )?;
            }
            Command::MixerCcBatch {
                section,
                channel,
                changes,
                all_sections,
            } => {
                apply_mixer_ccs(
                    &mut style,
                    &self.document,
                    &mut next,
                    &mut touched,
                    section,
                    channel,
                    &changes,
                    all_sections,
                )?;
            }
            Command::MixerEq {
                section,
                channel,
                band,
                value,
                all_sections,
            } => {
                if !(1..=16).contains(&channel) || value > 127 {
                    return Err(reject("invalid mixer EQ"));
                }
                let _ = bounds(section)?;
                let targets: Vec<_> = if all_sections {
                    self.document.sections.iter().collect()
                } else {
                    self.document
                        .sections
                        .iter()
                        .filter(|s| s.id == section)
                        .collect()
                };
                if targets.is_empty() {
                    return Err(reject("section not found"));
                }
                let status = 0xb0 | (channel - 1);
                let packet = crate::channel_eq::nrpn_packet(band, value);
                for s in targets {
                    let track = self
                        .document
                        .notes
                        .iter()
                        .find(|n| n.channel == channel && n.start >= s.start && n.start < s.end)
                        .map(|n| n.track)
                        .or_else(|| {
                            style
                                .tracks
                                .iter()
                                .position(|es| es.iter().any(|e| e.channel() == Some(channel)))
                        })
                        .unwrap_or(0);
                    let es = style
                        .tracks
                        .get_mut(track)
                        .ok_or_else(|| reject("track not found"))?;
                    let remove: std::collections::HashSet<_> =
                        crate::channel_eq::packet_indices(es, channel, s.start, band)
                            .into_iter()
                            .collect();
                    if !remove.is_empty() {
                        let mut kept = Vec::with_capacity(es.len());
                        for (i, e) in es.iter().enumerate() {
                            if !remove.contains(&i) {
                                kept.push(e.clone());
                            }
                        }
                        *es = kept;
                    }
                    for (controller, v) in packet {
                        es.push(Event {
                            id: next,
                            tick: s.start,
                            order: next,
                            data: vec![status, controller, v],
                        });
                        next += 1;
                    }
                    touched.insert(track);
                }
            }
            Command::PitchBendPoints {
                section,
                channel,
                points,
                remove,
            } => {
                let s = bounds(section)?;
                if !(1..=16).contains(&channel)
                    || points.len() + remove.len() > 50_000
                    || (points.is_empty() && remove.is_empty())
                {
                    return Err(reject("invalid pitch bend transaction"));
                }
                let status = 0xe0 | (channel - 1);
                let matches = |e: &Event| {
                    e.data.len() == 3 && e.data[0] == status && e.tick >= s.start && e.tick < s.end
                };
                let mut ticks = HashSet::new();
                for point in &points {
                    if point.value > 16_383
                        || point.tick < s.start
                        || point.tick >= s.end
                        || !ticks.insert(point.tick)
                    {
                        return Err(reject(
                            "pitch bend point duplicated or outside MIDI/section bounds",
                        ));
                    }
                }
                let mut removed = HashSet::new();
                for id in remove {
                    if !removed.insert(id) {
                        return Err(reject("duplicate pitch bend ID"));
                    }
                    if !event_index
                        .get(&id)
                        .is_some_and(|(t, i)| matches(&self.document.style.tracks[*t][*i]))
                    {
                        return Err(reject("pitch bend event is outside selected lane"));
                    }
                }
                let target_track = style
                    .tracks
                    .iter()
                    .position(|es| es.iter().any(matches))
                    .or_else(|| {
                        self.document
                            .notes
                            .iter()
                            .find(|n| n.channel == channel && n.start >= s.start && n.start < s.end)
                            .map(|n| n.track)
                    })
                    .or_else(|| {
                        style
                            .tracks
                            .iter()
                            .position(|es| es.iter().any(|e| e.channel() == Some(channel)))
                    })
                    .unwrap_or(0);
                for (track, es) in style.tracks.iter_mut().enumerate() {
                    if es.iter().any(|e| removed.contains(&e.id)) {
                        es.retain(|e| !removed.contains(&e.id));
                        touched.insert(track);
                    }
                }
                let mut effective: std::collections::HashMap<u64, (usize, usize, u64, u64)> =
                    std::collections::HashMap::new();
                for (track, es) in style.tracks.iter().enumerate() {
                    for (index, e) in es.iter().enumerate().filter(|(_, e)| matches(e)) {
                        let entry = effective
                            .entry(e.tick)
                            .or_insert((track, index, e.order, e.id));
                        if (e.order, e.id) > (entry.2, entry.3) {
                            *entry = (track, index, e.order, e.id);
                        }
                    }
                }
                for point in points {
                    let lsb = (point.value & 0x7f) as u8;
                    let msb = ((point.value >> 7) & 0x7f) as u8;
                    if let Some((track, index, _, _)) = effective.get(&point.tick) {
                        style.tracks[*track][*index].data = vec![status, lsb, msb];
                        touched.insert(*track);
                    } else {
                        style.tracks[target_track].push(Event {
                            id: next,
                            order: next,
                            tick: point.tick,
                            data: vec![status, lsb, msb],
                        });
                        next += 1;
                        touched.insert(target_track);
                    }
                }
            }
            Command::PitchBendRange {
                section,
                channel,
                semitones,
                all_sections,
            } => {
                if !(9..=16).contains(&channel) || semitones > 24 {
                    return Err(reject(
                        "pitch bend range requires style channel 9–16 and semitones 0..=24",
                    ));
                }
                apply_pitch_bend_range(
                    &mut style,
                    &self.document,
                    &mut next,
                    &mut touched,
                    section,
                    channel,
                    semitones,
                    all_sections,
                )?;
            }
            Command::TempoPoints {
                section,
                points,
                remove,
            } => {
                let s = bounds(section)?;
                if points.len() + remove.len() > 50_000
                    || (points.is_empty() && remove.is_empty())
                {
                    return Err(reject("invalid tempo transaction"));
                }
                let matches = |e: &Event| {
                    e.meta(0x51).is_some() && e.tick >= s.start && e.tick < s.end
                };
                let mut ticks = HashSet::new();
                for point in &points {
                    if !point.bpm.is_finite()
                        || !(20.0..=400.0).contains(&point.bpm)
                        || point.tick < s.start
                        || point.tick >= s.end
                        || !ticks.insert(point.tick)
                    {
                        return Err(reject(
                            "tempo point duplicated, outside section, or BPM not in 20..=400",
                        ));
                    }
                }
                let mut removed = HashSet::new();
                for id in remove {
                    if !removed.insert(id) {
                        return Err(reject("duplicate tempo event ID"));
                    }
                    if !event_index
                        .get(&id)
                        .is_some_and(|(t, i)| matches(&self.document.style.tracks[*t][*i]))
                    {
                        return Err(reject("tempo event is outside selected section"));
                    }
                }
                let target_track = style
                    .tracks
                    .iter()
                    .position(|es| es.iter().any(matches))
                    .unwrap_or(0);
                for (track, es) in style.tracks.iter_mut().enumerate() {
                    if es.iter().any(|e| removed.contains(&e.id)) {
                        es.retain(|e| !removed.contains(&e.id));
                        touched.insert(track);
                    }
                }
                let mut effective: std::collections::HashMap<u64, (usize, usize, u64, u64)> =
                    std::collections::HashMap::new();
                for (track, es) in style.tracks.iter().enumerate() {
                    for (index, e) in es.iter().enumerate().filter(|(_, e)| matches(e)) {
                        let entry = effective
                            .entry(e.tick)
                            .or_insert((track, index, e.order, e.id));
                        if (e.order, e.id) > (entry.2, entry.3) {
                            *entry = (track, index, e.order, e.id);
                        }
                    }
                }
                for point in points {
                    let tempo = crate::blank::tempo_bytes(point.bpm)?;
                    let data = vec![0xff, 0x51, 3, tempo[0], tempo[1], tempo[2]];
                    if let Some((track, index, _, _)) = effective.get(&point.tick) {
                        style.tracks[*track][*index].data = data;
                        touched.insert(*track);
                    } else {
                        style.tracks[target_track].push(Event {
                            id: next,
                            order: next,
                            tick: point.tick,
                            data,
                        });
                        next += 1;
                        touched.insert(target_track);
                    }
                }
            }
            Command::MixerEqPoints {
                section,
                channel,
                band,
                points,
                clear_ticks,
            } => {
                if !(1..=16).contains(&channel)
                    || points.len() + clear_ticks.len() > 50_000
                    || (points.is_empty() && clear_ticks.is_empty())
                {
                    return Err(reject("invalid EQ automation batch"));
                }
                let s = bounds(section)?;
                let mut ticks = HashSet::new();
                for &t in &clear_ticks {
                    if t < s.start || t >= s.end || !ticks.insert(t) {
                        return Err(reject("EQ clear tick outside section or duplicated"));
                    }
                }
                for p in &points {
                    if p.value > 127 || p.tick < s.start || p.tick >= s.end || !ticks.insert(p.tick)
                    {
                        return Err(reject("EQ point duplicated or outside MIDI/section bounds"));
                    }
                }
                let status = 0xb0 | (channel - 1);
                let track = self
                    .document
                    .notes
                    .iter()
                    .find(|n| n.channel == channel && n.start >= s.start && n.start < s.end)
                    .map(|n| n.track)
                    .or_else(|| {
                        style
                            .tracks
                            .iter()
                            .position(|es| es.iter().any(|e| e.channel() == Some(channel)))
                    })
                    .unwrap_or(0);
                let es = style
                    .tracks
                    .get_mut(track)
                    .ok_or_else(|| reject("track not found"))?;
                let mut remove = HashSet::new();
                for &tick in ticks.iter() {
                    remove.extend(crate::channel_eq::packet_indices(es, channel, tick, band));
                }
                if !remove.is_empty() {
                    let mut kept = Vec::with_capacity(es.len());
                    for (i, e) in es.iter().enumerate() {
                        if !remove.contains(&i) {
                            kept.push(e.clone());
                        }
                    }
                    *es = kept;
                }
                for p in &points {
                    for (controller, v) in crate::channel_eq::nrpn_packet(band, p.value) {
                        es.push(Event {
                            id: next,
                            tick: p.tick,
                            order: next,
                            data: vec![status, controller, v],
                        });
                        next += 1;
                    }
                }
                touched.insert(track);
            }
        }
        if style.tracks == self.document.style.tracks && style.chunks == self.document.style.chunks
        {
            return Ok(());
        }
        for t in touched {
            style.tracks[t].sort_by_key(|e| (e.tick, e.order, e.id));
            style.chunks[style.track_chunks[t]].data = smf::encode_track(&style.tracks[t])?;
        }
        let bytes = style.bytes()?;
        if bytes == self.document.style.bytes()? {
            return Ok(());
        }
        // Validate the entire candidate before committing any change or history.
        Document::parse(&bytes)?;
        let candidate = Document::from_style(style)?;
        if candidate
            .notes
            .iter()
            .any(|n| expected_pairs.get(&n.id) != Some(&n.off_id))
            || candidate.notes.len() != expected_pairs.len()
        {
            return Err(reject(
                "edit would change FIFO pairing of overlapping notes; shorten or move the conflicting notes first",
            ));
        }
        self.undo.push(self.document.style.clone());
        if self.undo.len() > 32 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.document = candidate;
        self.revision += 1;
        Ok(())
    }
}
fn check_ids(ids: &[u64]) -> Result<()> {
    if ids.is_empty()
        || ids.len() > 50_000
        || ids.iter().copied().collect::<HashSet<_>>().len() != ids.len()
    {
        return Err(reject("invalid or duplicate selection"));
    }
    Ok(())
}

/// Replace all CC events for one controller in `[start, end)` with absolute-tick points.
#[allow(clippy::too_many_arguments)]
fn replace_controller_lane(
    style: &mut Style,
    start: u64,
    end: u64,
    channel: u8,
    controller: u8,
    points: &[ControllerPoint],
    mut next: u64,
    touched: &mut HashSet<usize>,
) -> Result<u64> {
    if !(1..=16).contains(&channel)
        || controller > 119
        || matches!(controller, 0 | 32)
        || points.len() > 50_000
    {
        return Err(reject("invalid MIDI controller transaction"));
    }
    let status = 0xb0 | (channel - 1);
    let matches = |e: &Event| {
        e.data.len() == 3
            && e.data[0] == status
            && e.data[1] == controller
            && e.tick >= start
            && e.tick < end
    };
    let mut ticks = HashSet::new();
    for point in points {
        if point.value > 127 || point.tick < start || point.tick >= end || !ticks.insert(point.tick)
        {
            return Err(reject(
                "controller point is duplicated or outside MIDI/section bounds",
            ));
        }
    }
    let target_track = style
        .tracks
        .iter()
        .position(|es| es.iter().any(matches))
        .or_else(|| {
            style
                .tracks
                .iter()
                .position(|es| es.iter().any(|e| e.channel() == Some(channel)))
        })
        .unwrap_or(0);
    for (track, es) in style.tracks.iter_mut().enumerate() {
        let before = es.len();
        es.retain(|e| !matches(e));
        if es.len() != before {
            touched.insert(track);
        }
    }
    for point in points {
        style.tracks[target_track].push(Event {
            id: next,
            order: next,
            tick: point.tick,
            data: vec![status, controller, point.value],
        });
        next += 1;
        touched.insert(target_track);
    }
    Ok(next)
}
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn apply_pitch_bend_range(
    style: &mut Style,
    document: &Document,
    next: &mut u64,
    touched: &mut HashSet<usize>,
    section: u64,
    channel: u8,
    semitones: u8,
    all_sections: bool,
) -> Result<()> {
    if document.sections.iter().all(|s| s.id != section) {
        return Err(reject("section not found"));
    }
    let targets: Vec<_> = if all_sections {
        document.sections.iter().collect()
    } else {
        document
            .sections
            .iter()
            .filter(|s| s.id == section)
            .collect()
    };
    if targets.is_empty() {
        return Err(reject("section not found"));
    }
    let status = 0xb0 | (channel - 1);
    // SFM Genos/Genos2 fingerprint (`pitch-bend-map.js` setRange).
    let packet = [
        (101u8, 0u8),
        (100, 0),
        (6, semitones),
        (101, 127),
        (100, 127),
    ];
    for s in targets {
        let track = document
            .notes
            .iter()
            .find(|n| n.channel == channel && n.start >= s.start && n.start < s.end)
            .map(|n| n.track)
            .or_else(|| {
                style
                    .tracks
                    .iter()
                    .position(|es| es.iter().any(|e| e.channel() == Some(channel)))
            })
            .unwrap_or(0);
        let es = style
            .tracks
            .get_mut(track)
            .ok_or_else(|| reject("track not found"))?;
        // Drop prior RPN range / data-entry CCs at this section start for the part.
        let before = es.len();
        es.retain(|e| {
            !(e.tick == s.start
                && e.data.len() == 3
                && e.data[0] == status
                && matches!(e.data[1], 6 | 100 | 101))
        });
        if es.len() != before {
            touched.insert(track);
        }
        for &(controller, value) in &packet {
            es.push(Event {
                id: *next,
                tick: s.start,
                order: *next,
                data: vec![status, controller, value],
            });
            *next += 1;
        }
        touched.insert(track);
    }
    Ok(())
}

fn apply_mixer_ccs(
    style: &mut Style,
    document: &Document,
    next: &mut u64,
    touched: &mut HashSet<usize>,
    section: u64,
    channel: u8,
    changes: &[(u8, u8)],
    all_sections: bool,
) -> Result<()> {
    if !(1..=16).contains(&channel) || changes.is_empty() || changes.len() > 64 {
        return Err(reject("invalid mixer controller"));
    }
    for &(controller, value) in changes {
        if controller > 119 || matches!(controller, 0 | 32) || value > 127 {
            return Err(reject("invalid mixer controller"));
        }
    }
    if document.sections.iter().all(|s| s.id != section) {
        return Err(reject("section not found"));
    }
    let targets: Vec<_> = if all_sections {
        document.sections.iter().collect()
    } else {
        document
            .sections
            .iter()
            .filter(|s| s.id == section)
            .collect()
    };
    if targets.is_empty() {
        return Err(reject("section not found"));
    }
    let status = 0xb0 | (channel - 1);
    for s in targets {
        for &(controller, value) in changes {
            let data = vec![status, controller, value];
            let matches = |e: &Event| {
                e.data.len() == 3
                    && e.data[0] == status
                    && e.data[1] == controller
                    && e.tick == s.start
            };
            let existing = style
                .tracks
                .iter()
                .enumerate()
                .find_map(|(t, es)| es.iter().position(matches).map(|i| (t, i)));
            if let Some((t, i)) = existing {
                style.tracks[t][i].data = data;
                touched.insert(t);
            } else {
                let track = document
                    .notes
                    .iter()
                    .find(|n| n.channel == channel && n.start >= s.start && n.start < s.end)
                    .map(|n| n.track)
                    .or_else(|| {
                        style
                            .tracks
                            .iter()
                            .position(|es| es.iter().any(|e| e.channel() == Some(channel)))
                    })
                    .unwrap_or(0);
                let es = style
                    .tracks
                    .get_mut(track)
                    .ok_or_else(|| reject("track not found"))?;
                es.push(Event {
                    id: *next,
                    tick: s.start,
                    order: *next,
                    data,
                });
                *next += 1;
                touched.insert(track);
            }
        }
    }
    Ok(())
}

fn validate_channel(data: &[u8]) -> Result<()> {
    let Some(s) = data.first() else {
        return Err(reject("empty event"));
    };
    if !matches!(s >> 4, 0xb..=0xe)
        || data.len() != if matches!(s >> 4, 0xc | 0xd) { 2 } else { 3 }
        || data[1..].iter().any(|x| *x > 127)
    {
        return Err(reject("expected controller/program/pressure/bend event"));
    }
    Ok(())
}
