//! Native source-note transport. No CASM chord transformation or synthesis.
use crate::{Error, Result, document::Document};
use midir::{MidiInput, MidiOutput};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Serialize)]
pub struct Port {
    pub index: usize,
    pub name: String,
}
#[derive(Serialize)]
pub struct Devices {
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
}
pub fn devices() -> Result<Devices> {
    let input = MidiInput::new("GenosStyleStudio scan").map_err(err)?;
    let output = MidiOutput::new("GenosStyleStudio scan").map_err(err)?;
    let inputs = input
        .ports()
        .iter()
        .enumerate()
        .map(|(index, p)| input.port_name(p).map(|name| Port { index, name }))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    let outputs = output
        .ports()
        .iter()
        .enumerate()
        .map(|(index, p)| output.port_name(p).map(|name| Port { index, name }))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(Devices { inputs, outputs })
}
fn err(e: impl std::fmt::Display) -> Error {
    Error::Midi(e.to_string())
}
pub(crate) fn map_err(e: impl std::fmt::Display) -> Error {
    err(e)
}
#[derive(Clone, Debug)]
pub struct Packet {
    pub at: Duration,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub setup: Vec<Vec<u8>>,
    pub packets: Vec<Packet>,
    pub duration: Duration,
}
pub fn plan(doc: &Document, section: u64, channels: &[u8]) -> Result<Plan> {
    if channels.iter().any(|x| !(1..=16).contains(x)) {
        return Err(Error::Midi("invalid enabled channel".into()));
    }
    let s = doc
        .sections
        .iter()
        .find(|s| s.id == section)
        .ok_or_else(|| Error::Midi("section not found".into()))?;
    if s.end <= s.start {
        return Err(Error::Midi(
            "Selected section has no playback duration".into(),
        ));
    }
    let mut events: Vec<_> = doc.style.tracks.iter().flatten().collect();
    events.sort_by_key(|e| (e.tick, e.order));
    let mut tempo = 500_000u64;
    let mut prev = 0;
    let mut elapsed = 0f64;
    let mut times = vec![(0u64, 0f64, tempo)];
    for e in &events {
        if let Some(t) = e.meta(0x51).filter(|t| t.len() == 3) {
            elapsed += (e.tick - prev) as f64 * tempo as f64 / doc.style.ppq as f64;
            prev = e.tick;
            let value = ((t[0] as u64) << 16) | ((t[1] as u64) << 8) | t[2] as u64;
            if value > 0 {
                tempo = value;
                times.push((e.tick, elapsed, tempo));
            }
        }
    }
    // Integrate arbitrary tick from the tempo map rather than using UI BPM.
    let at = |tick: u64| -> Duration {
        let i = times
            .partition_point(|(t, _, _)| *t <= tick)
            .saturating_sub(1);
        let (previous, total, tempo) = times[i];
        Duration::from_secs_f64(
            (total + (tick - previous) as f64 * tempo as f64 / doc.style.ppq as f64) / 1_000_000.0,
        )
    };
    let start = at(s.start);
    let mut baseline = BTreeMap::new();
    for e in &events {
        if e.tick >= s.start {
            break;
        }
        if let Some(ch) = e.channel().filter(|ch| channels.contains(ch)) {
            match e.data[0] >> 4 {
                0xb => {
                    baseline.insert((ch, 0xb, e.data[1]), e.data.clone());
                }
                0xc | 0xe => {
                    baseline.insert((ch, e.data[0] >> 4, 0), e.data.clone());
                }
                _ => {}
            }
        }
    }
    let mut packets: Vec<_> = baseline
        .into_values()
        .map(|data| Packet {
            at: Duration::ZERO,
            data,
        })
        .collect();
    let paired: HashSet<_> = doc.notes.iter().flat_map(|n| [n.id, n.off_id]).collect();
    // Only complete paired notes are played, including notes held over start.
    for n in &doc.notes {
        if !channels.contains(&n.channel) || n.start >= s.end || n.start + n.duration <= s.start {
            continue;
        }
        let on = Packet {
            at: at(n.start.max(s.start)) - start,
            data: vec![0x90 | (n.channel - 1), n.pitch, n.velocity],
        };
        let off = Packet {
            at: at((n.start + n.duration).min(s.end)) - start,
            data: vec![0x80 | (n.channel - 1), n.pitch, 0],
        };
        packets.push(on);
        packets.push(off);
    }
    for e in events {
        if e.tick >= s.start
            && e.tick < s.end
            && e.channel().is_some_and(|ch| channels.contains(&ch))
            && !paired.contains(&e.id)
            && !e.is_on()
            && !e.is_off()
        {
            packets.push(Packet {
                at: at(e.tick) - start,
                data: e.data.clone(),
            });
        }
    }
    // Stable order, with paired note-offs before subsequent onsets at a tie.
    packets.sort_by_key(|p| {
        (
            p.at,
            if p.data[0] & 0xf0 == 0x80 {
                0
            } else if p.data[0] & 0xf0 == 0x90 {
                2
            } else {
                1
            },
        )
    });
    Ok(Plan {
        setup: Vec::new(),
        packets,
        duration: at(s.end) - start,
    })
}
/// Optional typed initialization is separate from the timed, looping note plan.
pub fn plan_with_initial_effects(
    doc: &Document,
    section: u64,
    channels: &[u8],
    initial_effects: bool,
) -> Result<Plan> {
    let mut result = plan(doc, section, channels)?;
    if initial_effects {
        if channels.is_empty() {
            return Err(Error::Midi(
                "Enable a source channel before previewing initial effects".into(),
            ));
        }
        result.setup = crate::style_effects::preview_setup(doc)?.packets;
    }
    Ok(result)
}
pub struct PlaybackOptions {
    pub section: u64,
    pub channels: Vec<u8>,
    pub index: usize,
    pub name: String,
    pub looping: bool,
    pub initial_effects: bool,
}
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub playing: bool,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    #[serde(skip)]
    started: Option<Instant>,
}
pub struct Transport {
    worker: Option<JoinHandle<()>>,
    cancel: Arc<(Mutex<bool>, Condvar)>,
    status: Arc<Mutex<Status>>,
}
impl Default for Transport {
    fn default() -> Self {
        Self {
            worker: None,
            cancel: Arc::new((Mutex::new(false), Condvar::new())),
            status: Arc::new(Mutex::new(Status::default())),
        }
    }
}
impl Transport {
    pub fn status(&self) -> Status {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if status.playing
            && let Some(start) = status.started
        {
            status.elapsed_ms = start.elapsed().as_millis() as u64;
        }
        status
    }
    pub fn stop(&mut self) {
        let (lock, cv) = &*self.cancel;
        *lock.lock().unwrap_or_else(|p| p.into_inner()) = true;
        cv.notify_all();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
    pub fn play(
        &mut self,
        doc: &Document,
        section: u64,
        channels: Vec<u8>,
        index: usize,
        name: String,
        looping: bool,
    ) -> Result<()> {
        self.play_with_options(
            doc,
            PlaybackOptions {
                section,
                channels,
                index,
                name,
                looping,
                initial_effects: false,
            },
        )
    }
    pub fn play_with_options(&mut self, doc: &Document, options: PlaybackOptions) -> Result<()> {
        let PlaybackOptions {
            section,
            channels,
            index,
            name,
            looping,
            initial_effects,
        } = options;
        let plan = plan_with_initial_effects(doc, section, &channels, initial_effects)?;
        self.play_plan(plan, channels, index, name, looping)
    }
    pub fn play_plan(
        &mut self,
        plan: Plan,
        channels: Vec<u8>,
        index: usize,
        name: String,
        looping: bool,
    ) -> Result<()> {
        if channels.is_empty()
            || channels.iter().any(|c| !(1..=16).contains(c))
            || plan.duration.is_zero()
        {
            return Err(Error::Midi(
                "Invalid prepared playback route or duration".into(),
            ));
        }
        self.stop();
        let output = MidiOutput::new("GenosStyleStudio playback").map_err(err)?;
        let ports = output.ports();
        let port = ports
            .get(index)
            .ok_or_else(|| Error::Midi("output disconnected; refresh devices".into()))?;
        if output.port_name(port).map_err(err)? != name {
            return Err(Error::Midi(
                "device list changed; select the output again".into(),
            ));
        }
        let mut connection = output
            .connect(port, "GenosStyleStudio source playback")
            .map_err(err)?;
        self.cancel = Arc::new((Mutex::new(false), Condvar::new()));
        let cancel = self.cancel.clone();
        let status = self.status.clone();
        *status.lock().unwrap() = Status {
            playing: true,
            ..Default::default()
        };
        self.worker = Some(thread::spawn(move || {
            let failure = run_plan(&plan, &channels, looping, &cancel, &status, |data| {
                connection.send(data).map_err(|e| e.to_string())
            });
            let mut st = status.lock().unwrap_or_else(|p| p.into_inner());
            st.playing = false;
            if let Some(start) = st.started.take() {
                st.elapsed_ms = start.elapsed().as_millis() as u64;
            }
            st.error = failure;
        }));
        Ok(())
    }
}
fn run_plan(
    plan: &Plan,
    channels: &[u8],
    looping: bool,
    cancel: &(Mutex<bool>, Condvar),
    status: &Mutex<Status>,
    mut send: impl FnMut(&[u8]) -> std::result::Result<(), String>,
) -> Option<String> {
    let mut failure = None;
    'play: {
        // Send once, before any timed channel setup or notes, never each loop.
        for data in &plan.setup {
            if wait_until(cancel, Instant::now()) {
                break 'play;
            }
            if let Err(e) = send(data) {
                failure = Some(e);
                break 'play;
            }
        }
        loop {
            let start = Instant::now();
            status.lock().unwrap_or_else(|p| p.into_inner()).started = Some(start);
            for packet in &plan.packets {
                if wait_until(cancel, start + packet.at) {
                    break 'play;
                }
                if let Err(e) = send(&packet.data) {
                    failure = Some(e);
                    break 'play;
                }
                status.lock().unwrap_or_else(|p| p.into_inner()).elapsed_ms =
                    packet.at.as_millis() as u64;
            }
            if wait_until(cancel, start + plan.duration) || !looping {
                break;
            }
        }
    }
    // Release selected routes even if startup fails or is cancelled.
    for ch in channels {
        let s = ch - 1;
        for data in [[0xb0 | s, 64, 0], [0xb0 | s, 123, 0], [0xb0 | s, 120, 0]] {
            if let Err(e) = send(&data) {
                failure.get_or_insert(e);
            }
        }
        for pitch in 0..128 {
            if let Err(e) = send(&[0x80 | s, pitch, 0]) {
                failure.get_or_insert(e);
            }
        }
    }
    failure
}
fn wait_until(cancel: &(Mutex<bool>, Condvar), deadline: Instant) -> bool {
    let (lock, cv) = cancel;
    let mut flag = lock.lock().unwrap_or_else(|p| p.into_inner());
    while !*flag {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let (f, _) = cv
            .wait_timeout(flag, remaining)
            .unwrap_or_else(|p| p.into_inner());
        flag = f;
    }
    *flag
}
impl Drop for Transport {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Open exact output briefly and send sustain-off / all-notes / all-sound / note-offs.
pub fn panic_release(index: usize, name: &str, channels: &[u8]) -> Result<()> {
    if channels.iter().any(|x| !(1..=16).contains(x)) {
        return Err(Error::Midi("invalid channel for release".into()));
    }
    let output = MidiOutput::new("GenosStyleStudio panic").map_err(err)?;
    let ports = output.ports();
    let port = ports
        .get(index)
        .ok_or_else(|| Error::Midi("output disconnected; refresh devices".into()))?;
    if output.port_name(port).map_err(err)? != name {
        return Err(Error::Midi(
            "device list changed; select the output again".into(),
        ));
    }
    let mut connection = output
        .connect(port, "GenosStyleStudio panic release")
        .map_err(err)?;
    for ch in channels {
        let s = ch - 1;
        for data in [[0xb0 | s, 64, 0], [0xb0 | s, 123, 0], [0xb0 | s, 120, 0]] {
            connection.send(&data).map_err(err)?;
        }
        for pitch in 0u8..128 {
            connection.send(&[0x80 | s, pitch, 0]).map_err(err)?;
        }
    }
    Ok(())
}

/// Audition a single note on an exact output (FEAT-025 test-note).
pub fn test_note(
    index: usize,
    name: &str,
    channel: u8,
    pitch: u8,
    velocity: u8,
    hold_ms: u64,
) -> Result<()> {
    if !(1..=16).contains(&channel) {
        return Err(Error::Midi("channel must be 1..16".into()));
    }
    if pitch > 127 || velocity == 0 || velocity > 127 {
        return Err(Error::Midi("pitch/velocity out of range".into()));
    }
    if !(20..=2000).contains(&hold_ms) {
        return Err(Error::Midi("hold_ms must be 20..2000".into()));
    }
    let output = MidiOutput::new("GenosStyleStudio audition").map_err(err)?;
    let ports = output.ports();
    let port = ports
        .get(index)
        .ok_or_else(|| Error::Midi("output disconnected; refresh devices".into()))?;
    if output.port_name(port).map_err(err)? != name {
        return Err(Error::Midi(
            "device list changed; select the output again".into(),
        ));
    }
    let mut connection = output
        .connect(port, "GenosStyleStudio test note")
        .map_err(err)?;
    let ch = channel - 1;
    connection
        .send(&[0x90 | ch, pitch, velocity])
        .map_err(err)?;
    thread::sleep(Duration::from_millis(hold_ms));
    connection.send(&[0x80 | ch, pitch, 0]).map_err(err)?;
    Ok(())
}

/// Bounded Genos drum-note audition (channels 9–10, notes 13–91 per shell contract).
/// Does not apply Drum Setup SysEx, remaps, or kit initialization.
pub fn preview_drum_note(
    index: usize,
    name: &str,
    channel: u8,
    pitch: u8,
    velocity: u8,
    hold_ms: u64,
) -> Result<()> {
    if !crate::voice_catalog::DRUM_CHANNELS.contains(&channel) {
        return Err(Error::Midi(
            "drum preview is limited to MIDI channels 9 and 10".into(),
        ));
    }
    if !(crate::voice_catalog::DRUM_PREVIEW_NOTE_MIN..=crate::voice_catalog::DRUM_PREVIEW_NOTE_MAX)
        .contains(&pitch)
    {
        return Err(Error::Midi(
            "drum preview notes are limited to 13..91 for Genos shell bounds".into(),
        ));
    }
    test_note(index, name, channel, pitch, velocity, hold_ms)
}

/// Display-only MIDI input monitor. Not a recorder and not timing-qualified.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorEvent {
    pub stamp_ms: u64,
    pub data: Vec<u8>,
    pub summary: String,
}
pub(crate) fn summarize_public(data: &[u8]) -> String {
    summarize(data)
}
fn summarize(data: &[u8]) -> String {
    if data.is_empty() {
        return "empty".into();
    }
    let st = data[0];
    match st & 0xf0 {
        0x90 if data.len() >= 3 && data[2] > 0 => {
            format!("Note On ch{} n{} v{}", (st & 15) + 1, data[1], data[2])
        }
        0x80 | 0x90 if data.len() >= 2 => {
            format!("Note Off ch{} n{}", (st & 15) + 1, data[1])
        }
        0xb0 if data.len() >= 3 => {
            format!("CC ch{} #{} ={}", (st & 15) + 1, data[1], data[2])
        }
        0xc0 if data.len() >= 2 => format!("Program ch{} ={}", (st & 15) + 1, data[1]),
        _ => format!("{:02X?}", data),
    }
}
pub struct InputMonitor {
    _connection: Option<midir::MidiInputConnection<()>>,
    events: Arc<Mutex<Vec<MonitorEvent>>>,
    port_name: Option<String>,
}
impl Default for InputMonitor {
    fn default() -> Self {
        Self {
            _connection: None,
            events: Arc::new(Mutex::new(Vec::new())),
            port_name: None,
        }
    }
}
impl InputMonitor {
    pub fn is_active(&self) -> bool {
        self._connection.is_some()
    }
    pub fn start(&mut self, name: &str) -> Result<()> {
        if self._connection.is_some() {
            return Err(Error::Midi(
                "Stop the current monitor before starting another".into(),
            ));
        }
        let mut input = MidiInput::new("GenosStyleStudio input monitor").map_err(err)?;
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
        let started = Instant::now();
        let connection = input
            .connect(
                &matches[0],
                "GenosStyleStudio monitor",
                move |_t, data, _| {
                    let mut q = callback_events.lock().unwrap_or_else(|p| p.into_inner());
                    let truncated = data.len() > 1024;
                    let data = &data[..data.len().min(1024)];
                    q.push(MonitorEvent {
                        stamp_ms: started.elapsed().as_millis() as u64,
                        data: data.to_vec(),
                        summary: format!(
                            "{}{}",
                            summarize(data),
                            if truncated {
                                " … display truncated"
                            } else {
                                ""
                            }
                        ),
                    });
                    if q.len() > 200 {
                        let n = q.len() - 200;
                        q.drain(0..n);
                    }
                },
                (),
            )
            .map_err(err)?;
        self._connection = Some(connection);
        self.events = events;
        self.port_name = Some(name.into());
        Ok(())
    }
    pub fn stop(&mut self) {
        self._connection = None;
        self.port_name = None;
        self.clear();
    }
    pub fn snapshot(&self) -> (Option<String>, Vec<MonitorEvent>) {
        (
            self.port_name.clone(),
            self.events
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
        )
    }
    pub fn clear(&self) {
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }
}
impl Drop for InputMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod monitor_tests {
    use super::summarize;

    #[test]
    fn summarize_note_cc_and_program() {
        assert_eq!(summarize(&[0x90, 60, 100]), "Note On ch1 n60 v100");
        assert_eq!(summarize(&[0x80, 60, 0]), "Note Off ch1 n60");
        assert_eq!(summarize(&[0x90, 64, 0]), "Note Off ch1 n64");
        assert_eq!(summarize(&[0xb0, 7, 90]), "CC ch1 #7 =90");
        assert_eq!(summarize(&[0xc2, 12]), "Program ch3 =12");
        assert_eq!(summarize(&[]), "empty");
    }
}
#[cfg(test)]
mod playback_tests {
    use super::*;
    fn test_plan() -> Plan {
        Plan {
            setup: vec![vec![0xf0, 0x43, 0x10, 0x4c, 2, 1, 12, 64, 0xf7]],
            packets: vec![
                Packet {
                    at: Duration::ZERO,
                    data: vec![0xb0, 91, 127],
                },
                Packet {
                    at: Duration::ZERO,
                    data: vec![0x90, 60, 48],
                },
            ],
            duration: Duration::ZERO,
        }
    }
    #[test]
    fn startup_is_once_before_channel_packets_across_loops_and_stop_releases() {
        let cancel = (Mutex::new(false), Condvar::new());
        let status = Mutex::new(Status::default());
        let mut sent = Vec::new();
        let mut ons = 0;
        let failure = run_plan(&test_plan(), &[1], true, &cancel, &status, |data| {
            sent.push(data.to_vec());
            if data[0] == 0x90 {
                ons += 1;
                if ons == 2 {
                    *cancel.0.lock().unwrap() = true;
                }
            }
            Ok(())
        });
        assert_eq!(failure, None);
        assert_eq!(ons, 2);
        assert_eq!(sent.iter().filter(|d| d[0] == 0xf0).count(), 1);
        assert_eq!(
            &sent[..3],
            &[
                test_plan().setup[0].clone(),
                vec![0xb0, 91, 127],
                vec![0x90, 60, 48]
            ]
        );
        assert!(sent.contains(&vec![0xb0, 64, 0]));
        assert!(sent.contains(&vec![0xb0, 120, 0]));
        assert_eq!(sent.iter().filter(|d| d[0] == 0x80).count(), 128);
    }
    #[test]
    fn cancelled_or_failed_startup_never_sends_notes_and_still_attempts_release() {
        for cancelled in [false, true] {
            let cancel = (Mutex::new(cancelled), Condvar::new());
            let status = Mutex::new(Status::default());
            let mut sent = Vec::new();
            let failure = run_plan(&test_plan(), &[1], false, &cancel, &status, |data| {
                sent.push(data.to_vec());
                if data[0] == 0xf0 {
                    Err("setup failed".into())
                } else {
                    Ok(())
                }
            });
            assert_eq!(
                failure,
                if cancelled {
                    None
                } else {
                    Some("setup failed".into())
                }
            );
            assert!(!sent.iter().any(|d| d[0] == 0x90));
            assert!(sent.contains(&vec![0xb0, 123, 0]));
            assert_eq!(
                sent.iter().filter(|d| d[0] == 0xf0).count(),
                usize::from(!cancelled)
            );
        }
    }
}
