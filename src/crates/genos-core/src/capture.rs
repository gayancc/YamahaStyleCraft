//! Raw MIDI capture with markers. Not Style/CASM analyze-build (F027 remains separate).
use crate::{
    Error, Result,
    edit::NoteInput,
    midi::{self, MonitorEvent},
    smf::{self, Chunk, Event},
};
use midir::MidiInput;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

const MAX_EVENTS: usize = 50_000;
const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureMarker {
    pub stamp_ms: u64,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSnapshot {
    pub recording: bool,
    pub port_name: Option<String>,
    pub events: Vec<MonitorEvent>,
    pub markers: Vec<CaptureMarker>,
    pub event_count: usize,
    pub dropped_events: usize,
}

pub struct MidiCapture {
    connection: Option<midir::MidiInputConnection<()>>,
    events: Arc<Mutex<Vec<MonitorEvent>>>,
    markers: Arc<Mutex<Vec<CaptureMarker>>>,
    started: Option<Instant>,
    port_name: Option<String>,
    dropped: Arc<AtomicUsize>,
    data_bytes: Arc<AtomicUsize>,
}

impl Default for MidiCapture {
    fn default() -> Self {
        Self {
            connection: None,
            events: Arc::new(Mutex::new(Vec::new())),
            markers: Arc::new(Mutex::new(Vec::new())),
            started: None,
            port_name: None,
            dropped: Arc::new(AtomicUsize::new(0)),
            data_bytes: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl MidiCapture {
    pub fn is_recording(&self) -> bool {
        self.connection.is_some()
    }
    pub fn start(&mut self, name: &str) -> Result<()> {
        if self.connection.is_some() {
            return Err(Error::Midi(
                "Stop capture before starting a new take".into(),
            ));
        }
        let mut input = MidiInput::new("GenosStyleStudio capture").map_err(midi::map_err)?;
        input.ignore(midir::Ignore::None);
        let matches: Vec<_> = input
            .ports()
            .into_iter()
            .filter(|p| input.port_name(p).is_ok_and(|n| n == name))
            .collect();
        if matches.len() != 1 {
            return Err(Error::Midi(
                "exactly one matching MIDI input is required".into(),
            ));
        }
        let events = Arc::new(Mutex::new(Vec::new()));
        let callback_events = events.clone();
        let dropped = Arc::new(AtomicUsize::new(0));
        let callback_dropped = dropped.clone();
        let data_bytes = Arc::new(AtomicUsize::new(0));
        let callback_bytes = data_bytes.clone();
        let started = Instant::now();
        let connection = input
            .connect(
                &matches[0],
                "GenosStyleStudio capture",
                move |_t, data, _| {
                    // Skip active sensing / clock for musical capture log size.
                    if matches!(data.first(), Some(0xf8 | 0xfe)) {
                        return;
                    }
                    let mut q = callback_events.lock().unwrap_or_else(|p| p.into_inner());
                    if q.len() >= MAX_EVENTS
                        || data.len() > MAX_MESSAGE_BYTES
                        || callback_bytes
                            .load(Ordering::Relaxed)
                            .saturating_add(data.len())
                            > MAX_CAPTURE_BYTES
                    {
                        callback_dropped.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    callback_bytes.fetch_add(data.len(), Ordering::Relaxed);
                    q.push(MonitorEvent {
                        stamp_ms: started.elapsed().as_millis() as u64,
                        data: data.to_vec(),
                        summary: midi::summarize_public(data),
                    });
                },
                (),
            )
            .map_err(midi::map_err)?;
        self.connection = Some(connection);
        self.events = events;
        self.dropped = dropped;
        self.data_bytes = data_bytes;
        self.markers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        self.port_name = Some(name.into());
        self.started = Some(started);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stop_connection_only();
    }

    fn stop_connection_only(&mut self) {
        self.connection = None;
        self.started = None;
    }

    pub fn add_marker(&self, label: &str) -> Result<CaptureMarker> {
        let started = self
            .started
            .ok_or_else(|| Error::Midi("start capture before adding a marker".into()))?;
        let label = label.trim();
        if label.is_empty() || label.chars().count() > 64 {
            return Err(Error::Midi("marker label must be 1..64 characters".into()));
        }
        let marker = CaptureMarker {
            stamp_ms: started.elapsed().as_millis() as u64,
            label: label.into(),
        };
        self.markers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(marker.clone());
        Ok(marker)
    }

    pub fn clear(&self) -> Result<()> {
        if self.connection.is_some() {
            return Err(Error::Midi("Stop capture before clearing the take".into()));
        }
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        self.dropped.store(0, Ordering::Relaxed);
        self.data_bytes.store(0, Ordering::Relaxed);
        self.markers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        Ok(())
    }

    pub fn snapshot(&self) -> CaptureSnapshot {
        self.snapshot_tail(usize::MAX)
    }
    pub fn snapshot_tail(&self, limit: usize) -> CaptureSnapshot {
        let retained = self.events.lock().unwrap_or_else(|p| p.into_inner());
        let event_count = retained.len();
        let events = retained[retained.len().saturating_sub(limit)..].to_vec();
        drop(retained);
        let markers = self
            .markers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        CaptureSnapshot {
            recording: self.connection.is_some(),
            port_name: self.port_name.clone(),
            event_count,
            events,
            markers,
            dropped_events: self.dropped.load(Ordering::Relaxed),
        }
    }

    /// Encode captured channel messages + markers as a Type-0 SMF (PPQ 480).
    /// Driver callback stamps → ticks at 120 BPM; not wire-timing qualified.
    pub fn to_smf_bytes(&self) -> Result<Vec<u8>> {
        self.require_complete_take()?;
        let events = self
            .events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let markers = self
            .markers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let ppq: u16 = 480;
        let ms_to_tick = |ms: u64| -> Result<u64> {
            // 120 BPM → 2 beats/sec → 960 ticks/sec at PPQ 480
            u64::try_from(ms as u128 * 960 / 1000)
                .ok()
                .filter(|t| *t <= smf::MAX_TICK)
                .ok_or_else(|| Error::Format("Capture exceeds supported MIDI duration".into()))
        };
        let mut track = Vec::new();
        let mut id = 1u64;
        track.push(Event {
            id,
            tick: 0,
            order: id,
            data: vec![0xff, 0x51, 0x03, 0x07, 0xa1, 0x20], // 500_000 µs = 120 BPM
        });
        id += 1;
        let mut timed: Vec<(u64, Vec<u8>)> = Vec::new();
        for m in &markers {
            let mut data = vec![0xff, 0x06];
            smf::write_vlq(m.label.len() as u64, &mut data)?;
            data.extend(m.label.as_bytes());
            timed.push((ms_to_tick(m.stamp_ms)?, data));
        }
        for e in &events {
            if e.data.is_empty() {
                return Err(Error::Format("Empty captured MIDI message".into()));
            }
            let data = if e.data[0] >= 0xf0 {
                let (status, payload) = if e.data[0] == 0xf0 {
                    (0xf0, &e.data[1..])
                } else {
                    (0xf7, e.data.as_slice())
                };
                let mut data = vec![status];
                smf::write_vlq(payload.len() as u64, &mut data)?;
                data.extend(payload);
                data
            } else {
                validate_message(&e.data)?;
                e.data.clone()
            };
            timed.push((ms_to_tick(e.stamp_ms)?, data));
        }
        timed.sort_by_key(|(t, _)| *t);
        for (tick, data) in timed {
            track.push(Event {
                id,
                tick,
                order: id,
                data,
            });
            id += 1;
        }
        let end_tick = track.last().map(|e| e.tick).unwrap_or(0);
        track.push(Event {
            id,
            tick: end_tick,
            order: id,
            data: vec![0xff, 0x2f, 0],
        });
        let chunks = [
            Chunk {
                tag: *b"MThd",
                data: vec![0, 0, 0, 1, (ppq >> 8) as u8, (ppq & 0xff) as u8],
            },
            Chunk {
                tag: *b"MTrk",
                data: smf::encode_track(&track)?,
            },
        ];
        smf::pack(&chunks)
    }

    /// Pair captured note ons/offs into `NoteInput`s placed at `section_start`.
    /// Timing uses document PPQ at `tempo_bpm` from capture stamps — not wire-qualified.
    /// Does not invent Style/CASM analyze-build (F027).
    pub fn to_note_inputs(
        &self,
        section_start: u64,
        section_end: u64,
        ppq: u16,
        tempo_bpm: f64,
        track: usize,
    ) -> Result<Vec<NoteInput>> {
        self.require_complete_take()?;
        if !tempo_bpm.is_finite() || !(20.0..=400.0).contains(&tempo_bpm) {
            return Err(Error::Edit("tempo must be 20..400 BPM".into()));
        }
        if section_end <= section_start || section_end > smf::MAX_TICK || ppq == 0 {
            return Err(Error::Edit("invalid section window".into()));
        }
        let ticks_per_ms = (ppq as f64) * tempo_bpm / 60_000.0;
        let events = self
            .events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let mut open: HashMap<(u8, u8), VecDeque<(u64, u8)>> = HashMap::new();
        let mut notes = Vec::new();
        for ev in &events {
            if ev.data.is_empty() {
                continue;
            }
            let st = ev.data[0];
            let kind = st & 0xf0;
            if !matches!(kind, 0x80 | 0x90) {
                continue;
            }
            validate_message(&ev.data)?;
            let ch = (st & 0x0f) + 1;
            let pitch = ev.data[1];
            let vel = ev.data[2];
            let delta = ((ev.stamp_ms as f64) * ticks_per_ms).round();
            if delta > smf::MAX_TICK as f64 {
                return Err(Error::Edit(
                    "Capture extends beyond the selected section".into(),
                ));
            }
            let tick = section_start
                .checked_add(delta as u64)
                .ok_or_else(|| Error::Edit("Capture tick overflow".into()))?;
            if kind == 0x90 && vel > 0 {
                open.entry((ch, pitch)).or_default().push_back((tick, vel));
            } else {
                let (start,on_vel)=open.get_mut(&(ch,pitch)).and_then(VecDeque::pop_front).ok_or_else(||Error::Edit("Capture contains unmatched note-offs; retain/export the raw take or record again".into()))?;
                let end = tick.max(start + 1);
                if start < section_start || start >= section_end || end > section_end {
                    return Err(Error::Edit(
                        "Capture extends beyond the selected section; choose a longer section"
                            .into(),
                    ));
                }
                notes.push(NoteInput {
                    track,
                    channel: ch,
                    pitch,
                    velocity: on_vel,
                    start,
                    duration: end - start,
                });
            }
        }
        if open.values().any(|q| !q.is_empty()) {
            return Err(Error::Edit(
                "Capture contains unclosed notes; finish the notes and record again".into(),
            ));
        }
        if notes.is_empty() {
            return Err(Error::Edit(
                "capture has no complete note on/off pairs to apply".into(),
            ));
        }
        Ok(notes)
    }
    fn require_complete_take(&self) -> Result<()> {
        if self.connection.is_some() {
            return Err(Error::Midi(
                "Stop capture before applying or exporting the take".into(),
            ));
        }
        if self.dropped.load(Ordering::Relaxed) > 0 {
            return Err(Error::Midi(
                "Capture overflowed; events were dropped. Record a shorter take".into(),
            ));
        }
        Ok(())
    }
}
fn validate_message(data: &[u8]) -> Result<()> {
    if data.is_empty()
        || !(0x80..=0xef).contains(&data[0])
        || data.len()
            != if matches!(data[0] >> 4, 0xc | 0xd) {
                2
            } else {
                3
            }
        || data[1..].iter().any(|v| *v > 127)
    {
        return Err(Error::Format("Invalid captured channel message".into()));
    }
    Ok(())
}

impl Drop for MidiCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn take(events: &[(u64, &[u8])]) -> MidiCapture {
        let cap = MidiCapture::default();
        *cap.events.lock().unwrap() = events
            .iter()
            .map(|(stamp, data)| MonitorEvent {
                stamp_ms: *stamp,
                data: data.to_vec(),
                summary: String::new(),
            })
            .collect();
        cap
    }

    #[test]
    fn apply_pairs_repeated_pitch_fifo_instead_of_overwriting_the_first_note() {
        let cap = take(&[
            (0, &[0x90, 60, 90]),
            (100, &[0x90, 60, 80]),
            (300, &[0x80, 60, 0]),
            (400, &[0x90, 60, 0]),
        ]);
        let notes = cap.to_note_inputs(100, 1000, 480, 120.0, 0).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(
            (notes[0].start, notes[0].duration, notes[0].velocity),
            (100, 288, 90)
        );
        assert_eq!(
            (notes[1].start, notes[1].duration, notes[1].velocity),
            (196, 288, 80)
        );
    }

    #[test]
    fn apply_rejects_out_of_window_and_incomplete_notes_without_clamping() {
        let valid = take(&[(0, &[0x90, 60, 90]), (100, &[0x80, 60, 0])]);
        assert!(valid.to_note_inputs(0, 480, 480, 120.0, 0).is_ok());
        for events in [
            vec![(600, &[0x90, 60, 90][..]), (800, &[0x80, 60, 0][..])],
            vec![(0, &[0x90, 60, 90][..]), (800, &[0x80, 60, 0][..])],
            vec![(0, &[0x90, 60, 90][..])],
            vec![(0, &[0x80, 60, 0][..])],
        ] {
            assert!(take(&events).to_note_inputs(0, 480, 480, 120.0, 0).is_err());
        }
        assert!(valid.to_note_inputs(0, 480, 0, 120.0, 0).is_err());
        assert!(valid.to_note_inputs(0, 480, 480, f64::NAN, 0).is_err());
    }

    #[test]
    fn malformed_notes_and_timestamp_overflow_reject() {
        for data in [
            &[0x90, 60][..],
            &[0x90, 128, 90][..],
            &[0x90, 60, 90, 1][..],
        ] {
            let cap = take(&[(0, data), (100, &[0x80, 60, 0])]);
            assert!(cap.to_note_inputs(0, 480, 480, 120.0, 0).is_err());
            assert!(cap.to_smf_bytes().is_err());
        }
        let cap = take(&[(u64::MAX, &[0x90, 60, 90]), (u64::MAX, &[0x80, 60, 0])]);
        assert!(cap.to_smf_bytes().is_err());
        assert!(cap.to_note_inputs(0, 480, 480, 120.0, 0).is_err());
    }

    #[test]
    fn capture_keeps_sysex_and_system_messages_in_midi_export() {
        let cap = take(&[(0, &[0xf0, 0x43, 0x12, 0x34, 0xf7]), (10, &[0xf1, 7])]);
        let style = smf::Style::parse(&cap.to_smf_bytes().unwrap()).unwrap();
        assert!(
            style.tracks[0]
                .iter()
                .any(|e| e.data == [0xf0, 4, 0x43, 0x12, 0x34, 0xf7])
        );
        assert!(style.tracks[0].iter().any(|e| e.data == [0xf7, 2, 0xf1, 7]));
    }

    #[test]
    fn overflow_is_visible_and_prevents_silent_partial_apply_or_export() {
        let cap = take(&[(0, &[0x90, 60, 90]), (100, &[0x80, 60, 0])]);
        cap.dropped.store(1, Ordering::Relaxed);
        assert_eq!(cap.snapshot().dropped_events, 1);
        assert!(cap.to_note_inputs(0, 480, 480, 120.0, 0).is_err());
        assert!(cap.to_smf_bytes().is_err());
        cap.clear().unwrap();
        assert_eq!(cap.snapshot().dropped_events, 0);
        assert_eq!(cap.snapshot().event_count, 0);
    }

    #[test]
    fn invalid_restart_preserves_previous_capture_and_stop_retains_source_name() {
        let mut cap = take(&[(0, &[0x90, 60, 90]), (100, &[0x80, 60, 0])]);
        cap.port_name = Some("previous input".into());
        assert!(
            cap.start("GenosStyleStudio deliberately missing test port")
                .is_err()
        );
        assert_eq!(cap.snapshot().event_count, 2);
        assert_eq!(cap.snapshot().port_name.as_deref(), Some("previous input"));
        cap.stop();
        assert_eq!(cap.snapshot().port_name.as_deref(), Some("previous input"));
    }

    #[test]
    fn display_snapshot_is_bounded_without_losing_the_recorded_take() {
        let cap = take(
            &(0..500)
                .map(|i| (i, &[0x90, 60, 90][..]))
                .collect::<Vec<_>>(),
        );
        let tail = cap.snapshot_tail(128);
        assert_eq!(tail.event_count, 500);
        assert_eq!(tail.events.len(), 128);
        assert_eq!(tail.events[0].stamp_ms, 372);
        assert_eq!(cap.snapshot().events.len(), 500);
    }

    #[test]
    fn unicode_markers_use_a_real_midi_length_prefix_and_zero_length_playback_rejects() {
        let mut cap = MidiCapture::default();
        cap.started = Some(Instant::now());
        let label = "🎹".repeat(64);
        assert!(cap.add_marker(&label).is_ok());
        cap.stop();
        let style = smf::Style::parse(&cap.to_smf_bytes().unwrap()).unwrap();
        assert!(
            style.tracks[0]
                .iter()
                .filter_map(|e| e.meta(6))
                .any(|b| b == label.as_bytes())
        );
        let events = vec![
            Event {
                id: 1,
                tick: 0,
                order: 1,
                data: vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A'],
            },
            Event {
                id: 2,
                tick: 0,
                order: 2,
                data: vec![0xff, 0x2f, 0],
            },
        ];
        let bytes = smf::pack(&[
            Chunk {
                tag: *b"MThd",
                data: vec![0, 0, 0, 1, 1, 224],
            },
            Chunk {
                tag: *b"MTrk",
                data: smf::encode_track(&events).unwrap(),
            },
        ])
        .unwrap();
        let document = crate::document::Document::parse(&bytes).unwrap();
        assert!(crate::midi::plan(&document, document.sections[0].id, &[1]).is_err());
    }

    #[test]
    fn smf_export_includes_note_and_marker() {
        let cap = MidiCapture::default();
        {
            let mut ev = cap.events.lock().unwrap();
            ev.push(MonitorEvent {
                stamp_ms: 0,
                data: vec![0x90, 60, 100],
                summary: "Note On".into(),
            });
            ev.push(MonitorEvent {
                stamp_ms: 500,
                data: vec![0x80, 60, 0],
                summary: "Note Off".into(),
            });
        }
        {
            let mut mk = cap.markers.lock().unwrap();
            mk.push(CaptureMarker {
                stamp_ms: 250,
                label: "A".into(),
            });
        }
        let bytes = cap.to_smf_bytes().unwrap();
        assert!(bytes.starts_with(b"MThd"));
        assert!(bytes.windows(4).any(|w| w == b"MTrk"));
        assert!(bytes.windows(3).any(|w| *w == [0x90, 60, 100]));
        assert!(bytes.windows(1).any(|w| w == b"A"));
    }

    #[test]
    fn marker_requires_active_capture() {
        let cap = MidiCapture::default();
        assert!(cap.add_marker("x").is_err());
    }
}
