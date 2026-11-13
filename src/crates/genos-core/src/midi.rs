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

/// Attempts to open an input before giving up. Windows MIDI Services fails the first
/// `midiInOpen` on an idle Bluetooth endpoint (for example a CME WIDI) and then succeeds.
const INPUT_CONNECT_ATTEMPTS: u32 = 4;
const INPUT_CONNECT_BACKOFF: Duration = Duration::from_millis(120);

/// Whether a failed input connection is worth retrying. A port another program holds
/// (`MMSYSERR_ALLOCATED`) and an invalid port are reported at once.
fn input_connect_is_transient(kind: midir::ConnectErrorKind) -> bool {
    match kind {
        midir::ConnectErrorKind::InvalidPort => false,
        midir::ConnectErrorKind::Other(message) => !message.contains("ALLOCATED"),
    }
}

/// Opens an exact input port, retrying the transient Windows open failure.
/// `make_callback` builds a fresh callback for each attempt because midir consumes it.
pub fn connect_input<F>(
    mut input: MidiInput,
    port: &midir::MidiInputPort,
    label: &str,
    mut make_callback: impl FnMut() -> F,
) -> Result<midir::MidiInputConnection<()>>
where
    F: FnMut(u64, &[u8], &mut ()) + Send + 'static,
{
    let mut attempt = 1;
    loop {
        match input.connect(port, label, make_callback(), ()) {
            Ok(connection) => return Ok(connection),
            Err(e) if attempt < INPUT_CONNECT_ATTEMPTS && input_connect_is_transient(e.kind()) => {
                input = e.into_inner();
                thread::sleep(INPUT_CONNECT_BACKOFF * attempt);
                attempt += 1;
            }
            Err(e) if e.kind() == midir::ConnectErrorKind::InvalidPort => {
                return Err(Error::Midi(
                    "MIDI input disconnected; refresh devices".into(),
                ));
            }
            Err(e) if !input_connect_is_transient(e.kind()) => {
                return Err(Error::Midi(
                    "MIDI input is in use by another program; close it and try again".into(),
                ));
            }
            Err(e) => {
                return Err(Error::Midi(format!(
                    "{e} after {INPUT_CONNECT_ATTEMPTS} attempts; reconnect the device and try again"
                )));
            }
        }
    }
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
    /// Bar-line offsets from plan start, ending at the plan end.
    pub bars: Vec<Duration>,
    /// Beat offsets from plan start (last entry is the plan end); live chord changes land here.
    pub beats: Vec<Duration>,
    /// Style section this plan plays, so the UI can follow queued switches.
    pub section: Option<u64>,
    /// Tempo segments for converting playback deadlines to musical time.
    pub tempo: Option<TempoClock>,
    /// Output channel for each source channel: the section's CASM destination part.
    pub routes: ChannelRoutes,
}

/// Maps each source channel (index 0..16) to the 1-based channel the instrument part
/// receives, so a Style track plays on its CASM destination part as the Genos plays it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelRoutes([u8; 16]);

impl Default for ChannelRoutes {
    fn default() -> Self {
        Self(std::array::from_fn(|i| i as u8 + 1))
    }
}

impl ChannelRoutes {
    /// Destination channel (1..16) for a source channel (1..16).
    pub fn destination(&self, source: u8) -> u8 {
        self.0[usize::from(source.clamp(1, 16) - 1)]
    }

    fn set(&mut self, source: u8, destination: u8) {
        if (1..=16).contains(&source) && (1..=16).contains(&destination) {
            self.0[usize::from(source - 1)] = destination;
        }
    }

    /// Rewrites a channel message onto its destination channel; other messages pass through.
    fn apply<'a>(&self, data: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        match packet_channel(data) {
            Some(source) if self.destination(source) != source => {
                let mut routed = data.to_vec();
                routed[0] = (data[0] & 0xf0) | (self.destination(source) - 1);
                std::borrow::Cow::Owned(routed)
            }
            _ => std::borrow::Cow::Borrowed(data),
        }
    }
}

impl Plan {
    fn rhythm_sources(&self) -> u16 {
        self.tempo.as_ref().and_then(|clock| clock.rhythm_sources).unwrap_or(DEFAULT_RHYTHM_SOURCES)
    }

    fn finish_at(&mut self, end: Duration) {
        self.duration = end;
        self.packets.retain(|packet| packet.at <= end);
        self.bars.retain(|bar| *bar < end);
        self.bars.push(end);
        self.beats.retain(|beat| *beat < end);
        self.beats.push(end);
    }
}

#[derive(Clone, Debug)]
pub struct TempoClock {
    segments: Vec<(Duration, f64, u64)>,
    meter: Option<FillMeter>,
    fill: bool,
    held_seek_sources: u16,
    /// None uses the conventional rhythm source numbers for styles without CASM routes.
    rhythm_sources: Option<u16>,
}

#[derive(Clone, Debug)]
struct FillMeter {
    beat_quarters: f64,
    bar_quarters: f64,
    start_quarters: f64,
}

impl TempoClock {
    fn quarters_at(&self, elapsed: Duration) -> f64 {
        let index = self.segments.partition_point(|(at, _, _)| *at <= elapsed) - 1;
        let (at, quarters, tempo_us) = self.segments[index];
        quarters + (elapsed - at).as_secs_f64() * 1_000_000.0 / tempo_us as f64
    }

    fn elapsed_at(&self, quarters: f64) -> Duration {
        let index = self.segments.partition_point(|(_, q, _)| *q <= quarters) - 1;
        let (at, base, tempo_us) = self.segments[index];
        at + Duration::from_secs_f64((quarters - base) * tempo_us as f64 / 1_000_000.0)
    }

    fn in_root_window(&self, elapsed: Duration, base: f64, changed: f64) -> bool {
        let deadline = changed - base + 0.5;
        // Compare scheduled durations so floating-point subtraction cannot
        // leave the window open at the exact half-quarter boundary.
        deadline > 0.0
            && elapsed < self.elapsed_at(deadline)
            && base + self.quarters_at(elapsed) >= changed
    }
}
/// Build a one-shot playback plan from relative phrase notes (library preview).
/// `tempo_us` is microseconds per quarter note; `ppq` is pulses per quarter.
#[must_use = "plan is unused unless passed to Transport::play_plan"]
pub fn plan_from_phrases(
    phrases: &[(u8, &[crate::exchange::PhraseNote])],
    ppq: u16,
    tempo_us: u64,
    section_len: u64,
) -> Result<Plan> {
    if ppq == 0 || tempo_us == 0 {
        return Err(Error::Midi("invalid preview timing".into()));
    }
    if phrases.is_empty() || phrases.iter().any(|(ch, _)| !(1..=16).contains(ch)) {
        return Err(Error::Midi("invalid preview channels".into()));
    }
    let at = |tick: u64| -> Duration {
        Duration::from_secs_f64(tick as f64 * tempo_us as f64 / ppq as f64 / 1_000_000.0)
    };
    let mut packets = Vec::new();
    let mut end = section_len.max(1);
    for &(channel, notes) in phrases {
        for n in notes {
            if !(1..=127).contains(&n.pitch) || n.velocity == 0 || n.duration == 0 {
                continue;
            }
            packets.push(Packet {
                at: at(n.start),
                data: vec![0x90 | (channel - 1), n.pitch, n.velocity],
            });
            packets.push(Packet {
                at: at(n.start.saturating_add(n.duration)),
                data: vec![0x80 | (channel - 1), n.pitch, 0],
            });
            end = end.max(n.start.saturating_add(n.duration));
        }
    }
    if packets.is_empty() {
        return Err(Error::Midi("preview has no playable notes".into()));
    }
    packets.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.data.cmp(&b.data)));
    Ok(Plan {
        setup: vec![],
        packets,
        duration: at(end),
        bars: vec![at(end)],
        beats: vec![at(end)],
        section: None,
        tempo: None,
        routes: ChannelRoutes::default(),
    })
}

pub fn plan(doc: &Document, section: u64, channels: &[u8]) -> Result<Plan> {
    plan_from(doc, section, channels, None)
}

/// Build a section playback plan, optionally starting at `from_tick` (SFM yellow cursor).
pub fn plan_from(
    doc: &Document,
    section: u64,
    channels: &[u8],
    from_tick: Option<u64>,
) -> Result<Plan> {
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
    let play_from = from_tick
        .unwrap_or(s.start)
        .clamp(s.start, s.end.saturating_sub(1).max(s.start));
    if play_from >= s.end {
        return Err(Error::Midi(
            "Play cursor is at the end of the section".into(),
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
    let start = at(play_from);
    let tempo_at_start = times[times.partition_point(|(tick, _, _)| *tick <= play_from) - 1].2;
    let mut segments = vec![(Duration::ZERO, 0.0, tempo_at_start)];
    segments.extend(times.iter().filter(|(tick, _, _)| *tick > play_from && *tick < s.end)
        .map(|&(tick, _, tempo)| {
            (at(tick) - start, (tick - play_from) as f64 / f64::from(doc.style.ppq), tempo)
        }));
    let mut baseline = BTreeMap::new();
    for e in &events {
        if e.tick >= play_from {
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
    // Notes sounding at/after the play cursor, including notes held over it.
    for n in &doc.notes {
        if !channels.contains(&n.channel) || n.start >= s.end || n.start + n.duration <= play_from {
            continue;
        }
        let on = Packet {
            at: at(n.start.max(play_from)) - start,
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
        if e.tick >= play_from
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
    let (_, numerator, denominator) = doc.timing_at(s.start);
    let bar_ticks = (doc.style.ppq as u64 * 4 / denominator.max(1) as u64) * numerator as u64;
    let mut bars = Vec::new();
    if bar_ticks > 0 {
        let mut tick = s.start + bar_ticks;
        while tick < s.end {
            if tick > play_from {
                bars.push(at(tick) - start);
            }
            tick += bar_ticks;
        }
    }
    let beat_ticks = (doc.style.ppq as u64 * 4 / denominator.max(1) as u64).max(1);
    let mut beats = Vec::new();
    let mut tick = s.start + beat_ticks;
    while tick < s.end {
        if tick > play_from {
            beats.push(at(tick) - start);
        }
        tick += beat_ticks;
    }
    let duration = at(s.end) - start;
    bars.push(duration);
    beats.push(duration);
    let mut held_seek_sources = 0;
    let mut rhythm_sources = 0;
    let mut has_section_routes = doc.routes.iter().any(|route| route.sections.contains(&s.label));
    let mut channel_routes = ChannelRoutes::default();
    for source in 1..=16 {
        let resolved = doc.firmware_cseg_route(&s.label, source);
        let candidates = || doc.routes.iter()
            .filter(|route| route.source == source && route.sections.contains(&s.label));
        match resolved {
            Some(Some(route)) => channel_routes.set(source, route.destination),
            Some(None) => {}
            None => {
                let mut destinations = candidates().map(|route| route.destination);
                if let Some(first) = destinations.next()
                    && destinations.all(|destination| destination == first)
                {
                    channel_routes.set(source, first);
                }
            }
        }
        let mut routes = doc.routes.iter()
            .filter(|route| resolved.is_none() && route.source == source && route.sections.contains(&s.label));
        if let Some(first) = resolved.unwrap_or_else(|| routes.next()) {
            has_section_routes = true;
            let bit = channel_bit(source);
            let mut rhythm = matches!(first.destination, 9 | 10);
            let mut restores_held = (14..=16).contains(&first.destination);
            for route in routes {
                rhythm &= matches!(route.destination, 9 | 10);
                restores_held &= (14..=16).contains(&route.destination);
            }
            if rhythm {
                rhythm_sources |= bit;
            }
            if restores_held {
                held_seek_sources |= bit;
            }
        }
    }
    Ok(Plan {
        setup: Vec::new(),
        packets,
        duration,
        bars,
        beats,
        section: Some(section),
        routes: channel_routes,
        tempo: Some(TempoClock {
            segments,
            meter: Some(FillMeter {
                beat_quarters: beat_ticks as f64 / f64::from(doc.style.ppq),
                bar_quarters: bar_ticks as f64 / f64::from(doc.style.ppq),
                start_quarters: (play_from - s.start) as f64 / f64::from(doc.style.ppq),
            }),
            fill: matches!(
                s.label.split_whitespace().map(str::to_ascii_lowercase).collect::<Vec<_>>().join(" ").as_str(),
                "fill in aa" | "fill in bb" | "fill in cc" | "fill in dd" | "fill in ba"
                    | "fill a" | "fill b" | "fill c" | "fill d" | "break"
            ),
            held_seek_sources,
            rhythm_sources: has_section_routes.then_some(rhythm_sources),
        }),
    })
}
/// Optional typed initialization is separate from the timed, looping note plan.
pub fn plan_with_initial_effects(
    doc: &Document,
    section: u64,
    channels: &[u8],
    initial_effects: bool,
) -> Result<Plan> {
    plan_with_options(doc, section, channels, initial_effects, None)
}

fn plan_with_options(
    doc: &Document,
    section: u64,
    channels: &[u8],
    initial_effects: bool,
    from_tick: Option<u64>,
) -> Result<Plan> {
    let mut result = plan_from(doc, section, channels, from_tick)?;
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
    /// Absolute Style tick to start from (SFM play-from-yellow-cursor). `None` = section start.
    pub from_tick: Option<u64>,
}
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub playing: bool,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    /// Section currently sounding (follows live switches).
    pub section: Option<u64>,
    #[serde(skip)]
    started: Option<Instant>,
    #[serde(skip)]
    bars: Vec<Duration>,
    #[serde(skip)]
    beats: Vec<Duration>,
    #[serde(skip)]
    tempo: Option<TempoClock>,
    /// Destination part of each source in the sounding section.
    #[serde(skip)]
    routes: ChannelRoutes,
}
/// Chord played on the keyboard's left hand that the style should follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveChord {
    pub root: u8,
    pub quality: String,
    pub bass: Option<u8>,
    pub bass_quality: Option<String>,
}
/// Re-pitches one source note for a chord: (section, source channel 1..16, pitch, chord).
/// `None` silences the note (a CASM note mute).
pub type PitchMapper = Arc<dyn Fn(u64, u8, u8, &LiveChord) -> Option<u8> + Send + Sync>;
pub type TimedPitchMapper =
    Arc<dyn Fn(u64, u8, u8, &LiveChord, bool) -> Option<u8> + Send + Sync>;
pub type RoutedPitchMapper =
    Arc<dyn Fn(u64, u8, u8, &LiveChord, bool) -> LivePitch + Send + Sync>;

pub struct LivePitch {
    pub pitch: Option<u8>,
    pub portamento: bool,
    pub stop_on_change: bool,
    pub bass_region: bool,
    /// A chord-mask mute that sets CC84 and retains a sounding rhythm note.
    pub held_mask_portamento: bool,
}

impl From<Option<u8>> for LivePitch {
    fn from(pitch: Option<u8>) -> Self {
        Self {
            pitch,
            portamento: false,
            stop_on_change: false,
            bass_region: false,
            held_mask_portamento: false,
        }
    }
}

#[derive(Default)]
struct ChordState {
    current: Option<LiveChord>,
    initial: Option<LiveChord>,
    pending: Option<(Option<LiveChord>, Instant)>,
    mapper: Option<RoutedPitchMapper>,
    root_changed_quarter: Option<f64>,
}
// Default source assignment when the section has no CASM routes.
const DEFAULT_RHYTHM_SOURCES: u16 = (1 << 8) | (1 << 9);

fn channel_bit(channel: u8) -> u16 {
    1 << (channel - 1)
}
/// One leg of a queued live switch: a fill-in or intro plays once, a Main loops.
pub struct Step {
    pub plan: Plan,
    pub looping: bool,
}
struct Switch {
    at: Instant,
    steps: Vec<Step>,
    fill: Option<FillHandoff>,
    source_start: Instant,
}

struct FillHandoff {
    phase: f64,
    end: Duration,
    rhythm_only: bool,
}

fn fill_handoff(status: &Status, elapsed: Duration) -> Option<FillHandoff> {
    let tempo = status.tempo.as_ref()?;
    let meter = tempo.meter.as_ref()?;
    let end = *status.bars.iter().find(|bar| **bar > elapsed)?;
    if meter.bar_quarters <= 0.0 {
        return None;
    }
    let phase = (meter.start_quarters + tempo.quarters_at(elapsed)).rem_euclid(meter.bar_quarters);
    let half_beat = meter.beat_quarters / 2.0;
    (phase <= meter.bar_quarters - half_beat).then_some(FillHandoff {
        phase,
        end,
        rhythm_only: phase > half_beat,
    })
}
pub struct Transport {
    worker: Option<JoinHandle<()>>,
    cancel: Arc<(Mutex<bool>, Condvar)>,
    status: Arc<Mutex<Status>>,
    /// Live mute/solo filter (SFM M/S). Updated while playing without restarting the plan.
    audible: Arc<Mutex<Vec<u8>>>,
    /// Pending live section change.
    switch: Arc<Mutex<Option<Switch>>>,
    /// Left-hand chord the sounding style follows (live style play).
    chord: Arc<Mutex<ChordState>>,
}
impl Default for Transport {
    fn default() -> Self {
        Self {
            worker: None,
            cancel: Arc::new((Mutex::new(false), Condvar::new())),
            status: Arc::new(Mutex::new(Status::default())),
            audible: Arc::new(Mutex::new(Vec::new())),
            switch: Arc::new(Mutex::new(None)),
            chord: Arc::new(Mutex::new(ChordState::default())),
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
    /// Channels currently allowed through live mute/solo (copy).
    /// Output channels the given source channels currently sound on (their CASM
    /// destination parts in the playing section), for releasing muted sources.
    pub fn output_channels(&self, sources: &[u8]) -> Vec<u8> {
        let routes = self.status.lock().unwrap_or_else(|p| p.into_inner()).routes;
        let mut outputs: Vec<u8> = sources
            .iter()
            .filter(|ch| (1..=16).contains(*ch))
            .map(|&ch| routes.destination(ch))
            .collect();
        outputs.sort_unstable();
        outputs.dedup();
        outputs
    }
    pub fn audible_channels(&self) -> Vec<u8> {
        self.audible
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
    /// Update mute/solo filter during playback. Empty = silence all channel messages.
    pub fn set_audible_channels(&self, channels: Vec<u8>) -> Result<()> {
        if channels.iter().any(|c| !(1..=16).contains(c)) {
            return Err(Error::Midi("invalid audible channel".into()));
        }
        *self.audible.lock().unwrap_or_else(|p| p.into_inner()) = channels;
        Ok(())
    }
    /// Queue a live section change using the Fill timing window or next bar line.
    pub fn queue_switch(&self, mut steps: Vec<Step>) -> Result<()> {
        if steps.is_empty() || steps.iter().any(|s| s.plan.duration.is_zero()) {
            return Err(Error::Midi("Invalid live section switch".into()));
        }
        let (at, fill, source_start) = {
            let st = self.status.lock().unwrap_or_else(|p| p.into_inner());
            let Some(start) = st.started.filter(|_| st.playing) else {
                return Err(Error::Midi("Transport is not playing".into()));
            };
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(start);
            let is_fill = steps[0].plan.tempo.as_ref().is_some_and(|clock| clock.fill);
            let mut fill = if is_fill { fill_handoff(&st, elapsed) } else { None };
            if let Some(handoff) = &mut fill
                && steps[0].plan.section == st.section
                && steps.get(1).is_some_and(|next| next.looping && next.plan.tempo.is_some())
            {
                // Repressing the sounding Fill within its window selects its Main.
                steps.remove(0);
                handoff.rhythm_only = false;
            }
            let deadline_floor = if is_fill { elapsed } else { elapsed + Duration::from_millis(15) };
            let at = if fill.is_some() {
                now
            } else {
                st.bars
                .iter()
                .find(|b| **b > deadline_floor)
                .map_or(now, |b| start + *b)
            };
            (at, fill, start)
        };
        *self.switch.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(Switch { at, steps, fill, source_start });
        // Wake the player so it re-reads the switch deadline.
        let (lock, cv) = &*self.cancel;
        drop(lock.lock().unwrap_or_else(|p| p.into_inner()));
        cv.notify_all();
        Ok(())
    }
    /// Install the document's pitch rules used while following a chord.
    pub fn set_pitch_mapper(&self, mapper: PitchMapper) {
        self.set_timed_pitch_mapper(Arc::new(move |section, channel, pitch, chord, _| {
            mapper(section, channel, pitch, chord)
        }));
    }
    /// Install a mapper that also receives the scheduled half-quarter root-change flag.
    pub fn set_timed_pitch_mapper(&self, mapper: TimedPitchMapper) {
        self.set_routed_pitch_mapper(Arc::new(move |section, channel, pitch, chord, in_window| {
            mapper(section, channel, pitch, chord, in_window).into()
        }));
    }
    pub fn set_routed_pitch_mapper(&self, mapper: RoutedPitchMapper) {
        self.chord.lock().unwrap_or_else(|p| p.into_inner()).mapper = Some(mapper);
    }

    /// Follow `chord` from the next beat (immediately when stopped). `None` plays the literal
    /// source notes again.
    pub fn follow_chord(&self, chord: Option<LiveChord>) {
        let at = {
            let st = self.status.lock().unwrap_or_else(|p| p.into_inner());
            st.started.filter(|_| st.playing).map(|start| {
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(start) + Duration::from_millis(15);
                st.beats
                    .iter()
                    .find(|b| **b > elapsed)
                    .map_or(now, |b| start + *b)
            })
        };
        {
            let mut state = self.chord.lock().unwrap_or_else(|p| p.into_inner());
            match at {
                Some(at) => state.pending = Some((chord, at)),
                None => {
                    if chord.is_none() {
                        state.initial = None;
                    }
                    state.current = chord;
                    state.pending = None;
                    state.root_changed_quarter = None;
                }
            }
        }
        let (lock, cv) = &*self.cancel;
        drop(lock.lock().unwrap_or_else(|p| p.into_inner()));
        cv.notify_all();
    }
    pub fn stop(&mut self) {
        let (lock, cv) = &*self.cancel;
        *lock.lock().unwrap_or_else(|p| p.into_inner()) = true;
        cv.notify_all();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        self.chord.lock().unwrap_or_else(|p| p.into_inner()).root_changed_quarter = None;
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
                from_tick: None,
            },
        )
    }
    pub fn play_with_options(&mut self, doc: &Document, options: PlaybackOptions) -> Result<()> {
        self.play_with_initial_chord(doc, options, None)
    }
    /// Start playback with a fallback chord before the first Live Play input.
    pub fn play_with_initial_chord(
        &mut self,
        doc: &Document,
        options: PlaybackOptions,
        initial_chord: Option<LiveChord>,
    ) -> Result<()> {
        let PlaybackOptions {
            section,
            channels,
            index,
            name,
            looping,
            initial_effects,
            from_tick,
        } = options;
        let plan = plan_with_options(doc, section, &channels, initial_effects, from_tick)?;
        self.play_plan_with_initial_chord(plan, channels, index, name, looping, initial_chord)
    }
    pub fn play_plan(
        &mut self,
        plan: Plan,
        channels: Vec<u8>,
        index: usize,
        name: String,
        looping: bool,
    ) -> Result<()> {
        self.play_plan_with_initial_chord(plan, channels, index, name, looping, None)
    }
    fn play_plan_with_initial_chord(
        &mut self,
        plan: Plan,
        channels: Vec<u8>,
        index: usize,
        name: String,
        looping: bool,
        initial_chord: Option<LiveChord>,
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
        *self.switch.lock().unwrap_or_else(|p| p.into_inner()) = None;
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
        self.chord.lock().unwrap_or_else(|p| p.into_inner()).initial = initial_chord;
        self.spawn_player(plan, channels, looping, move |data| {
            connection.send(data).map_err(|e| e.to_string())
        });
        Ok(())
    }
    /// Start the playback thread on an already-connected sender.
    fn spawn_player(
        &mut self,
        plan: Plan,
        channels: Vec<u8>,
        looping: bool,
        mut send: impl FnMut(&[u8]) -> std::result::Result<(), String> + Send + 'static,
    ) {
        self.cancel = Arc::new((Mutex::new(false), Condvar::new()));
        let cancel = self.cancel.clone();
        let status = self.status.clone();
        let audible = self.audible.clone();
        let switch = self.switch.clone();
        let chord = self.chord.clone();
        self.chord.lock().unwrap_or_else(|p| p.into_inner()).pending = None;
        *audible.lock().unwrap_or_else(|p| p.into_inner()) = channels.clone();
        *status.lock().unwrap() = Status {
            playing: true,
            ..Default::default()
        };
        self.worker = Some(thread::spawn(move || {
            let failure = run_plan(
                plan, &channels, &audible, looping, &switch, &chord, &cancel, &status, &mut send,
            );
            let mut st = status.lock().unwrap_or_else(|p| p.into_inner());
            st.playing = false;
            if let Some(start) = st.started.take() {
                st.elapsed_ms = start.elapsed().as_millis() as u64;
            }
            st.error = failure;
        }));
    }
}
fn packet_channel(data: &[u8]) -> Option<u8> {
    let status = *data.first()?;
    if status < 0xf0 {
        Some((status & 0x0f) + 1)
    } else {
        None
    }
}

// Order of the persistent MIDI fields in the Genos part setup routine.
// Bank, program and parameter selection/data entry retain their stream order.
fn seek_state_order(data: &[u8]) -> Option<(u8, bool)> {
    match data {
        [status, cc, _] if status & 0xf0 == 0xb0 => match cc {
            11 => Some((0, true)),
            7 => Some((1, true)),
            10 => Some((2, true)),
            91 => Some((3, true)),
            93 => Some((4, true)),
            94 => Some((5, true)),
            0 | 32 | 6 | 98..=101 => Some((6, false)),
            1 => Some((7, true)),
            71..=74 => Some((8 + (cc - 71), true)),
            80..=82 => Some((12 + (cc - 80), true)),
            _ => None,
        },
        [status, _] if status & 0xf0 == 0xc0 => Some((6, false)),
        [status, _, _] if status & 0xf0 == 0xe0 => Some((15, true)),
        _ => None,
    }
}

fn seek_packets(plan: &Plan, cut: Duration, rhythm_only: bool) -> Vec<&[u8]> {
    let mut state = Vec::new();
    let mut latest = BTreeMap::new();
    let mut held: Vec<&[u8]> = Vec::new();
    let held_sources = plan.tempo.as_ref().map_or(0, |clock| clock.held_seek_sources);
    for packet in plan.packets.iter().take_while(|packet| packet.at <= cut) {
        let data = packet.data.as_slice();
        let Some(channel) = packet_channel(data) else { continue };
        if rhythm_only && plan.rhythm_sources() & channel_bit(channel) == 0 {
            continue;
        }
        if packet.at < cut && let Some((order, coalesce)) = seek_state_order(data) {
            if coalesce {
                latest.insert((channel, order), data);
            } else {
                state.push(((channel, order), data));
            }
        }
        if rhythm_only || held_sources & channel_bit(channel) == 0 {
            continue;
        }
        if let [status, pitch, velocity] = data {
            match (status & 0xf0, velocity) {
                (0x90, 1..) if packet.at < cut => held.push(data),
                (0x80, _) | (0x90, 0) => {
                    if let Some(index) = held.iter().position(|on| {
                        on[0] & 0x0f == status & 0x0f && on[1] == *pitch
                    }) {
                        held.remove(index);
                    }
                }
                _ => {}
            }
        }
    }
    state.extend(latest);
    state.sort_by_key(|(key, _)| *key);
    state.into_iter().map(|(_, data)| data).chain(held).collect()
}

#[derive(Clone, Copy)]
struct SourceNote {
    source: u8,
    output: Option<u8>,
    velocity: u8,
    routed: bool,
}

/// Playback state shared by the timed loop: note pairing, live chord and switch waits.
struct Live<'a, F: FnMut(&[u8]) -> std::result::Result<(), String>> {
    cancel: &'a (Mutex<bool>, Condvar),
    switch: &'a Mutex<Option<Switch>>,
    chord: &'a Mutex<ChordState>,
    send: F,
    /// One entry per note-on, in arrival order, including muted occurrences.
    notes: [Vec<SourceNote>; 16],
    section: Option<u64>,
    fill_section: Option<u64>,
    clock: Option<(Instant, f64, TempoClock)>,
    /// Destination part of each source, so sources sharing a part share its notes.
    routes: ChannelRoutes,
}
impl<F: FnMut(&[u8]) -> std::result::Result<(), String>> Live<'_, F> {
    /// Whether `pitch` already sounds on the part `channel` (0-based source) plays on.
    /// Within one source only chord-mapped notes are shared; across sources sharing a
    /// destination part an equal pitch is always the same sounding key.
    fn part_sounds(&self, channel: u8, pitch: u8, routed: bool) -> bool {
        let part = self.routes.destination(channel + 1);
        (0..16u8)
            .filter(|&other| self.routes.destination(other + 1) == part)
            .any(|other| {
                self.notes[usize::from(other)].iter().any(|note| {
                    note.output == Some(pitch) && (other != channel || routed || note.routed)
                })
            })
    }
    fn is_rhythm(&self, channel: u8) -> bool {
        let sources = self.clock.as_ref()
            .and_then(|(_, _, clock)| clock.rhythm_sources)
            .unwrap_or(DEFAULT_RHYTHM_SOURCES);
        sources & channel_bit(channel) != 0
    }

    fn routes_rhythm(&self, state: &ChordState) -> bool {
        state.initial.is_some()
            && self.clock.as_ref().and_then(|(_, _, clock)| clock.rhythm_sources).is_some()
    }

    fn release(&mut self, channels: impl Iterator<Item = u8>) -> std::result::Result<(), String> {
        for ch in channels {
            let channel = ch - 1;
            for data in [[0xb0 | channel, 64, 0], [0xb0 | channel, 123, 0]] {
                (self.send)(&data)?;
            }
            self.notes[usize::from(channel)].clear();
        }
        Ok(())
    }

    fn quarter_at(&self, at: Instant) -> Option<f64> {
        self.clock.as_ref().map(|(start, base, tempo)| {
            base + tempo.quarters_at(at.saturating_duration_since(*start))
        })
    }
    fn mapped(&self, channel: u8, pitch: u8, at: Instant) -> Option<LivePitch> {
        let state = self.chord.lock().unwrap_or_else(|p| p.into_inner());
        let rhythm = self.is_rhythm(channel + 1);
        if rhythm && !self.routes_rhythm(&state) {
            return None;
        }
        let section = if rhythm { self.fill_section.or(self.section) } else { self.section };
        let (Some(chord), Some(map), Some(section)) = (
            state.current.as_ref().or(state.initial.as_ref()),
            state.mapper.as_ref(),
            section,
        ) else {
            return None;
        };
        let to_root = state.root_changed_quarter
            .zip(self.clock.as_ref())
            .is_some_and(|(changed, (start, base, tempo))| {
                tempo.in_root_window(at.saturating_duration_since(*start), *base, changed)
            });
        Some(map(section, channel + 1, pitch, chord, to_root))
    }
    fn emit(&mut self, data: &[u8], at: Instant) -> std::result::Result<(), String> {
        let note = data.len() == 3 && matches!(data[0] & 0xf0, 0x80 | 0x90);
        if !note {
            return (self.send)(data);
        }
        let channel = data[0] & 0x0f;
        let source = data[1];
        if data[0] & 0xf0 == 0x90 && data[2] > 0 {
            let mapped = self.mapped(channel, source, at);
            let routed = mapped.is_some();
            let output = mapped.map_or(Some(source), |pitch| pitch.pitch);
            let shared = output.is_some_and(|pitch| self.part_sounds(channel, pitch, routed));
            self.notes[usize::from(channel)].push(SourceNote {
                source,
                output,
                velocity: data[2],
                routed,
            });
            if let Some(pitch) = output.filter(|_| !shared) {
                (self.send)(&[data[0], pitch, data[2]])?;
            }
            return Ok(());
        }
        let notes = &mut self.notes[usize::from(channel)];
        let Some(index) = notes.iter().position(|note| note.source == source) else {
            return (self.send)(data);
        };
        let note = notes.remove(index);
        let Some(pitch) = note.output else {
            return Ok(());
        };
        if self.part_sounds(channel, pitch, note.routed) {
            return Ok(());
        }
        (self.send)(&[data[0], pitch, data[2]])
    }
    /// Start following the pending chord and re-voice notes that are still held.
    fn apply_chord(&mut self) -> std::result::Result<(), String> {
        let (at, ordinary_changed, bass_changed, routes_rhythm) = {
            let mut state = self.chord.lock().unwrap_or_else(|p| p.into_inner());
            if state.pending.as_ref().is_some_and(|(_, at)| *at > Instant::now()) {
                return Ok(());
            }
            let Some((chord, at)) = state.pending.take() else {
                return Ok(());
            };
            let mut ordinary_changed = false;
            let mut bass_changed = false;
            if let (Some(previous), Some(next)) =
                (state.current.as_ref().or(state.initial.as_ref()), &chord)
            {
                let previous_bass = previous.bass.unwrap_or(previous.root);
                let next_bass = next.bass.unwrap_or(next.root);
                let roots_changed = previous.root != next.root || previous_bass != next_bass;
                ordinary_changed = previous.root != next.root || previous.quality != next.quality;
                bass_changed = previous_bass != next_bass
                    || previous.bass_quality.as_deref().unwrap_or(&previous.quality)
                        != next.bass_quality.as_deref().unwrap_or(&next.quality);
                if roots_changed {
                    state.root_changed_quarter = self.quarter_at(at);
                }
            } else {
                state.root_changed_quarter = None;
            }
            if chord.is_none() {
                state.initial = None;
            }
            state.current = chord;
            (at, ordinary_changed, bass_changed, self.routes_rhythm(&state))
        };
        for channel in 0..16u8 {
            let rhythm = self.is_rhythm(channel + 1);
            if rhythm && !routes_rhythm {
                continue;
            }
            let channel_index = usize::from(channel);
            let mut held: Vec<_> = self.notes[channel_index].iter().enumerate()
                .filter_map(|(index, note)| note.output.map(|pitch| (index, *note, pitch)))
                .collect();
            // The rhythm mute callback consumes the first eligible note in note-on order.
            if !rhythm {
                held.sort_by_key(|(_, note, _)| note.source);
            }
            let mut rhythm_portamento_sent = false;
            for (index, note, sent) in held {
                let SourceNote { source, velocity, .. } = note;
                let mapped = self.mapped(channel, source, at);
                let routed = note.routed || mapped.is_some();
                let LivePitch { mut pitch, portamento, stop_on_change, bass_region, held_mask_portamento } =
                    mapped.unwrap_or_else(|| Some(source).into());
                let changed = if bass_region { bass_changed } else { ordinary_changed };
                if rhythm {
                    if !changed {
                        continue;
                    }
                    if held_mask_portamento && !rhythm_portamento_sent {
                        (self.send)(&[0xb0 | channel, 84, sent])?;
                        rhythm_portamento_sent = true;
                        continue;
                    }
                }
                if stop_on_change && changed {
                    pitch = None;
                }
                if pitch == Some(sent) {
                    continue;
                }
                let notes = &mut self.notes[channel_index];
                let shared = |output, routed| notes.iter().enumerate().any(|(other_index, other)| {
                    other_index != index && other.output == Some(output) && (routed || other.routed)
                });
                let old_shared = shared(sent, note.routed);
                let new_attack = pitch.filter(|&output| !shared(output, routed));
                if portamento
                    && !old_shared
                    && let Some(target) = new_attack
                {
                    (self.send)(&[0xb0 | channel, 84, sent])?;
                    (self.send)(&[0x90 | channel, target, velocity])?;
                    (self.send)(&[0x80 | channel, sent, 0])?;
                } else {
                    if !old_shared {
                        (self.send)(&[0x80 | channel, sent, 0])?;
                    }
                    if let Some(target) = new_attack {
                        (self.send)(&[0x90 | channel, target, velocity])?;
                    }
                }
                notes[index] = SourceNote { output: pitch, routed, ..note };
            }
        }
        Ok(())
    }
    fn wait(&mut self, deadline: Instant) -> Wake {
        loop {
            match wait_for(self.cancel, deadline, self.switch, self.chord) {
                Wake::Chord => {
                    if let Err(e) = self.apply_chord() {
                        return Wake::Failed(e);
                    }
                }
                other => return other,
            }
        }
    }
}

fn splice_fill(plan: &mut Plan, fill: Plan, handoff: FillHandoff, at: Duration) {
    let fill_rhythm = fill.rhythm_sources();
    let replaced = plan.rhythm_sources() | fill_rhythm;
    let clock = plan.tempo.as_ref().expect("Fill handoff needs the source clock");
    let fill_clock = fill.tempo.as_ref().expect("Fill plan needs a clock");
    let source_quarter = clock.quarters_at(at);
    let remaining = (fill_clock.quarters_at(fill.duration) - handoff.phase).max(0.0);
    let end = handoff.end.min(clock.elapsed_at(source_quarter + remaining).max(at));
    plan.packets.retain(|packet| {
        handoff.rhythm_only && packet.at >= at && packet.at <= handoff.end
            && packet_channel(&packet.data).is_some_and(|ch| replaced & channel_bit(ch) == 0)
    });
    let cut = fill_clock.elapsed_at(handoff.phase);
    let mut restore: Vec<_> = seek_packets(&fill, cut, handoff.rhythm_only)
        .into_iter().map(|data| Packet { at, data: data.to_vec() }).collect();
    for mut packet in fill.packets {
        if handoff.rhythm_only
            && !packet_channel(&packet.data).is_some_and(|ch| fill_rhythm & channel_bit(ch) != 0)
        {
            continue;
        }
        let quarter = fill_clock.quarters_at(packet.at);
        if packet.at < cut {
            continue;
        }
        packet.at = clock.elapsed_at(source_quarter + (quarter - handoff.phase).max(0.0)).max(at);
        if packet.at <= handoff.end {
            plan.packets.push(packet);
        }
    }
    restore.append(&mut plan.packets);
    restore.sort_by_key(|packet| packet.at);
    plan.packets = restore;
    plan.finish_at(end);
    if let Some(clock) = &mut plan.tempo {
        clock.rhythm_sources = fill_clock.rhythm_sources;
    }
    // The fill's sources play on the fill's destination parts.
    for source in (1..=16u8).filter(|&source| !handoff.rhythm_only || fill_rhythm & channel_bit(source) != 0) {
        plan.routes.set(source, fill.routes.destination(source));
    }
    if !handoff.rhythm_only {
        plan.section = fill.section;
    }
}

#[allow(clippy::too_many_arguments)]
fn run_plan(
    plan: Plan,
    channels: &[u8],
    audible: &Mutex<Vec<u8>>,
    looping: bool,
    switch: &Mutex<Option<Switch>>,
    chord: &Mutex<ChordState>,
    cancel: &(Mutex<bool>, Condvar),
    status: &Mutex<Status>,
    mut send: impl FnMut(&[u8]) -> std::result::Result<(), String>,
) -> Option<String> {
    let mut failure = None;
    // Source tracks are played on their CASM destination parts; the active table follows
    // the sounding section. Every output channel is remembered for the final release.
    let routes = std::cell::Cell::new(plan.routes);
    let used_outputs = std::cell::Cell::new(0u16);
    let routed_send = |data: &[u8]| {
        let routed = routes.get().apply(data);
        if let Some(channel) = packet_channel(&routed) {
            used_outputs.set(used_outputs.get() | channel_bit(channel));
        }
        send(&routed)
    };
    let mut live = Live {
        cancel,
        switch,
        chord,
        send: routed_send,
        notes: std::array::from_fn(|_| Vec::new()),
        section: plan.section,
        fill_section: None,
        clock: None,
        routes: plan.routes,
    };
    'play: {
        // Send once, before any timed channel setup or notes, never each loop.
        for data in &plan.setup {
            if wait_until(cancel, Instant::now()) {
                break 'play;
            }
            if let Some(ch) = packet_channel(data) {
                let allow = audible.lock().unwrap_or_else(|p| p.into_inner());
                if !allow.contains(&ch) {
                    continue;
                }
            }
            if let Err(e) = (live.send)(data) {
                failure = Some(e);
                break 'play;
            }
        }
        let mut step = Step { plan, looping };
        let mut queue: std::collections::VecDeque<Step> = Default::default();
        let mut start = Instant::now();
        let mut quarters = 0.0;
        let mut fill_section = None;
        let mut packet_index = 0;
        loop {
            {
                let mut st = status.lock().unwrap_or_else(|p| p.into_inner());
                st.started = Some(start);
                st.bars = step.plan.bars.clone();
                st.beats = step.plan.beats.clone();
                st.section = fill_section.or(step.plan.section);
                st.tempo.clone_from(&step.plan.tempo);
            }
            live.section = step.plan.section;
            live.fill_section = fill_section;
            routes.set(step.plan.routes);
            live.routes = step.plan.routes;
            status.lock().unwrap_or_else(|p| p.into_inner()).routes = step.plan.routes;
            live.clock = step.plan.tempo.clone().map(|tempo| (start, quarters, tempo));
            let mut switched = false;
            for packet in &step.plan.packets[packet_index..] {
                match live.wait(start + packet.at) {
                    Wake::Cancelled => break 'play,
                    Wake::Failed(e) => {
                        failure = Some(e);
                        break 'play;
                    }
                    Wake::Switch => {
                        switched = true;
                        break;
                    }
                    Wake::Reached | Wake::Chord => {}
                }
                packet_index += 1;
                if let Some(ch) = packet_channel(&packet.data) {
                    let allow = audible.lock().unwrap_or_else(|p| p.into_inner());
                    if !allow.contains(&ch) {
                        status.lock().unwrap_or_else(|p| p.into_inner()).elapsed_ms =
                            packet.at.as_millis() as u64;
                        continue;
                    }
                }
                if let Err(e) = live.emit(&packet.data, start + packet.at) {
                    failure = Some(e);
                    break 'play;
                }
                status.lock().unwrap_or_else(|p| p.into_inner()).elapsed_ms =
                    packet.at.as_millis() as u64;
            }
            let end = start + step.plan.duration;
            if !switched {
                match live.wait(end) {
                    Wake::Cancelled => break 'play,
                    Wake::Failed(e) => {
                        failure = Some(e);
                        break 'play;
                    }
                    Wake::Switch => switched = true,
                    Wake::Reached | Wake::Chord => {}
                }
            }
            if switched {
                let next = {
                    let mut pending = switch.lock().unwrap_or_else(|p| p.into_inner());
                    if pending.as_ref().is_some_and(|next| next.at <= Instant::now()) {
                        pending.take()
                    } else {
                        None
                    }
                };
                let Some(next) = next else {
                    continue;
                };
                let fill = next.fill.filter(|_| next.source_start == start);
                let rhythm_only = fill.as_ref().is_some_and(|fill| fill.rhythm_only);
                queue = next.steps.into();
                let incoming = queue.pop_front().expect("switch has at least one step");
                let replaced = step.plan.rhythm_sources() | incoming.plan.rhythm_sources();
                if let Err(e) = live.release(channels.iter().copied()
                    .filter(|&ch| !rhythm_only || replaced & channel_bit(ch) != 0))
                {
                    failure = Some(e);
                    break 'play;
                }
                let main_seek = fill.as_ref().filter(|handoff| !handoff.rhythm_only).and_then(|handoff| {
                    incoming.plan.tempo.as_ref().filter(|clock| !clock.fill)
                        .map(|clock| {
                            let phase = handoff.phase.min(clock.quarters_at(incoming.plan.duration));
                            (clock.elapsed_at(phase), phase)
                        })
                });
                if let Some(fill) = fill.filter(|_| main_seek.is_none()) {
                    fill_section = incoming.plan.section;
                    step.plan.packets.drain(..packet_index);
                    splice_fill(&mut step.plan, incoming.plan, fill, next.at.saturating_duration_since(start));
                    step.looping = false;
                    packet_index = 0;
                    continue;
                }
                fill_section = None;
                quarters = live.quarter_at(next.at).unwrap_or(quarters);
                start = next.at.max(start);
                step = incoming;
                packet_index = 0;
                if let Some((elapsed, phase)) = main_seek
                    && let Some(origin) = start.checked_sub(elapsed)
                {
                    start = origin;
                    quarters -= phase;
                    packet_index = step.plan.packets.partition_point(|packet| packet.at < elapsed);
                    live.section = step.plan.section;
                    live.fill_section = None;
                    routes.set(step.plan.routes);
                    live.routes = step.plan.routes;
                    status.lock().unwrap_or_else(|p| p.into_inner()).routes = step.plan.routes;
                    live.clock = step.plan.tempo.clone().map(|clock| (start, quarters, clock));
                    for data in seek_packets(&step.plan, elapsed, false) {
                        if packet_channel(data).is_some_and(|ch| {
                                audible.lock().unwrap_or_else(|p| p.into_inner()).contains(&ch)
                            })
                            && let Err(e) = live.emit(data, next.at)
                        {
                            failure = Some(e);
                            break 'play;
                        }
                    }
                }
                if !queue.is_empty()
                    && step.plan.tempo.as_ref().is_some_and(|clock| clock.fill)
                    && let Some(&bar_end) = step.plan.bars.first()
                {
                    step.plan.finish_at(bar_end);
                }
            } else if !step.looping {
                // One-shot sections (fill-in, break, intro) hand over to the next queued step.
                match queue.pop_front() {
                    Some(next) => {
                        let completing_fill = fill_section.take().is_some()
                            || step.plan.tempo.as_ref().is_some_and(|clock| clock.fill);
                        // Notes still held must be released on the parts they started on.
                        let reroutes = next.plan.routes != step.plan.routes;
                        if (completing_fill || reroutes)
                            && let Err(e) = live.release(channels.iter().copied())
                        {
                            failure = Some(e);
                            break 'play;
                        }
                        quarters = live.quarter_at(end).unwrap_or(quarters);
                        step = next;
                        start = end;
                        packet_index = 0;
                    }
                    None => break,
                }
            } else {
                quarters = live.quarter_at(end).unwrap_or(quarters);
                start = end;
                packet_index = 0;
            }
        }
    }
    drop(live);
    // Release selected sources and every destination part they reached, even if startup
    // fails or is cancelled.
    let final_routes = routes.get();
    let released = channels.iter().fold(used_outputs.get(), |bits, &ch| {
        bits | channel_bit(final_routes.destination(ch))
    });
    for ch in (1..=16u8).filter(|&ch| released & channel_bit(ch) != 0) {
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
enum Wake {
    Cancelled,
    Reached,
    Switch,
    Chord,
    Failed(String),
}
/// Wait for `deadline`, waking early for cancellation, a queued bar-line switch or a chord
/// change that is due.
fn wait_for(
    cancel: &(Mutex<bool>, Condvar),
    deadline: Instant,
    switch: &Mutex<Option<Switch>>,
    chord: &Mutex<ChordState>,
) -> Wake {
    let (lock, cv) = cancel;
    let mut flag = lock.lock().unwrap_or_else(|p| p.into_inner());
    loop {
        if *flag {
            return Wake::Cancelled;
        }
        let switch_at = switch
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(|s| s.at)
            .filter(|at| *at <= deadline);
        let chord_at = chord
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .pending
            .as_ref()
            .map(|(_, at)| *at)
            .filter(|at| *at <= deadline);
        let target = [switch_at, chord_at]
            .into_iter()
            .flatten()
            .fold(deadline, Instant::min);
        let now = Instant::now();
        let remaining = target.saturating_duration_since(now);
        if remaining.is_zero() {
            return if chord_at.is_some_and(|at| at <= now) {
                Wake::Chord
            } else if switch_at.is_some_and(|at| at <= now) {
                Wake::Switch
            } else {
                Wake::Reached
            };
        }
        let (f, _) = cv
            .wait_timeout(flag, remaining)
            .unwrap_or_else(|p| p.into_inner());
        flag = f;
    }
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
    #[serde(skip)]
    pub(crate) injected: bool,
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
    /// Held (channel 1..=16, pitch) for MIDI LH chord recognition (SFM-045).
    held: Arc<Mutex<HeldNoteState>>,
    xf: Arc<Mutex<XfState>>,
    port_name: Option<String>,
}
impl Default for InputMonitor {
    fn default() -> Self {
        Self {
            _connection: None,
            events: Arc::new(Mutex::new(Vec::new())),
            held: Arc::new(Mutex::new(HeldNoteState::default())),
            xf: Arc::new(Mutex::new(XfState::default())),
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
        let held = Arc::new(Mutex::new(HeldNoteState::default()));
        let callback_events = events.clone();
        let callback_held = held.clone();
        let xf = Arc::new(Mutex::new(XfState::default()));
        let callback_xf = xf.clone();
        let started = Instant::now();
        let port = matches[0].clone();
        let connection = connect_input(input, &port, "GenosStyleStudio monitor", || {
            let callback_events = callback_events.clone();
            let callback_held = callback_held.clone();
            let callback_xf = callback_xf.clone();
            move |_t, data: &[u8], _: &mut ()| {
                update_held_notes(&callback_held, data);
                update_xf_chord(&callback_xf, data);
                let mut q = callback_events.lock().unwrap_or_else(|p| p.into_inner());
                let truncated = data.len() > 1024;
                let data = &data[..data.len().min(1024)];
                q.push(MonitorEvent {
                    stamp_ms: started.elapsed().as_millis() as u64,
                    data: data.to_vec(),
                    injected: false,
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
            }
        })?;
        self._connection = Some(connection);
        self.events = events;
        self.held = held;
        self.xf = xf;
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
    /// Pitches currently held. Optional MIDI channel (1..=16) and LH upper note
    /// (inclusive max pitch) match SFM chordPreviewChannel / chordPreviewSplit.
    pub fn held_notes(&self, channel: Option<u8>, max_pitch: Option<u8>) -> Vec<u8> {
        let set = self.held.lock().unwrap_or_else(|p| p.into_inner());
        let mut pitches: Vec<u8> = set
            .keys
            .iter()
            .filter(|(ch, pitch)| {
                channel.is_none_or(|want| *ch == want) && max_pitch.is_none_or(|max| *pitch <= max)
            })
            .map(|(_, pitch)| *pitch)
            .collect();
        pitches.sort_unstable();
        pitches.dedup();
        pitches
    }
    /// Single Finger result latched at the latest left-hand MIDI note-on.
    pub fn single_finger_chord(&self) -> Option<crate::chord_preview::RecognizedChord> {
        self.held
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .single_finger
            .clone()
    }
    /// Latest chord the keyboard's arranger reported over MIDI (XF chord message), with a
    /// counter that changes on every message so repeats of the same chord are noticed.
    pub fn xf_chord(&self) -> Option<XfChord> {
        let st = self.xf.lock().unwrap_or_else(|p| p.into_inner());
        st.chord.clone().map(|chord| XfChord {
            seq: st.seq,
            root: chord.root,
            quality: chord.quality,
            bass: chord.bass,
            bass_quality: chord.bass_quality,
        })
    }
    pub fn clear(&self) {
        *self.xf.lock().unwrap_or_else(|p| p.into_inner()) = XfState::default();
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        *self.held.lock().unwrap_or_else(|p| p.into_inner()) = HeldNoteState::default();
    }
}

#[derive(Default)]
struct XfState {
    seq: u64,
    chord: Option<LiveChord>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XfChord {
    pub seq: u64,
    pub root: u8,
    pub quality: String,
    pub bass: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_quality: Option<String>,
}
fn update_xf_chord(xf: &Mutex<XfState>, data: &[u8]) {
    if let Some(chord) = crate::chord_preview::xf_live_chord(data) {
        let mut st = xf.lock().unwrap_or_else(|p| p.into_inner());
        st.seq += 1;
        st.chord = Some(chord);
    }
}
#[derive(Default)]
struct HeldNoteState {
    keys: std::collections::BTreeSet<(u8, u8)>,
    single_finger: Option<crate::chord_preview::RecognizedChord>,
}

fn update_held_notes(held: &Mutex<HeldNoteState>, data: &[u8]) {
    if data.len() < 2 {
        return;
    }
    let st = data[0] & 0xf0;
    let channel = (data[0] & 0x0f) + 1;
    let note = data[1];
    if note > 127 {
        return;
    }
    let mut set = held.lock().unwrap_or_else(|p| p.into_inner());
    match st {
        0x90 if data.len() >= 3 && data[2] > 0 => {
            set.keys.insert((channel, note));
            const LEFT_HAND_MAX: u8 = 59;
            if note <= LEFT_HAND_MAX {
                let notes: Vec<_> = set
                    .keys
                    .iter()
                    .filter_map(|&(_, pitch)| (pitch <= LEFT_HAND_MAX).then_some(pitch))
                    .collect();
                set.single_finger = crate::chord_preview::recognize_single_finger(&notes);
            }
        }
        0x80 | 0x90 => {
            set.keys.remove(&(channel, note));
        }
        _ => {}
    }
}
impl Drop for InputMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod audible_filter_tests {
    use super::{Transport, packet_channel};

    #[test]
    fn packet_channel_reads_voice_status() {
        assert_eq!(packet_channel(&[0x90, 60, 100]), Some(1));
        assert_eq!(packet_channel(&[0xb2, 7, 64]), Some(3));
        assert_eq!(packet_channel(&[0xe9, 0, 64]), Some(10));
        assert_eq!(packet_channel(&[0xf0, 0x43, 0xf7]), None);
        assert_eq!(packet_channel(&[]), None);
    }

    #[test]
    fn set_audible_channels_rejects_out_of_range() {
        let t = Transport::default();
        assert!(t.set_audible_channels(vec![1, 17]).is_err());
        assert!(t.set_audible_channels(vec![9, 10, 11]).is_ok());
        assert_eq!(t.audible_channels(), vec![9, 10, 11]);
    }
}
#[cfg(test)]
mod monitor_tests {
    use super::{HeldNoteState, summarize, update_held_notes};
    use std::sync::{Arc, Mutex};

    #[test]
    fn held_notes_track_on_and_off() {
        let held = Arc::new(Mutex::new(HeldNoteState::default()));
        update_held_notes(&held, &[0x90, 60, 100]);
        update_held_notes(&held, &[0x91, 64, 80]);
        update_held_notes(&held, &[0x92, 67, 90]);
        assert_eq!(
            held.lock().unwrap().keys.iter().copied().collect::<Vec<_>>(),
            vec![(1, 60), (2, 64), (3, 67)]
        );
        update_held_notes(&held, &[0x81, 64, 0]); // ch2 note-off
        update_held_notes(&held, &[0x92, 67, 0]); // ch3 note-on vel 0 = off
        assert_eq!(
            held.lock().unwrap().keys.iter().copied().collect::<Vec<_>>(),
            vec![(1, 60)]
        );
    }

    #[test]
    fn single_finger_latches_from_note_events_between_snapshots() {
        let monitor = super::InputMonitor::default();
        let feed = |message: &[u8]| update_held_notes(&monitor.held, message);
        feed(&[0x90, 48, 100]);
        feed(&[0x90, 55, 100]);
        assert_eq!(monitor.single_finger_chord().unwrap().quality, "7");
        feed(&[0x80, 48, 0]);
        assert_eq!(monitor.single_finger_chord().unwrap().quality, "7");
        feed(&[0x90, 55, 0]);
        feed(&[0x90, 55, 100]);
        assert_eq!(monitor.held_notes(None, Some(59)), vec![55]);
        assert_eq!(monitor.single_finger_chord().unwrap().quality, "Major");
        feed(&[0x90, 59, 100]);
        feed(&[0x80, 59, 0]);
        feed(&[0x90, 72, 100]);
        assert_eq!(monitor.held_notes(None, Some(59)), vec![55]);
        let chord = monitor.single_finger_chord().unwrap();
        assert_eq!((chord.root, chord.quality.as_str()), (11, "7"));
        monitor.clear();
        assert!(monitor.single_finger_chord().is_none());
    }

    #[test]
    fn held_notes_filter_channel_and_lh_split() {
        let mon = super::InputMonitor::default();
        {
            let mut h = mon.held.lock().unwrap();
            h.keys.insert((1, 48));
            h.keys.insert((1, 72));
            h.keys.insert((2, 55));
        }
        assert_eq!(mon.held_notes(None, Some(59)), vec![48, 55]);
        assert_eq!(mon.held_notes(Some(1), Some(59)), vec![48]);
        assert_eq!(mon.held_notes(Some(2), None), vec![55]);
    }

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
mod plan_from_tests {
    use super::*;
    use crate::document::Document;
    use std::fs;
    use std::path::Path;

    pub(super) fn fixture_doc() -> Document {
        let bytes = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
        )
        .unwrap();
        Document::parse(&bytes).unwrap()
    }

    #[test]
    fn plan_from_mid_section_shortens_duration() {
        let doc = fixture_doc();
        let section = doc.sections.iter().find(|s| s.label == "Main A").unwrap();
        let channels: Vec<u8> = (1..=16).collect();
        let full = plan(&doc, section.id, &channels).unwrap();
        let mid = section.start + (section.end - section.start) / 2;
        let partial = plan_from(&doc, section.id, &channels, Some(mid)).unwrap();
        assert!(partial.duration < full.duration);
        assert!(!partial.duration.is_zero());
    }

    #[test]
    fn rhythm_sources_follow_section_destinations() {
        let mut doc = fixture_doc();
        let mut rhythm = doc.routes[0].clone();
        doc.style.chunks.retain(|chunk| chunk.tag != *b"CASM");
        let mut doc = Document::from_style(doc.style).unwrap();
        let id = doc.sections.iter().find(|s| s.label == "Main A").unwrap().id;
        rhythm.sections = vec!["Main A".into()];
        rhythm.source = 3;
        rhythm.destination = 9;
        let mut accompaniment = rhythm.clone();
        accompaniment.source = 9;
        accompaniment.destination = 14;
        let mut second_rhythm = rhythm.clone();
        second_rhythm.destination = 10;
        let mut second_accompaniment = accompaniment.clone();
        second_accompaniment.destination = 16;
        let mut mixed = rhythm.clone();
        mixed.destination = 14;
        mixed.source = 5;
        let mut mixed_rhythm = rhythm.clone();
        mixed_rhythm.source = 5;
        doc.routes = vec![rhythm, second_rhythm, accompaniment, second_accompaniment, mixed, mixed_rhythm];
        let playback = plan(&doc, id, &[3, 5, 9]).unwrap();
        assert_eq!(playback.rhythm_sources(), channel_bit(3));
        assert_eq!(playback.tempo.unwrap().held_seek_sources, channel_bit(9));
    }

    #[test]
    fn plan_plays_each_source_on_its_casm_destination_part() {
        let mut doc = fixture_doc();
        let mut pad = doc.routes[0].clone();
        doc.style.chunks.retain(|chunk| chunk.tag != *b"CASM");
        let mut doc = Document::from_style(doc.style).unwrap();
        let id = doc.sections.iter().find(|s| s.label == "Main A").unwrap().id;
        pad.sections = vec!["Main A".into()];
        pad.source = 6;
        pad.destination = 14;
        let mut other_section = pad.clone();
        other_section.sections = vec!["Main B".into()];
        other_section.source = 7;
        let mut conflicting = pad.clone();
        conflicting.source = 5;
        let mut conflicting_other = conflicting.clone();
        conflicting_other.destination = 15;
        doc.routes = vec![pad, other_section, conflicting, conflicting_other];
        let routes = plan(&doc, id, &[5, 6, 7, 9]).unwrap().routes;
        assert_eq!(routes.destination(6), 14);
        assert_eq!(routes.destination(7), 7, "routes of other sections do not apply");
        assert_eq!(routes.destination(5), 5, "ambiguous routes keep the source channel");
        assert_eq!(routes.destination(9), 9);
    }

    #[test]
    fn ordinary_sff2_uses_first_cseg_and_last_source_record() {
        use crate::smf::{Chunk, Event, pack};
        let doc = fixture_doc();
        let mut style = doc.style;
        style.chunks.retain(|chunk| !matches!(&chunk.tag, b"CASM" | b"Ghst" | b"Rsec" | b"Rtrk"));
        style.tracks[0].push(Event {
            id: u64::MAX,
            order: u64::MAX,
            tick: 0,
            data: vec![0xff, 6, 4, b'S', b'F', b'F', b'2'],
        });
        let cseg = |labels: &[u8], pairs: &[(u8, u8)]| {
            let mut records = vec![Chunk { tag: *b"Sdec", data: labels.to_vec() }];
            for &(source, destination) in pairs {
                let mut data = vec![0; 47];
                data[0] = source - 1;
                data[9] = destination - 1;
                records.push(Chunk { tag: *b"Ctb2", data });
            }
            Chunk { tag: *b"CSEG", data: pack(&records).unwrap() }
        };
        style.chunks.push(Chunk {
            tag: *b"CASM",
            data: pack(&[
                cseg(b"Main A,Main B", &[(3, 9), (3, 14)]),
                cseg(b"Main A,Main C", &[(3, 10), (4, 9)]),
            ]).unwrap(),
        });
        let doc = Document::from_style(style).unwrap();
        let id = doc.sections.iter().find(|s| s.label == "Main A").unwrap().id;
        let playback = plan(&doc, id, &[3, 4]).unwrap();
        assert_eq!(doc.routes.len(), 4);
        assert_eq!(doc.firmware_cseg_route("Main A", 3).flatten().unwrap().destination, 14);
        assert!(matches!(doc.firmware_cseg_route("Main A", 4), Some(None)));
        assert_eq!(doc.firmware_cseg_route("Main C", 3).flatten().unwrap().destination, 10);
        assert_eq!(playback.rhythm_sources(), 0);
        assert_eq!(playback.tempo.unwrap().held_seek_sources, channel_bit(3));

        let mut without_main_a = doc.style;
        for track in &mut without_main_a.tracks {
            track.retain(|event| event.meta(6) != Some(b"Main A"));
        }
        let doc = Document::from_style(without_main_a).unwrap();
        assert_eq!(doc.routes.len(), 4);
        assert!(matches!(doc.firmware_cseg_route("Main A", 3), Some(None)));
        assert_eq!(
            doc.firmware_cseg_route("Main C", 3).flatten().unwrap().destination,
            10
        );
    }

    #[test]
    fn tempo_clock_uses_tempo_events_after_play_cursor() {
        let mut doc = fixture_doc();
        let section = doc.sections.iter().find(|s| s.label == "Main A").unwrap();
        let (id, start) = (section.id, section.start);
        let ppq = u64::from(doc.style.ppq);
        for track in &mut doc.style.tracks {
            track.retain(|event| event.meta(0x51).is_none());
        }
        doc.style.tracks[0].push(crate::smf::Event {
            id: u64::MAX,
            order: u64::MAX,
            tick: start + ppq / 2,
            data: vec![0xff, 0x51, 3, 0x03, 0xd0, 0x90],
        });
        let plan = plan_from(&doc, id, &[11], Some(start + ppq / 4)).unwrap();
        let tempo = plan.tempo.unwrap();
        assert_eq!(tempo.quarters_at(Duration::from_millis(125)), 0.25);
        assert_eq!(tempo.quarters_at(Duration::from_micros(187_500)), 0.5);
    }
}

#[cfg(test)]
mod playback_tests {
    use super::*;

    #[test]
    fn fill_window_matches_firmware_meter_boundaries() {
        for (numerator, denominator) in [(4, 4), (3, 4), (6, 8), (2, 2)] {
            let beat = 4.0 / f64::from(denominator);
            let bar = f64::from(numerator) * beat;
            let clock = TempoClock {
                segments: vec![(Duration::ZERO, 0.0, 500_000)],
                meter: Some(FillMeter {
                    beat_quarters: beat,
                    bar_quarters: bar,
                    start_quarters: 0.0,
                }),
                fill: false,
                held_seek_sources: 0,
                rhythm_sources: None,
            };
            let status = Status {
                bars: vec![clock.elapsed_at(bar), clock.elapsed_at(2.0 * bar)],
                tempo: Some(clock.clone()),
                ..Default::default()
            };
            for (phase, expected) in [
                (0.0, Some(false)),
                (beat / 2.0, Some(false)),
                (beat / 2.0 + 0.01, Some(true)),
                (bar - beat / 2.0, Some(true)),
                (bar - beat / 2.0 + 0.01, None),
                (bar + beat, Some(true)),
            ] {
                let actual = fill_handoff(&status, clock.elapsed_at(phase));
                assert_eq!(actual.as_ref().map(|fill| fill.rhythm_only), expected);
                if let Some(fill) = actual {
                    let end = if phase < bar { bar } else { 2.0 * bar };
                    assert_eq!(fill.end, clock.elapsed_at(end));
                }
            }
        }
    }

    #[test]
    fn repeating_the_sounding_fill_queues_a_full_track_main_seek() {
        let transport = Transport::default();
        let clock = TempoClock {
            segments: vec![(Duration::ZERO, 0.0, 2_000_000)],
            meter: Some(FillMeter {
                beat_quarters: 1.0,
                bar_quarters: 4.0,
                start_quarters: 0.0,
            }),
            fill: true,
            held_seek_sources: 0,
            rhythm_sources: None,
        };
        *transport.status.lock().unwrap() = Status {
            playing: true,
            started: Some(Instant::now() - Duration::from_secs(2)),
            section: Some(2),
            bars: vec![Duration::from_secs(8)],
            tempo: Some(clock.clone()),
            ..Default::default()
        };
        let mut fill = test_plan();
        fill.section = Some(2);
        fill.tempo = Some(clock.clone());
        fill.duration = Duration::from_secs(8);
        let mut main = test_plan();
        main.section = Some(1);
        main.tempo = Some(TempoClock { fill: false, ..clock });
        main.duration = Duration::from_secs(8);
        transport.queue_switch(vec![
            Step { plan: fill, looping: false },
            Step { plan: main, looping: true },
        ]).unwrap();
        let pending = transport.switch.lock().unwrap();
        let pending = pending.as_ref().unwrap();
        assert_eq!(pending.steps.len(), 1);
        assert_eq!(pending.steps[0].plan.section, Some(1));
        let handoff = pending.fill.as_ref().unwrap();
        assert!(!handoff.rhythm_only);
        assert!((1.0..=3.5).contains(&handoff.phase));
    }

    #[test]
    fn mid_bar_fill_keeps_source_accompaniment_and_seeks_in_musical_time() {
        let packet = |ms, status, note| Packet {
            at: Duration::from_millis(ms),
            data: vec![status, note, 100],
        };
        let clock = |tempo| TempoClock {
            segments: vec![(Duration::ZERO, 0.0, tempo)],
            meter: None,
            fill: false,
            held_seek_sources: 0,
            rhythm_sources: None,
        };
        let mut source = test_plan();
        source.section = Some(1);
        source.tempo = Some(clock(500_000));
        source.packets = vec![
            packet(400, 0x90, 60),
            packet(600, 0x98, 36),
            packet(650, 0x92, 37),
            packet(800, 0x80, 60),
            packet(1200, 0x90, 62),
        ];
        let mut fill = test_plan();
        fill.section = Some(2);
        fill.tempo = Some(TempoClock { rhythm_sources: Some(channel_bit(3)), ..clock(250_000) });
        fill.duration = Duration::from_millis(600);
        fill.packets = vec![
            packet(0, 0xb2, 0),
            Packet { at: Duration::from_millis(10), data: vec![0xc2, 23] },
            packet(20, 0xb2, 64),
            packet(30, 0xb2, 84),
            packet(40, 0xb0, 7),
            packet(100, 0x92, 38),
            packet(300, 0x92, 40),
            packet(350, 0x90, 65),
            packet(600, 0x92, 42),
        ];
        splice_fill(
            &mut source,
            fill,
            FillHandoff { phase: 1.0, end: Duration::from_secs(1), rhythm_only: true },
            Duration::from_millis(500),
        );
        let events: Vec<_> = source.packets.iter().map(|p| (p.at.as_millis(), p.data[1])).collect();
        assert_eq!(events, [(500, 0), (500, 23), (600, 40), (800, 60)]);
        assert_eq!(source.section, Some(1));
        assert_eq!(source.rhythm_sources(), channel_bit(3));
        assert_eq!(source.duration, Duration::from_secs(1));
    }

    #[test]
    fn seek_restores_only_eligible_held_notes_after_controller_state() {
        let mut plan = test_plan();
        plan.tempo = Some(TempoClock {
            segments: vec![(Duration::ZERO, 0.0, 500_000)],
            meter: None,
            fill: false,
            held_seek_sources: 1,
            rhythm_sources: None,
        });
        let packet = |ms, data| Packet { at: Duration::from_millis(ms), data };
        plan.packets = vec![
            packet(0, vec![0x90, 60, 100]),
            packet(0, vec![0x90, 62, 80]),
            packet(0, vec![0xb0, 7, 55]),
            packet(0, vec![0xe0, 0, 60]),
            packet(1, vec![0x91, 64, 100]),
            packet(1, vec![0xb0, 0, 8]),
            packet(1, vec![0xc0, 23]),
            packet(2, vec![0xb0, 7, 99]),
            packet(2, vec![0xb0, 11, 101]),
            packet(2, vec![0xe0, 12, 65]),
            packet(3, vec![0x80, 60, 0]),
            packet(3, vec![0x90, 65, 100]),
        ];
        let cut = Duration::from_millis(3);
        assert_eq!(seek_packets(&plan, cut, false), [
            &[0xb0, 11, 101][..], &[0xb0, 7, 99], &[0xb0, 0, 8], &[0xc0, 23],
            &[0xe0, 12, 65], &[0x90, 62, 80],
        ]);
        assert!(seek_packets(&plan, cut, true).is_empty());
    }

    #[test]
    #[ignore = "requires the local Genos firmware tables and held rhythm replay"]
    fn held_rhythm_changes_match_firmware_replay() {
        assert_held_rhythm_replay("GENOS_RHYTHM_HELD_REPLAY", 96);
    }

    #[test]
    #[ignore = "requires the local Genos firmware tables and mixed Bass rhythm replay"]
    fn mixed_bass_rhythm_changes_match_firmware_replay() {
        assert_held_rhythm_replay("GENOS_RHYTHM_BASS_REPLAY", 2_304);
    }

    fn assert_held_rhythm_replay(env: &str, expected_cases: usize) {
        #[derive(serde::Deserialize)]
        struct Case {
            payload: Vec<u8>,
            running: Option<bool>,
            rtr: [u8; 3],
            variant: String,
            old: [u8; 4],
            target: [u8; 4],
            pitches: Vec<u8>,
            startup: Vec<[u8; 3]>,
            messages: Vec<[u8; 3]>,
            release: Option<Vec<[u8; 3]>>,
        }
        let path = std::env::var_os(env).unwrap();
        let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let cases: Vec<_> = cases.into_iter().filter(|case| case.running != Some(false)).collect();
        assert_eq!(cases.len(), expected_cases);
        let mut doc = super::plan_from_tests::fixture_doc();
        let mut route = doc.routes.remove(0);
        doc.style.chunks.retain(|chunk| chunk.tag != *b"CASM");
        let mut doc = Document::from_style(doc.style).unwrap();
        let section = doc.sections.iter().find(|section| section.label == "Main A").unwrap().id;
        route.sections = vec!["Main A".into()];
        doc.routes.push(route);
        let name = |quality| match quality {
            0 => "Major",
            8 => "Minor",
            34 => "Genos:34",
            _ => panic!("unexpected chord type {quality}"),
        };
        let chord = |[root, quality, bass, bass_quality]: [u8; 4]| LiveChord {
            root,
            quality: name(quality).into(),
            bass: (bass != root).then_some(bass),
            bass_quality: (bass_quality != quality).then(|| name(bass_quality).into()),
        };
        for case in cases {
            let route = &mut doc.routes[0];
            route.source = case.payload[0] + 1;
            route.destination = case.payload[9] + 1;
            route.bytes = case.payload;
            let source = route.source;
            let destination = route.destination;
            let plan = plan(&doc, section, &[source]).unwrap();
            let rules = doc.clone();
            let state = Mutex::new(ChordState {
                initial: Some(chord([0, 34, 0, 34])),
                current: Some(chord(case.old)),
                mapper: Some(Arc::new(move |section, source, pitch, chord, in_window| {
                    crate::live_preview::follow_routed_pitch(&rules, section, source, pitch, chord, in_window)
                })),
                ..ChordState::default()
            });
            let mut sent = Vec::new();
            let at = Instant::now();
            let mut live = Live {
                cancel: &(Mutex::new(false), Condvar::new()),
                switch: &Mutex::new(None),
                chord: &state,
                send: |packet| {
                    sent.push(<[u8; 3]>::try_from(packet).unwrap());
                    Ok(())
                },
                notes: std::array::from_fn(|_| Vec::new()),
                section: Some(section),
                fill_section: None,
                clock: Some((at, 0.0, plan.tempo.unwrap())),
                routes: ChannelRoutes::default(),
            };
            for &pitch in &case.pitches {
                live.emit(&[0x90 | (source - 1), pitch, 90], at).unwrap();
            }
            state.lock().unwrap().pending = Some((Some(chord(case.target)), at));
            live.apply_chord().unwrap();
            if case.release.is_some() {
                live.emit(&[0x80 | (source - 1), 60, 0], at).unwrap();
            }
            let actual: Vec<_> = sent.iter().map(|&[status, first, second]| {
                // Compare the equivalent MIDI note-off encodings on the destination channel.
                let kind = if status & 0xf0 == 0x80 { 0x90 } else { status & 0xf0 };
                [kind | (destination - 1), first, second]
            }).collect();
            let mut expected = case.startup;
            expected.extend(case.messages);
            match (case.variant.as_str(), case.rtr[1], case.release) {
                ("masked", 1 | 2, Some(release)) => {
                    // The track's mute suppresses the firmware note-off. The app must
                    // still pair the note it sent to the external MIDI destination.
                    assert!(release.is_empty());
                    expected.push([0x90 | (destination - 1), 60, 0]);
                }
                ("unmasked", 1..=4, Some(release)) => {
                    assert_eq!(release, [[0x90 | (destination - 1), 60, 0]]);
                }
                (_, _, Some(release)) => expected.extend(release),
                _ => {}
            }
            assert_eq!(actual, expected, "destination {destination}, RTR {:?}, {}, notes {:?}, chord {:?} -> {:?}",
                case.rtr, case.variant, case.pitches, case.old, case.target);
        }
    }

    #[test]
    fn repeated_source_notes_pair_muted_and_audible_occurrences_in_order() {
        let chord = |root| LiveChord {
            root,
            quality: "Major".into(),
            bass: None,
            bass_quality: None,
        };
        let state = Mutex::new(ChordState {
            current: Some(chord(0)),
            mapper: Some(Arc::new(|_, _, pitch, chord, _| {
                (chord.root != 0).then_some(pitch + chord.root).into()
            })),
            ..ChordState::default()
        });
        let mut sent = Vec::new();
        let at = Instant::now();
        let mut live = Live {
            cancel: &(Mutex::new(false), Condvar::new()),
            switch: &Mutex::new(None),
            chord: &state,
            send: |packet| {
                sent.push(<[u8; 3]>::try_from(packet).unwrap());
                Ok(())
            },
            notes: std::array::from_fn(|_| Vec::new()),
            section: Some(1),
            fill_section: None,
            clock: None,
            routes: ChannelRoutes::default(),
        };
        live.emit(&[0x90, 60, 90], at).unwrap();
        state.lock().unwrap().pending = Some((Some(chord(2)), at));
        live.apply_chord().unwrap();
        live.emit(&[0x90, 60, 70], at).unwrap();
        live.emit(&[0x80, 60, 11], at).unwrap();
        live.emit(&[0x80, 60, 22], at).unwrap();
        assert!(live.notes[0].is_empty());
        assert_eq!(sent, [[0x90, 62, 70], [0x80, 62, 22]]);
    }

    #[test]
    #[ignore = "requires the local Genos firmware tables and overlapping rhythm replay"]
    fn overlapping_rhythm_matches_firmware_active_pitches() {
        #[derive(serde::Deserialize)]
        struct Case {
            payload: Vec<u8>,
            rtr: u8,
            variant: String,
            old: u8,
            steps: Vec<(String, u8, u8)>,
            events: Vec<Vec<[u8; 3]>>,
        }
        let path = std::env::var_os("GENOS_RHYTHM_OVERLAP_REPLAY").unwrap();
        let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(cases.len(), 192);
        let mut doc = super::plan_from_tests::fixture_doc();
        let mut route = doc.routes.remove(0);
        doc.style.chunks.retain(|chunk| chunk.tag != *b"CASM");
        let mut doc = Document::from_style(doc.style).unwrap();
        let section = doc.sections.iter().find(|section| section.label == "Main A").unwrap().id;
        route.sections = vec!["Main A".into()];
        doc.routes.push(route);
        let chord = |root, quality: &str| LiveChord {
            root, quality: quality.into(), bass: None, bass_quality: None,
        };
        for case in cases {
            let route = &mut doc.routes[0];
            route.source = case.payload[0] + 1;
            route.destination = case.payload[9] + 1;
            route.bytes = case.payload;
            let source = route.source;
            let rules = doc.clone();
            let state = Mutex::new(ChordState {
                initial: Some(chord(0, "Genos:34")),
                current: Some(chord(case.old, "Major")),
                mapper: Some(Arc::new(move |section, source, pitch, chord, in_window| {
                    crate::live_preview::follow_routed_pitch(&rules, section, source, pitch, chord, in_window)
                })),
                ..ChordState::default()
            });
            let sent = std::cell::RefCell::new(Vec::new());
            let at = Instant::now();
            let mut live = Live {
                cancel: &(Mutex::new(false), Condvar::new()),
                switch: &Mutex::new(None),
                chord: &state,
                send: |packet| {
                    sent.borrow_mut().push(<[u8; 3]>::try_from(packet).unwrap());
                    Ok(())
                },
                notes: std::array::from_fn(|_| Vec::new()),
                section: Some(section),
                fill_section: None,
                clock: Some((at, 0.0, plan(&doc, section, &[source]).unwrap().tempo.unwrap())),
                routes: ChannelRoutes::default(),
            };
            let mut firmware = [0usize; 128];
            let mut app = [false; 128];
            assert_eq!(case.steps.len(), case.events.len());
            for ((kind, first, velocity), expected) in case.steps.into_iter().zip(case.events) {
                let context = format!("{}, RTR {}, {kind} {first}", case.variant, case.rtr);
                match kind.as_str() {
                    "on" => live.emit(&[0x90 | (source - 1), first, velocity], at).unwrap(),
                    "off" => live.emit(&[0x80 | (source - 1), first, velocity], at).unwrap(),
                    "change" => {
                        state.lock().unwrap().pending = Some((Some(chord(first, "Major")), at));
                        live.apply_chord().unwrap();
                    }
                    _ => panic!("Unexpected step {kind}"),
                }
                for [status, pitch, velocity] in expected {
                    match (status & 0xf0, velocity) {
                        (0x90, 0) => firmware[usize::from(pitch)] -= 1,
                        (0x90, _) => firmware[usize::from(pitch)] += 1,
                        (0xb0, _) => {}
                        _ => panic!("Unexpected firmware event {status:#04x}"),
                    }
                }
                for [status, pitch, velocity] in sent.borrow_mut().drain(..) {
                    assert_eq!(status & 0xf, source - 1);
                    let active = &mut app[usize::from(pitch)];
                    match (status & 0xf0, velocity) {
                        (0x80, _) | (0x90, 0) => {
                            assert!(*active, "unpaired note-off {pitch}, {context}");
                            *active = false;
                        }
                        (0x90, _) => {
                            assert!(!*active, "duplicate note-on {pitch}, {context}");
                            *active = true;
                        }
                        (0xb0, _) => assert_eq!(pitch, 84),
                        _ => panic!("Unexpected app event {status:#04x}"),
                    }
                }
                assert_eq!(app, firmware.map(|count| count > 0), "{context}");
            }
            assert!(live.notes[usize::from(source - 1)].is_empty());
            assert!(!app.into_iter().any(|active| active));
        }
    }

    #[test]
    fn transport_keeps_accompaniment_sounding_until_the_mid_bar_fill_ends() {
        let note = |ms, status, pitch| Packet {
            at: Duration::from_millis(ms),
            data: vec![status, pitch, 100],
        };
        let clock = TempoClock {
            segments: vec![(Duration::ZERO, 0.0, 20_000)],
            meter: None,
            fill: false,
            held_seek_sources: 0,
            rhythm_sources: Some(channel_bit(9)),
        };
        let mut source = test_plan();
        source.setup.clear();
        source.section = Some(1);
        source.tempo = Some(clock.clone());
        source.duration = Duration::from_millis(80);
        source.packets = vec![note(0, 0x90, 60), note(45, 0x80, 60), note(50, 0x98, 36)];
        let mut fill = test_plan();
        fill.section = Some(2);
        fill.tempo = Some(clock);
        fill.duration = Duration::from_millis(80);
        fill.packets = vec![
            note(5, 0x98, 38),
            note(25, 0x98, 40),
            note(28, 0x98, 41),
            note(30, 0x90, 65),
            note(35, 0x88, 40),
            note(38, 0x88, 41),
        ];
        let mut back = test_plan();
        back.section = Some(3);
        back.packets = vec![note(0, 0x90, 62)];
        let switch = Mutex::new(None);
        let status = Mutex::new(Status::default());
        let chord = Mutex::new(ChordState {
            initial: Some(LiveChord {
                root: 0,
                quality: "Genos:34".into(),
                bass: None,
                bass_quality: None,
            }),
            mapper: Some(Arc::new(|section, channel, pitch, _, _| {
                if channel == 9 {
                    assert_eq!(section, 2, "incoming rhythm must use Fill rules");
                    return (pitch != 41).then_some(pitch + 2).into();
                }
                Some(pitch).into()
            })),
            ..ChordState::default()
        });
        let mut sent = Vec::new();
        let failure = run_plan(
            source, &[1, 9], &Mutex::new(vec![1, 9]), false, &switch,
            &chord, &(Mutex::new(false), Condvar::new()),
            &status,
            |data| {
                sent.push(data.to_vec());
                if data == [0x90, 60, 100] {
                    let start = status.lock().unwrap().started.unwrap();
                    *switch.lock().unwrap() = Some(Switch {
                        at: start + Duration::from_millis(20),
                        steps: vec![Step { plan: fill.clone(), looping: false },
                                    Step { plan: back.clone(), looping: false }],
                        fill: Some(FillHandoff {
                            phase: 1.0, end: Duration::from_millis(80), rhythm_only: true,
                        }),
                        source_start: start,
                    });
                }
                Ok(())
            },
        );
        assert_eq!(failure, None);
        assert_eq!(&sent[..9], &[
            vec![0x90, 60, 100], vec![0xb8, 64, 0], vec![0xb8, 123, 0], vec![0x98, 42, 100],
            vec![0x88, 42, 100],
            vec![0x80, 60, 100], vec![0xb0, 64, 0], vec![0xb0, 123, 0], vec![0xb8, 64, 0],
        ]);
        assert_eq!(sent[10], [0x90, 62, 100]);
    }

    #[test]
    fn full_fill_returns_to_main_at_its_first_bar_and_releases_sustain() {
        let mut source = test_plan();
        source.setup.clear();
        source.duration = Duration::from_millis(20);
        let mut fill = test_plan();
        fill.tempo = Some(TempoClock {
            segments: vec![(Duration::ZERO, 0.0, 20_000)],
            meter: None,
            fill: true,
            held_seek_sources: 0,
            rhythm_sources: None,
        });
        fill.duration = Duration::from_millis(20);
        fill.bars = vec![Duration::from_millis(10), fill.duration];
        fill.packets = vec![
            Packet { at: Duration::ZERO, data: vec![0xb0, 64, 127] },
            Packet { at: Duration::from_millis(15), data: vec![0x90, 64, 100] },
        ];
        let mut back = test_plan();
        back.packets = vec![Packet { at: Duration::ZERO, data: vec![0x90, 62, 100] }];
        let switch = Mutex::new(None);
        let status = Mutex::new(Status::default());
        let mut sent = Vec::new();
        let failure = run_plan(
            source, &[1], &Mutex::new(vec![1]), false, &switch,
            &Mutex::new(ChordState::default()), &(Mutex::new(false), Condvar::new()),
            &status,
            |data| {
                sent.push(data.to_vec());
                if data == [0x90, 60, 48] {
                    let start = status.lock().unwrap().started.unwrap();
                    *switch.lock().unwrap() = Some(Switch {
                        at: start + Duration::from_millis(20),
                        steps: vec![Step { plan: fill.clone(), looping: false },
                                    Step { plan: back.clone(), looping: false }],
                        fill: None,
                        source_start: start,
                    });
                }
                Ok(())
            },
        );
        assert_eq!(failure, None);
        let sustain_on = sent.iter().position(|data| data == &[0xb0, 64, 127]).unwrap();
        assert_eq!(&sent[sustain_on + 1..sustain_on + 4], &[
            vec![0xb0, 64, 0], vec![0xb0, 123, 0], vec![0x90, 62, 100],
        ]);
        assert!(!sent.contains(&vec![0x90, 64, 100]));
    }

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
            bars: vec![Duration::ZERO],
            beats: vec![Duration::ZERO],
            section: None,
            tempo: None,
            routes: ChannelRoutes::default(),
        }
    }
    #[test]
    fn startup_is_once_before_channel_packets_across_loops_and_stop_releases() {
        let cancel = (Mutex::new(false), Condvar::new());
        let status = Mutex::new(Status::default());
        let mut sent = Vec::new();
        let mut ons = 0;
        let audible = Mutex::new(vec![1u8]);
        let switch = Mutex::new(None);
        let chord = Mutex::new(ChordState::default());
        let failure = run_plan(
            test_plan(),
            &[1],
            &audible,
            true,
            &switch,
            &chord,
            &cancel,
            &status,
            |data| {
                sent.push(data.to_vec());
                if data[0] == 0x90 {
                    ons += 1;
                    if ons == 2 {
                        *cancel.0.lock().unwrap() = true;
                    }
                }
                Ok(())
            },
        );
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
    #[ignore = "requires the local Genos Groove retrigger replay"]
    fn retrigger_clock_matches_groove_countdown_input() {
        #[derive(serde::Deserialize)]
        struct Replay {
            groove: Vec<GrooveCase>,
        }
        #[derive(serde::Deserialize)]
        struct GrooveCase {
            rtr: u8,
            elapsed_steps: Vec<u64>,
            scheduled_steps: Vec<u64>,
            pitch: u8,
        }

        let path = std::env::var_os("GENOS_RTR_WINDOW_REPLAY").unwrap();
        let replay: Replay = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let clock = TempoClock {
            segments: vec![(Duration::ZERO, 0.0, 1_920_000)],
            meter: None,
            fill: false,
            held_seek_sources: 0,
            rhythm_sources: None,
        };
        assert_eq!(replay.groove.len(), 28);
        let mut schedules_crossing_boundary = 0;
        for case in replay.groove {
            // At this synthetic tempo, one 1920-PPQN unit equals one millisecond.
            let elapsed = Duration::from_millis(case.elapsed_steps.iter().sum());
            let scheduled = Duration::from_millis(case.scheduled_steps.iter().sum());
            let in_window = clock.in_root_window(elapsed, 0.0, 0.0);
            let pitch = if in_window && matches!(case.rtr, 2 | 4) { 74 } else { 66 };
            assert_eq!(pitch, case.pitch, "RTR {}, {elapsed:?}", case.rtr);
            if in_window && !clock.in_root_window(scheduled, 0.0, 0.0) {
                schedules_crossing_boundary += 1;
            }
        }
        assert!(schedules_crossing_boundary > 0);
    }

    #[test]
    fn retrigger_window_expires_at_the_scheduled_deadline_without_float_slip() {
        let mut clock = TempoClock {
            segments: vec![(Duration::ZERO, 0.0, 500_000)],
            meter: None,
            fill: false,
            held_seek_sources: 0,
            rhythm_sources: None,
        };
        for base in [0.0, 4.0] {
            for tempo in [500_000, 500_001, 333_333, 428_571] {
                clock.segments[0].2 = tempo;
                for offset in 1_000..1_100 {
                    let start = Duration::from_micros(offset);
                    let changed = base + clock.quarters_at(start);
                    let deadline = start + Duration::from_nanos(tempo * 500);
                    assert!(clock.in_root_window(start, base, changed));
                    assert!(clock.in_root_window(deadline - Duration::from_nanos(1), base, changed));
                    assert!(!clock.in_root_window(deadline, base, changed), "{tempo} {offset}");
                }
            }
        }
    }

    #[test]
    fn timed_mapper_uses_scheduled_root_change_and_note_deadlines() {
        let at = Instant::now();
        let chord = |root| LiveChord {
            root,
            quality: "Major".into(),
            bass: None,
            bass_quality: None,
        };
        let state = Mutex::new(ChordState {
            current: Some(chord(0)),
            pending: Some((Some(chord(2)), at)),
            mapper: Some(Arc::new(|_, _, pitch, chord, to_root| {
                LivePitch {
                    pitch: Some(pitch + if to_root { 10 } else { chord.root }),
                    portamento: true,
                    stop_on_change: false,
                    bass_region: false,
                    held_mask_portamento: false,
                }
            })),
            ..ChordState::default()
        });
        let mut sent = Vec::new();
        let mut live = Live {
            cancel: &(Mutex::new(false), Condvar::new()),
            switch: &Mutex::new(None),
            chord: &state,
            send: |packet| {
                sent.push(packet.to_vec());
                Ok(())
            },
            notes: std::array::from_fn(|_| Vec::new()),
            section: Some(1),
            fill_section: None,
            clock: Some((at, 0.0, TempoClock {
                segments: vec![
                    (Duration::ZERO, 0.0, 500_000),
                    (Duration::from_millis(125), 0.25, 250_000),
                ],
                meter: None,
                fill: false,
                held_seek_sources: 0,
                rhythm_sources: None,
            })),
            routes: ChannelRoutes::default(),
        };
        live.emit(&[0x90, 64, 90], at).unwrap();
        live.apply_chord().unwrap();
        live.emit(&[0x80, 64, 0], at).unwrap();
        for offset in [187_499, 187_500] {
            let deadline = at + Duration::from_micros(offset);
            live.emit(&[0x90, 64, 90], deadline).unwrap();
            live.emit(&[0x80, 64, 0], deadline).unwrap();
        }
        assert_eq!(
            sent,
            [
                [0x90, 64, 90],
                [0xb0, 84, 64],
                [0x90, 74, 90],
                [0x80, 64, 0],
                [0x80, 74, 0],
                [0x90, 74, 90],
                [0x80, 74, 0],
                [0x90, 66, 90],
                [0x80, 66, 0],
            ]
        );
    }

    #[test]
    fn live_stop_releases_once_even_when_conversion_would_keep_the_pitch() {
        let at = Instant::now();
        let chord = |root| LiveChord {
            root,
            quality: "Major".into(),
            bass: None,
            bass_quality: None,
        };
        let state = Mutex::new(ChordState {
            current: Some(chord(0)),
            pending: Some((Some(chord(2)), at)),
            mapper: Some(Arc::new(|_, _, pitch, _, _| LivePitch {
                stop_on_change: true,
                ..Some(pitch).into()
            })),
            ..ChordState::default()
        });
        let mut sent = Vec::new();
        let mut live = Live {
            cancel: &(Mutex::new(false), Condvar::new()),
            switch: &Mutex::new(None),
            chord: &state,
            send: |packet| {
                sent.push(packet.to_vec());
                Ok(())
            },
            notes: std::array::from_fn(|_| Vec::new()),
            section: Some(1),
            fill_section: None,
            clock: None,
            routes: ChannelRoutes::default(),
        };
        live.emit(&[0x90, 64, 90], at).unwrap();
        live.apply_chord().unwrap();
        live.emit(&[0x80, 64, 0], at).unwrap();
        assert_eq!(sent, [[0x90, 64, 90], [0x80, 64, 0]]);
    }

    #[test]
    fn followed_chord_repitches_notes_and_matches_their_offs() {
        let plan = Plan {
            setup: vec![],
            packets: vec![
                Packet {
                    at: Duration::ZERO,
                    data: vec![0x90, 60, 80],
                },
                Packet {
                    at: Duration::from_millis(5),
                    data: vec![0x80, 60, 0],
                },
                Packet {
                    at: Duration::from_millis(5),
                    data: vec![0x98, 36, 90],
                },
            ],
            duration: Duration::from_millis(10),
            bars: vec![Duration::from_millis(10)],
            beats: vec![Duration::from_millis(10)],
            section: Some(1),
            tempo: None,
            routes: ChannelRoutes::default(),
        };
        let cancel = (Mutex::new(false), Condvar::new());
        let status = Mutex::new(Status::default());
        let audible = Mutex::new(vec![1u8, 9]);
        let switch = Mutex::new(None);
        let chord = Mutex::new(ChordState {
            current: Some(LiveChord {
                root: 2,
                quality: "Major".into(),
                bass: None,
                bass_quality: None,
            }),
            mapper: Some(Arc::new(|_, _, pitch, chord, _| Some(pitch + chord.root).into())),
            ..ChordState::default()
        });
        let mut sent = Vec::new();
        run_plan(
            plan,
            &[1, 9],
            &audible,
            false,
            &switch,
            &chord,
            &cancel,
            &status,
            |data| {
                sent.push(data.to_vec());
                Ok(())
            },
        );
        assert_eq!(&sent[..2], &[vec![0x90, 62, 80], vec![0x80, 62, 0]]);
        // Rhythm source channel 9 stays literal.
        assert!(sent.contains(&vec![0x98, 36, 90]));
    }
    #[test]
    fn chord_arriving_mid_note_revoices_the_startup_pitch_on_time() {
        let plan = Plan {
            setup: vec![],
            packets: vec![
                Packet {
                    at: Duration::ZERO,
                    data: vec![0x92, 48, 70],
                },
                Packet {
                    at: Duration::from_millis(60),
                    data: vec![0x82, 48, 0],
                },
            ],
            duration: Duration::from_millis(70),
            bars: vec![Duration::from_millis(70)],
            beats: vec![Duration::from_millis(70)],
            section: Some(1),
            tempo: None,
            routes: ChannelRoutes::default(),
        };
        let cancel = (Mutex::new(false), Condvar::new());
        let status = Mutex::new(Status::default());
        let audible = Mutex::new(vec![3u8]);
        let switch = Mutex::new(None);
        let chord = Mutex::new(ChordState {
            initial: Some(LiveChord {
                root: 0,
                quality: "Genos:34".into(),
                bass: None,
                bass_quality: None,
            }),
            pending: Some((
                Some(LiveChord {
                    root: 5,
                    quality: "Major".into(),
                    bass: None,
                    bass_quality: None,
                }),
                Instant::now() + Duration::from_millis(20),
            )),
            mapper: Some(Arc::new(|_, _, pitch, chord, _| {
                Some(pitch + if chord.quality == "Genos:34" { 1 } else { chord.root }).into()
            })),
            ..ChordState::default()
        });
        let mut sent = Vec::new();
        run_plan(
            plan,
            &[3],
            &audible,
            false,
            &switch,
            &chord,
            &cancel,
            &status,
            |data| {
                sent.push(data.to_vec());
                Ok(())
            },
        );
        assert_eq!(
            &sent[..4],
            &[
                vec![0x92, 49, 70],
                vec![0x82, 49, 0],
                vec![0x92, 53, 70],
                vec![0x82, 53, 0],
            ]
        );
    }
    /// Real transport thread, captured output: a chord reported mid-play changes the notes
    /// from the next beat and keeps changing them across loops.
    #[test]
    fn transport_follows_a_chord_reported_while_playing() {
        let beat = Duration::from_millis(40);
        let packets: Vec<Packet> = (0..4u32)
            .flat_map(|i| {
                [
                    Packet {
                        at: beat * i,
                        data: vec![0x92, 48, 90],
                    },
                    Packet {
                        at: beat * i + Duration::from_millis(10),
                        data: vec![0x82, 48, 0],
                    },
                ]
            })
            .collect();
        let plan = Plan {
            setup: vec![],
            packets,
            duration: beat * 4,
            bars: vec![beat * 4],
            beats: (1..=4).map(|i| beat * i).collect(),
            section: Some(1),
            tempo: None,
            routes: ChannelRoutes::default(),
        };
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let mut transport = Transport::default();
        transport.set_pitch_mapper(Arc::new(|_, _, pitch, chord| Some(pitch + chord.root)));
        transport.spawn_player(plan, vec![3], true, move |data| {
            let _ = tx.send(data.to_vec());
            Ok(())
        });
        thread::sleep(Duration::from_millis(50));
        transport.follow_chord(Some(LiveChord {
            root: 2,
            quality: "Major".into(),
            bass: None,
            bass_quality: None,
        }));
        thread::sleep(Duration::from_millis(400));
        transport.stop();
        let ons: Vec<u8> = rx
            .try_iter()
            .filter(|d| d[0] == 0x92 && d[2] > 0)
            .map(|d| d[1])
            .collect();
        // Literal before the change, then D-major pitches (48 + 2) to the end of the run.
        assert_eq!(ons[0], 48, "{ons:?}");
        let first_changed = ons.iter().position(|p| *p == 50).expect("chord followed");
        assert!(first_changed >= 2, "chord must wait for a beat: {ons:?}");
        assert!(ons[first_changed..].iter().all(|p| *p == 50), "{ons:?}");
    }
    #[test]
    fn keyboard_input_bytes_become_a_held_chord_and_an_xf_chord() {
        let held = Arc::new(Mutex::new(super::HeldNoteState::default()));
        for note in [48u8, 52, 55] {
            update_held_notes(&held, &[0x93, note, 90]);
        }
        let notes: Vec<u8> = held.lock().unwrap().keys.iter().map(|(_, n)| *n).collect();
        let hit = crate::chord_preview::recognize(&notes, None).unwrap();
        assert_eq!((hit.root, hit.quality.as_str()), (0, "Major"));
        update_held_notes(&held, &[0x83, 52, 0]);
        assert_eq!(held.lock().unwrap().keys.len(), 2);
        let xf = Mutex::new(XfState::default());
        update_xf_chord(&xf, &[0xf0, 0x43, 0x7e, 0x00, 0x35, 0x08, 0x7f, 0x7f, 0xf7]);
        update_xf_chord(&xf, &[0xf0, 0x43, 0x7e, 0x02, 0x31, 0, 0x32, 8, 0xf7]);
        let st = xf.lock().unwrap();
        assert_eq!(st.seq, 2);
        assert_eq!(st.chord, Some(LiveChord {
            root: 0,
            quality: "Major".into(),
            bass: Some(2),
            bass_quality: Some("Minor".into()),
        }));
    }
    #[test]
    fn cancelled_or_failed_startup_never_sends_notes_and_still_attempts_release() {
        for cancelled in [false, true] {
            let cancel = (Mutex::new(cancelled), Condvar::new());
            let status = Mutex::new(Status::default());
            let mut sent = Vec::new();
            let audible = Mutex::new(vec![1u8]);
            let switch = Mutex::new(None);
            let chord = Mutex::new(ChordState::default());
            let failure = run_plan(
                test_plan(),
                &[1],
                &audible,
                false,
                &switch,
                &chord,
                &cancel,
                &status,
                |data| {
                    sent.push(data.to_vec());
                    if data[0] == 0xf0 {
                        Err("setup failed".into())
                    } else {
                        Ok(())
                    }
                },
            );
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

#[cfg(test)]
mod input_connect_tests {
    use super::input_connect_is_transient;
    use midir::ConnectErrorKind;

    #[test]
    fn generic_windows_open_failure_is_retried() {
        assert!(input_connect_is_transient(ConnectErrorKind::Other(
            "could not create Windows MM MIDI input port"
        )));
    }

    #[test]
    fn port_held_by_another_program_is_not_retried() {
        assert!(!input_connect_is_transient(ConnectErrorKind::Other(
            "could not create Windows MM MIDI input port (MMSYSERR_ALLOCATED)"
        )));
    }

    #[test]
    fn invalid_port_is_not_retried() {
        assert!(!input_connect_is_transient(ConnectErrorKind::InvalidPort));
    }
}

#[cfg(test)]
mod channel_route_tests {
    use super::*;

    fn routes(pairs: &[(u8, u8)]) -> ChannelRoutes {
        let mut routes = ChannelRoutes::default();
        for &(source, destination) in pairs {
            routes.set(source, destination);
        }
        routes
    }

    #[test]
    fn channel_messages_move_to_the_destination_part() {
        let routes = routes(&[(6, 14)]);
        assert_eq!(&*routes.apply(&[0x95, 36, 90]), &[0x9d, 36, 90]);
        assert_eq!(&*routes.apply(&[0x85, 36, 0]), &[0x8d, 36, 0]);
        assert_eq!(&*routes.apply(&[0xb5, 7, 100]), &[0xbd, 7, 100]);
        assert_eq!(&*routes.apply(&[0xc5, 4]), &[0xcd, 4]);
    }

    #[test]
    fn unrouted_channels_and_system_messages_pass_through() {
        let routes = routes(&[(6, 14)]);
        assert_eq!(&*routes.apply(&[0x9a, 60, 90]), &[0x9a, 60, 90]);
        let sysex = [0xf0, 0x43, 0x10, 0x4c, 0, 0, 0x7e, 0, 0xf7];
        assert_eq!(&*routes.apply(&sysex), &sysex);
        assert_eq!(ChannelRoutes::default().destination(16), 16);
    }

    #[test]
    fn invalid_destinations_are_ignored() {
        let mut routes = ChannelRoutes::default();
        routes.set(6, 0);
        routes.set(6, 17);
        routes.set(0, 14);
        assert_eq!(routes, ChannelRoutes::default());
    }

    #[test]
    fn sources_sharing_a_part_do_not_cut_each_others_notes() {
        let ms = Duration::from_millis;
        let plan = Plan {
            setup: vec![],
            packets: vec![
                Packet { at: ms(0), data: vec![0x94, 36, 90] },
                Packet { at: ms(2), data: vec![0x95, 36, 90] },
                Packet { at: ms(4), data: vec![0x84, 36, 0] },
                Packet { at: ms(6), data: vec![0x85, 36, 0] },
            ],
            duration: ms(10),
            bars: vec![ms(10)],
            beats: vec![ms(10)],
            section: Some(1),
            tempo: None,
            routes: routes(&[(5, 14), (6, 14)]),
        };
        let mut sent = Vec::new();
        let failure = run_plan(
            plan,
            &[5, 6],
            &Mutex::new(vec![5, 6]),
            false,
            &Mutex::new(None),
            &Mutex::new(ChordState::default()),
            &(Mutex::new(false), Condvar::new()),
            &Mutex::new(Status::default()),
            |data| {
                sent.push(data.to_vec());
                Ok(())
            },
        );
        assert_eq!(failure, None);
        // One key sounds on part 14 until the last source releases it.
        assert_eq!(&sent[..2], &[vec![0x9d, 36, 90], vec![0x8d, 36, 0]]);
    }

    #[test]
    fn player_sends_and_releases_on_the_destination_part() {
        let plan = Plan {
            setup: vec![vec![0xc5, 4]],
            packets: vec![
                Packet { at: Duration::ZERO, data: vec![0x95, 36, 90] },
                Packet { at: Duration::from_millis(5), data: vec![0x85, 36, 0] },
            ],
            duration: Duration::from_millis(10),
            bars: vec![Duration::from_millis(10)],
            beats: vec![Duration::from_millis(10)],
            section: Some(1),
            tempo: None,
            routes: routes(&[(6, 14)]),
        };
        let mut sent = Vec::new();
        let failure = run_plan(
            plan,
            &[6],
            &Mutex::new(vec![6]),
            false,
            &Mutex::new(None),
            &Mutex::new(ChordState::default()),
            &(Mutex::new(false), Condvar::new()),
            &Mutex::new(Status::default()),
            |data| {
                sent.push(data.to_vec());
                Ok(())
            },
        );
        assert_eq!(failure, None);
        assert_eq!(&sent[..3], &[vec![0xcd, 4], vec![0x9d, 36, 90], vec![0x8d, 36, 0]]);
        // The final release covers the destination part; channel 6 belongs to another part
        // of the instrument and is left alone.
        assert!(sent.contains(&vec![0xbd, 123, 0]));
        assert!(!sent.iter().any(|data| data[0] & 0x0f == 0x05), "{sent:?}");
    }
}

#[cfg(test)]
mod output_channel_tests {
    use super::*;

    #[test]
    fn muted_sources_map_to_the_parts_they_sound_on() {
        let transport = Transport::default();
        let mut routes = ChannelRoutes::default();
        routes.set(6, 14);
        transport.status.lock().unwrap().routes = routes;
        assert_eq!(transport.output_channels(&[6, 9, 0, 17]), vec![9, 14]);
    }
}
