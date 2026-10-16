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
#[derive(Clone, Debug)]
pub struct Packet {
    pub at: Duration,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Plan {
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
        packets,
        duration: at(s.end) - start,
    })
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
        self.stop();
        let plan = plan(doc, section, &channels)?;
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
            let mut failure = None;
            'play: loop {
                let start = Instant::now();
                status.lock().unwrap_or_else(|p| p.into_inner()).started = Some(start);
                for packet in &plan.packets {
                    if wait_until(&cancel, start + packet.at) {
                        break 'play;
                    }
                    if let Err(e) = connection.send(&packet.data) {
                        failure = Some(e.to_string());
                        break 'play;
                    }
                    status.lock().unwrap_or_else(|p| p.into_inner()).elapsed_ms =
                        packet.at.as_millis() as u64;
                }
                if wait_until(&cancel, start + plan.duration) || !looping {
                    break;
                }
            }
            // Sustain release + explicit offs + all-notes/all-sound-off on selected route.
            for ch in channels {
                let s = ch - 1;
                for data in [
                    vec![0xb0 | s, 64, 0],
                    vec![0xb0 | s, 123, 0],
                    vec![0xb0 | s, 120, 0],
                ] {
                    if let Err(e) = connection.send(&data) {
                        failure.get_or_insert(e.to_string());
                    }
                }
                for pitch in 0..128 {
                    if let Err(e) = connection.send(&[0x80 | s, pitch, 0]) {
                        failure.get_or_insert(e.to_string());
                    }
                }
            }
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
