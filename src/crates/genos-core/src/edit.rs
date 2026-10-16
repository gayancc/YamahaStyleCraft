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
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Command {
    AddNotes {
        section: u64,
        notes: Vec<NoteInput>,
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
    },
    DeleteNotes {
        ids: Vec<u64>,
    },
    Quantize {
        section: u64,
        ids: Vec<u64>,
        step: u64,
    },
    Casm {
        route: usize,
        field: String,
        value: u8,
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
        self.document.view(
            self.name.clone(),
            self.revision,
            self.document.style.bytes().is_ok_and(|b| b != self.saved),
            !self.undo.is_empty(),
            !self.redo.is_empty(),
        )
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
        let find_note = |id| {
            self.document
                .notes
                .iter()
                .find(|n| n.id == id)
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
            Command::AddNotes { section, notes } => {
                if notes.is_empty() || notes.len() > 50_000 {
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
            }
            Command::UpdateNotes {
                section,
                ids,
                delta_tick,
                delta_pitch,
                velocity,
                duration,
                delta_duration,
            } => {
                check_ids(&ids)?;
                for id in ids {
                    let n = find_note(id)?;
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
                    for e in &mut style.tracks[n.track] {
                        if e.id == n.id {
                            e.tick = start;
                            e.data[1] = pitch;
                            e.data[2] = velocity;
                        }
                        if e.id == n.off_id {
                            e.tick = start + duration;
                            e.data[1] = pitch;
                        }
                    }
                    touched.insert(n.track);
                }
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
            Command::Quantize { section, ids, step } => {
                check_ids(&ids)?;
                let s = bounds(section)?;
                if step == 0 || step > s.end - s.start {
                    return Err(reject("invalid quantize step"));
                }
                for id in ids {
                    let n = find_note(id)?;
                    let local = n
                        .start
                        .checked_sub(s.start)
                        .ok_or_else(|| reject("note outside section"))?;
                    let start =
                        s.start + ((local + step / 2) / step * step).min(s.end - s.start - 1);
                    let duration = n.duration.min(s.end - start);
                    checked(start, duration, n.pitch, n.velocity, section)?;
                    for e in &mut style.tracks[n.track] {
                        if e.id == n.id {
                            e.tick = start;
                        }
                        if e.id == n.off_id {
                            e.tick = start + duration;
                        }
                    }
                    touched.insert(n.track);
                }
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
