//! Opt-in hardware oracle (never ordinary CI).
//!
//! Plays one Style section on the Genos arranger for a chord, captures the parts the
//! instrument transmits on channels 9-16, and compares them with the app's own chord
//! conversion of the same file (`midi::plan` + `live_preview::follow_routed_pitch`).
//!
//! Requirements on the instrument: the same Style file selected, ACMP on, Chord System
//! Exclusive receive on, and Style parts transmitted on channels 9-16.
//!
//! Usage: arranger_compare <sty> <section-label> <exact input> <exact output>
//!        <root 0-11> <quality> [loops 1..4]
use genos_core::{document::Document, live_preview, midi};
use midir::{Ignore, MidiInput, MidiOutput};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    sync::mpsc,
    time::{Duration, Instant},
};

type BoxError = Box<dyn std::error::Error>;

/// One note-on: milliseconds from the first note-on, channel 1..16, pitch.
#[derive(Clone, Copy, Debug)]
struct NoteOn {
    ms: f64,
    channel: u8,
    pitch: u8,
}

const STYLE_CHANNELS: [u8; 8] = [9, 10, 11, 12, 13, 14, 15, 16];
const RHYTHM_CHANNELS: [u8; 2] = [9, 10];

/// XF root byte: high nibble accidental (2 flat, 3 natural, 4 sharp), low nibble letter.
fn xf_root_byte(root: u8) -> u8 {
    [
        0x31, 0x41, 0x32, 0x23, 0x33, 0x34, 0x44, 0x35, 0x26, 0x36, 0x27, 0x37,
    ][usize::from(root % 12)]
}

/// Genos chord type IDs for the qualities the app can name without firmware tables.
fn xf_type(quality: &str) -> Option<u8> {
    Some(match quality {
        "Major" => 0,
        "Maj7" => 2,
        "aug" => 7,
        "Minor" => 8,
        "m7" => 10,
        "m7b5" => 11,
        "dim" => 17,
        "7" => 19,
        "sus4" => 32,
        "sus2" => 33,
        other => other.strip_prefix("Genos:")?.parse().ok()?,
    })
}

fn app_notes(
    doc: &Document,
    section: u64,
    chord: &midi::LiveChord,
) -> Result<(Vec<NoteOn>, Duration), BoxError> {
    // Every source, as the app plays a Style: each one sounds on its CASM destination part.
    let sources: Vec<u8> = (1..=16).collect();
    let plan = midi::plan(doc, section, &sources)?;
    let notes = plan
        .packets
        .iter()
        .filter(|p| p.data.len() == 3 && p.data[0] & 0xf0 == 0x90 && p.data[2] > 0)
        .filter_map(|p| {
            let source_channel = (p.data[0] & 0x0f) + 1;
            let channel = plan.routes.destination(source_channel);
            let source = p.data[1];
            let pitch = if RHYTHM_CHANNELS.contains(&channel) {
                Some(source)
            } else {
                live_preview::follow_routed_pitch(doc, section, source_channel, source, chord, false)
                    .pitch
            };
            pitch.map(|pitch| NoteOn {
                ms: p.at.as_secs_f64() * 1000.0,
                channel,
                pitch,
            })
        })
        .collect();
    Ok((notes, plan.duration))
}

/// Genos Section Control switch number for a section label (Intro, Main, Ending).
fn section_switch(label: &str) -> Option<u8> {
    let (kind, letter) = label.rsplit_once(' ')?;
    let index = match letter {
        "A" => 0,
        "B" => 1,
        "C" => 2,
        "D" => 3,
        _ => return None,
    };
    match kind {
        "Intro" if index < 3 => Some(index),
        "Main" => Some(0x08 + index),
        "Ending" if index < 3 => Some(0x20 + index),
        _ => None,
    }
}

/// What the Genos is given to follow: a chord message, or held keys its own chord
/// detection reads (Chord Detect on for MIDI channel 1).
enum Stimulus<'a> {
    Chord(&'a midi::LiveChord),
    Keys(&'a [u8]),
}

fn genos_notes(
    in_name: &str,
    out_name: &str,
    switch: u8,
    stimulus: Stimulus<'_>,
    capture: Duration,
) -> Result<Vec<NoteOn>, BoxError> {
    let mut input = MidiInput::new("GenosStyleStudio arranger compare")?;
    input.ignore(Ignore::All);
    let in_port = input
        .ports()
        .into_iter()
        .find(|p| input.port_name(p).is_ok_and(|n| n == in_name))
        .ok_or("input port not found")?;
    let (sender, receiver) = mpsc::sync_channel(65536);
    // Phase markers on stderr locate a stall inside the MIDI stack if one recurs.
    let phase_clock = Instant::now();
    let phase = |name: &str| eprintln!("phase {:>6} ms {name}", phase_clock.elapsed().as_millis());
    phase("open input");
    let connection = midi::connect_input(input, &in_port, "GenosStyleStudio compare in", || {
        let sender = sender.clone();
        move |_, data: &[u8], _: &mut ()| {
            let _ = sender.try_send((Instant::now(), data.to_vec()));
        }
    })?;
    let output = MidiOutput::new("GenosStyleStudio arranger compare out")?;
    let out_port = output
        .ports()
        .into_iter()
        .find(|p| output.port_name(p).is_ok_and(|n| n == out_name))
        .ok_or("output port not found")?;
    phase("open output");
    let mut conn = output.connect(&out_port, "GenosStyleStudio compare send")?;
    phase("send setup");
    match stimulus {
        Stimulus::Chord(chord) => {
            let chord_type = xf_type(&chord.quality).ok_or("quality has no Genos chord type")?;
            conn.send(&[
                0xf0,
                0x43,
                0x7e,
                0x02,
                xf_root_byte(chord.root),
                chord_type,
                0x7f,
                0x7f,
                0xf7,
            ])?;
        }
        Stimulus::Keys(keys) => {
            // Clear keys an interrupted earlier run may have left held on the detect channel.
            for key in 0..128u8 {
                conn.send(&[0x80, key, 0])?;
            }
            // Lowest key last: with Bass detect on, the Genos takes the bass from the most
            // recent key, so this keeps the bass on the lowest note as a player's hand would.
            let mut ordered = keys.to_vec();
            ordered.sort_unstable_by(|a, b| b.cmp(a));
            for key in ordered {
                conn.send(&[0x90, key, 80])?;
            }
        }
    }
    conn.send(&[0xf0, 0x43, 0x7e, 0x00, switch, 0x7f, 0xf7])?;
    std::thread::sleep(Duration::from_millis(400));
    conn.send(&[0xf0, 0x43, 0x60, 0x7a, 0xf7])?;
    let deadline = Instant::now() + capture;
    let mut raw = Vec::new();
    while Instant::now() < deadline {
        if let Ok(event) = receiver.recv_timeout(Duration::from_millis(20)) {
            raw.push(event);
        }
    }
    phase("send stop");
    conn.send(&[0xf0, 0x43, 0x60, 0x7d, 0xf7])?;
    if let Stimulus::Keys(keys) = stimulus {
        for &key in keys {
            conn.send(&[0x80, key, 0])?;
        }
    }
    std::thread::sleep(Duration::from_millis(400));
    phase("close output");
    drop(conn);
    phase("close input");
    drop(connection);
    phase("closed");
    let ons: Vec<_> = raw
        .into_iter()
        .filter(|(_, d)| d.len() == 3 && d[0] & 0xf0 == 0x90 && d[2] > 0)
        .collect();
    let Some(t0) = ons.first().map(|(t, _)| *t) else {
        return Ok(Vec::new());
    };
    Ok(ons
        .into_iter()
        .map(|(t, d)| NoteOn {
            ms: t.saturating_duration_since(t0).as_secs_f64() * 1000.0,
            channel: (d[0] & 0x0f) + 1,
            pitch: d[1],
        })
        .collect())
}

fn pitch_counts(notes: &[NoteOn], channel: u8) -> BTreeMap<u8, usize> {
    notes
        .iter()
        .filter(|n| n.channel == channel)
        .fold(BTreeMap::new(), |mut counts, n| {
            *counts.entry(n.pitch).or_default() += 1;
            counts
        })
}

/// Length of the longest common subsequence of two pitch sequences.
fn lcs(a: &[u8], b: &[u8]) -> usize {
    let mut row = vec![0usize; b.len() + 1];
    for x in a {
        let mut diagonal = 0;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if x == y {
                diagonal + 1
            } else {
                row[j + 1].max(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

/// Up to 64 `[ms, pitch]` pairs for one channel, for locating a mismatch.
fn timed(notes: &[NoteOn], channel: u8) -> Vec<(u64, u8)> {
    notes
        .iter()
        .filter(|n| n.channel == channel)
        .take(64)
        .map(|n| (n.ms.round() as u64, n.pitch))
        .collect()
}

fn compare_channel(app: &[NoteOn], genos: &[NoteOn], channel: u8) -> serde_json::Value {
    let app_counts = pitch_counts(app, channel);
    let genos_counts = pitch_counts(genos, channel);
    let mut app_only = BTreeMap::new();
    let mut genos_only = BTreeMap::new();
    for pitch in app_counts.keys().chain(genos_counts.keys()) {
        let a = app_counts.get(pitch).copied().unwrap_or(0);
        let g = genos_counts.get(pitch).copied().unwrap_or(0);
        if a > g {
            app_only.insert(*pitch, a - g);
        } else if g > a {
            genos_only.insert(*pitch, g - a);
        }
    }
    let seq = |notes: &[NoteOn]| -> Vec<u8> {
        let mut v: Vec<_> = notes
            .iter()
            .filter(|n| n.channel == channel)
            .copied()
            .collect();
        // Same-onset notes arrive in arbitrary order over BLE; order chords by pitch.
        v.sort_by(|x, y| {
            ((x.ms / 40.0).round() as i64, x.pitch).cmp(&((y.ms / 40.0).round() as i64, y.pitch))
        });
        v.into_iter().map(|n| n.pitch).collect()
    };
    let (a, g) = (seq(app), seq(genos));
    let arrival = |notes: &[NoteOn]| -> Vec<u8> {
        notes.iter().filter(|n| n.channel == channel).map(|n| n.pitch).collect()
    };
    // BLE delivery can reorder near-simultaneous notes; score the better of both orders.
    let common = lcs(&a, &g).max(lcs(&arrival(app), &arrival(genos)));
    json!({
        "channel": channel,
        "appNotes": a.len(),
        "genosNotes": g.len(),
        "orderedMatch": if a.len().max(g.len()) == 0 { 1.0 } else { common as f64 / a.len().max(g.len()) as f64 },
        "appOnlyPitches": app_only,
        "genosOnlyPitches": genos_only,
        "appHead": a.iter().take(16).collect::<Vec<_>>(),
        "genosHead": g.iter().take(16).collect::<Vec<_>>(),
        "appTimed": timed(app, channel),
        "genosTimed": timed(genos, channel),
    })
}

type Rendering = BTreeMap<u8, BTreeMap<u8, usize>>;

/// Per-channel pitch counts of one loop, offset from the first note-on.
fn rendering(notes: &[NoteOn], window: f64) -> Rendering {
    let t0 = notes.first().map_or(0.0, |n| n.ms);
    notes
        .iter()
        .filter(|n| n.ms - t0 < window)
        .fold(BTreeMap::new(), |mut channels: Rendering, n| {
            *channels.entry(n.channel).or_default().entry(n.pitch).or_default() += 1;
            channels
        })
}

/// Chord recognition check: the Genos detects the held keys and plays the section; the
/// chords whose app rendering equals that output are what the Genos recognized.
fn detect(
    doc: &Document,
    section: u64,
    switch: u8,
    in_name: &str,
    out_name: &str,
    keys: &[u8],
) -> Result<(), BoxError> {
    let firmware = std::env::var_os("GENOS_NTT_ELF").is_some()
        || std::env::var_os("GENOS_NTT_PRG").is_some();
    let mut qualities: Vec<String> = ["Major", "Minor", "7", "Maj7", "m7", "dim", "aug", "sus4", "sus2", "m7b5"]
        .iter()
        .map(|q| q.to_string())
        .collect();
    if firmware {
        qualities.extend((0..59).map(|id| format!("Genos:{id}")));
    }
    let chord = |root: u8, quality: &str| midi::LiveChord {
        root,
        quality: quality.into(),
        bass: None,
        bass_quality: None,
    };
    let (_, duration) = app_notes(doc, section, &chord(0, "Major"))?;
    let window = duration.as_secs_f64() * 1000.0 - 60.0;
    let genos = genos_notes(
        in_name,
        out_name,
        switch,
        Stimulus::Keys(keys),
        duration + Duration::from_millis(1500),
    )?;
    // Chord quality and root are judged on the upper parts; the bass part (11) also depends
    // on the instrument's Bass detect setting and is reported separately.
    let upper = |mut r: Rendering| {
        r.remove(&11);
        r
    };
    let observed = rendering(&genos, window);
    let observed_upper = upper(observed.clone());
    let mut matching = Vec::new();
    for root in 0..12 {
        for quality in &qualities {
            let (notes, _) = app_notes(doc, section, &chord(root, quality))?;
            if upper(rendering(&notes, window)) == observed_upper {
                matching.push(format!("{}:{quality}", root));
            }
        }
    }
    let recognized = genos_core::chord_preview::recognize(keys, None);
    let recognized_matches = recognized.as_ref().is_some_and(|r| {
        app_notes(doc, section, &chord(r.root, &r.quality))
            .is_ok_and(|(notes, _)| upper(rendering(&notes, window)) == observed_upper)
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "keys": keys,
            "genosNoteOnsCaptured": genos.len(),
            "appRecognized": recognized,
            "appRecognitionMatchesGenos": recognized_matches,
            "genosEquivalentChords": matching,
            "observed": observed,
            "appRecognizedRendering": recognized.as_ref().and_then(|r| {
                app_notes(doc, section, &chord(r.root, &r.quality)).ok()
                    .map(|(notes, _)| rendering(&notes, window))
            }),
        }))?
    );
    Ok(())
}

fn main() -> Result<(), BoxError> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(6..=7).contains(&args.len()) {
        return Err("usage: arranger_compare <sty> <section-label> <input> <output> <root 0-11> <quality> [loops 1..4]".into());
    }
    let doc = Document::parse(&fs::read(&args[0])?)?;
    let section = doc
        .sections
        .iter()
        .find(|s| s.label == args[1])
        .ok_or("section not found")?
        .id;
    let switch = section_switch(&args[1]).ok_or("only Intro, Main and Ending sections can be selected")?;
    if let Some(keys) = args[5].strip_prefix("keys:") {
        let keys = keys
            .split(',')
            .map(|k| k.trim().parse::<u8>())
            .collect::<Result<Vec<_>, _>>()?;
        if keys.is_empty() || keys.iter().any(|&k| k > 127) {
            return Err("keys must be MIDI note numbers 0..127".into());
        }
        return detect(&doc, section, switch, &args[2], &args[3], &keys);
    }
    let root: u8 = args[4].parse()?;
    if root > 11 {
        return Err("root must be 0..11".into());
    }
    let loops: u32 = args.get(6).map(|s| s.parse()).transpose()?.unwrap_or(1);
    if !(1..=4).contains(&loops) {
        return Err("loops must be 1..4".into());
    }
    let chord = midi::LiveChord {
        root,
        quality: args[5].clone(),
        bass: None,
        bass_quality: None,
    };
    let (app, duration) = app_notes(&doc, section, &chord)?;
    let app_t0 = app.first().map_or(0.0, |n| n.ms);
    let app: Vec<_> = app
        .iter()
        .map(|n| NoteOn {
            ms: n.ms - app_t0,
            ..*n
        })
        .collect();
    let capture = duration * loops + Duration::from_millis(1500);
    let genos = genos_notes(&args[2], &args[3], switch, Stimulus::Chord(&chord), capture)?;
    // Compare the first complete loop only; later loops and the stop tail are excluded.
    let window = duration.as_secs_f64() * 1000.0 - 60.0;
    // Apply the same cut to both sides; notes in the final 60 ms are reported separately
    // because BLE delivery jitter can move them across the loop boundary.
    let genos_loop: Vec<_> = genos.iter().copied().filter(|n| n.ms < window).collect();
    let app_loop: Vec<_> = app.iter().copied().filter(|n| n.ms < window).collect();
    let boundary = |notes: &[NoteOn]| -> Vec<(u8, u8, u64)> {
        notes
            .iter()
            .filter(|n| n.ms >= window && n.ms < window + 160.0)
            .map(|n| (n.channel, n.pitch, n.ms.round() as u64))
            .collect()
    };
    let channels: Vec<_> = STYLE_CHANNELS
        .iter()
        .map(|&c| compare_channel(&app_loop, &genos_loop, c))
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "style": args[0],
            "section": args[1],
            "chord": { "root": root, "quality": args[5] },
            "firmwareTables": std::env::var_os("GENOS_NTT_ELF").is_some() || std::env::var_os("GENOS_NTT_PRG").is_some(),
            "loopMs": duration.as_millis() as u64,
            "genosNoteOnsCaptured": genos.len(),
            "channels": channels,
            "appBoundary": boundary(&app),
            "genosBoundary": boundary(&genos),
        }))?
    );
    Ok(())
}
