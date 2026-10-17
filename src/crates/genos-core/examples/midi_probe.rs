//! Opt-in diagnostics. Scan is read-only; listen opens only an exact input name.
use genos_core::{capture::MidiCapture, document::Document, genos_system_effect, midi};
use midir::{Ignore, MidiInput};
use serde_json::json;
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn release_document() -> genos_core::Result<Document> {
    use genos_core::smf::{self, Chunk, Event};
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
            data: vec![0x90, 60, 80],
        },
        Event {
            id: 3,
            tick: 9600,
            order: 3,
            data: vec![0x80, 60, 0],
        },
        Event {
            id: 4,
            tick: 9600,
            order: 4,
            data: vec![0xff, 0x2f, 0],
        },
    ];
    Document::parse(&smf::pack(&[
        Chunk {
            tag: *b"MThd",
            data: vec![0, 0, 0, 1, 1, 224],
        },
        Chunk {
            tag: *b"MTrk",
            data: smf::encode_track(&events)?,
        },
    ])?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn release_check_really_schedules_a_held_note_before_cancellation() {
        let document = super::release_document().unwrap();
        let plan = genos_core::midi::plan(&document, document.sections[0].id, &[1]).unwrap();
        assert!(
            plan.packets
                .iter()
                .any(|p| p.at.is_zero() && p.data == [0x90, 60, 80])
        );
        assert!(
            plan.packets
                .iter()
                .any(|p| p.at == std::time::Duration::from_secs(10) && p.data == [0x80, 60, 0])
        );
    }
    #[test]
    fn old_unpaired_release_fixture_is_not_a_real_playback_test() {
        let mut document = super::release_document().unwrap();
        document.style.tracks[0].retain(|e| !e.is_off());
        let document = genos_core::document::Document::from_style(document.style).unwrap();
        let plan = genos_core::midi::plan(&document, document.sections[0].id, &[1]).unwrap();
        assert!(!plan.packets.iter().any(|p| p.data[0] & 0xf0 == 0x90));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "scan" {
        println!("{}", serde_json::to_string_pretty(&midi::devices()?)?);
        return Ok(());
    }
    if args.len() == 2 && args[0] == "sections" {
        let bytes = fs::read(&args[1])?;
        let doc = Document::parse(&bytes)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "stylePath": args[1],
                "sectionCount": doc.sections.len(),
                "sections": doc.sections.iter().map(|s| json!({
                    "id": s.id,
                    "label": s.label,
                    "start": s.start,
                    "end": s.end
                })).collect::<Vec<_>>(),
                "claims": { "fullStyleAcceptance": false, "g7Closed": false }
            }))?
        );
        return Ok(());
    }
    if args.len() >= 3 && args[0] == "capture" {
        let in_name = &args[1];
        let seconds: u64 = args[2].parse()?;
        if !(1..=120).contains(&seconds) {
            return Err("capture duration must be 1..120 seconds".into());
        }
        let out_path = args.get(3).map(|s| s.as_str());
        let mut cap = MidiCapture::default();
        eprintln!(
            "CAPTURE: {} for {} seconds (Type-0 SMF export; not Style/CASM build)",
            in_name, seconds
        );
        cap.start(in_name)?;
        let _ = cap.add_marker("capture-start");
        std::thread::sleep(Duration::from_secs(seconds));
        let _ = cap.add_marker("capture-end");
        cap.stop();
        let snap = cap.snapshot();
        let smf = cap.to_smf_bytes()?;
        let mut written = None;
        if let Some(path) = out_path {
            genos_core::project::write_new(std::path::Path::new(path), &smf)?;
            written = Some(path.to_string());
        }
        let note_ons = snap
            .events
            .iter()
            .filter(|e| e.data.len() == 3 && e.data[0] & 0xf0 == 0x90 && e.data[2] > 0)
            .count();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "result": "CAPTURE_PILOT_COMPLETED",
                "input": in_name,
                "durationSec": seconds,
                "eventCount": snap.event_count,
                "noteOns": note_ons,
                "markers": snap.markers,
                "smfBytes": smf.len(),
                "written": written,
                "claims": {
                    "rawCaptureObserved": note_ons > 0,
                    "styleBuildFromCapture": false,
                    "f027Closed": false,
                    "g7Closed": false
                },
                "limitations": [
                    "Raw Type-0 MIDI only — does not analyze/build Styles (F027 gated).",
                    "Stamps are host capture clock at 120 BPM mapping, not wire latency."
                ]
            }))?
        );
        return Ok(());
    }
    if args.len() == 2 && args[0] == "notes" {
        let devices = midi::devices()?;
        let matches: Vec<_> = devices
            .outputs
            .iter()
            .filter(|p| p.name == args[1])
            .collect();
        if matches.len() != 1 {
            return Err("exactly one matching output is required; no fallback".into());
        }
        let port = matches[0];
        let mut events = vec![];
        for (tick, data) in [
            (0, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']),
            (0, vec![0x90, 60, 48]),
            (240, vec![0x80, 60, 0]),
            (480, vec![0x90, 64, 48]),
            (720, vec![0x80, 64, 0]),
            (960, vec![0xff, 0x2f, 0]),
        ] {
            let id = events.len() as u64 + 1;
            events.push(genos_core::smf::Event {
                id,
                tick,
                order: id,
                data,
            });
        }
        let chunks = [
            genos_core::smf::Chunk {
                tag: *b"MThd",
                data: vec![0, 0, 0, 1, 1, 224],
            },
            genos_core::smf::Chunk {
                tag: *b"MTrk",
                data: genos_core::smf::encode_track(&events)?,
            },
        ];
        let document = genos_core::document::Document::parse(&genos_core::smf::pack(&chunks)?)?;
        let mut transport = midi::Transport::default();
        eprintln!(
            "OUTPUT TEST: {} — two velocity-48 notes on channel 1 in two seconds",
            port.name
        );
        std::thread::sleep(Duration::from_secs(2));
        transport.play(
            &document,
            document.sections[0].id,
            vec![1],
            port.index,
            port.name.clone(),
            false,
        )?;
        let started = Instant::now();
        while transport.status().playing && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let status = transport.status();
        transport.stop();
        if status.playing || status.error.is_some() {
            return Err(format!("output did not complete cleanly: {:?}", status.error).into());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"result":"HARDWARE OUTPUT SEND COMPLETED","output":port.name,"notes":[60,64],"velocity":48,"channel":1,"elapsedMs":started.elapsed().as_millis(),"sendError":status.error,"limitations":["Application send completion, not MIDI wire timing.","Audible keyboard result requires user observation.","Transport releases channel sustain/notes/sound on completion; no bank/program/SysEx is sent."]})
            )?
        );
        return Ok(());
    }
    // system-effect <exact output> <reverb|chorus> <msb> <lsb> [return] [hold-ms]
    if args.len() >= 5 && args[0] == "system-effect" {
        let devices = midi::devices()?;
        let matches: Vec<_> = devices
            .outputs
            .iter()
            .filter(|p| p.name == args[1])
            .collect();
        if matches.len() != 1 {
            return Err("exactly one matching output is required; no fallback".into());
        }
        let port = matches[0];
        let block = args[2].as_str();
        let msb: u8 = args[3].parse()?;
        let lsb: u8 = args[4].parse()?;
        let return_level: Option<u8> = args.get(5).map(|s| s.parse()).transpose()?;
        let hold_ms: u64 = args.get(6).map(|s| s.parse()).transpose()?.unwrap_or(1200);
        if !(20..=2000).contains(&hold_ms) {
            return Err("effect audition hold must be 20..2000 ms".into());
        }
        let started = Instant::now();
        eprintln!(
            "SYSTEM EFFECT: {} block={} msb={} lsb={} return={:?} hold={}ms (CC91/93=127, ch1)",
            port.name, block, msb, lsb, return_level, hold_ms
        );
        let applied = genos_system_effect::apply_system_effect(
            port.index,
            &port.name,
            block,
            Some(msb),
            Some(lsb),
            return_level,
        )?;
        // F020 mixer CCs: raise part reverb/chorus send so system return is audible.
        {
            let output = midir::MidiOutput::new("GenosStyleStudio effect send")?;
            let ports = output.ports();
            let p = ports.get(port.index).ok_or("output disconnected")?;
            if output.port_name(p)? != port.name {
                return Err("device list changed".into());
            }
            let mut conn = output.connect(p, "GenosStyleStudio effect send")?;
            conn.send(&[0xB0, 91, 127])?; // CC91 reverb send
            conn.send(&[0xB0, 93, 127])?; // CC93 chorus send
            drop(conn);
        }
        std::thread::sleep(Duration::from_millis(100));
        midi::test_note(port.index, &port.name, 1, 60, 96, hold_ms)?;
        std::thread::sleep(Duration::from_millis(800)); // hear reverb tail
        midi::panic_release(port.index, &port.name, &[1])?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "result": "SYSTEM_EFFECT_SEND_COMPLETED",
                "output": port.name,
                "block": applied.block,
                "msb": msb,
                "lsb": lsb,
                "returnLevel": return_level,
                "packetCount": applied.packet_count,
                "packets": applied.packets,
                "partSends": {"cc91": 127, "cc93": 127, "channel": 1},
                "audition": {"channel": 1, "note": 60, "velocity": 96, "holdMs": hold_ms, "tailMs": 800},
                "elapsedMs": started.elapsed().as_millis(),
                "claims": {
                    "sysexFramingVerified": true,
                    "hardwareAudibleConfirmed": false,
                    "g6Closed": false
                },
                "limitations": [
                    "Packets match ProbeService/EFFECT1 unit oracles; midir reported send success.",
                    "Audible reverb/chorus change on Genos requires user observation.",
                    "Does not qualify variation, Parameter List, or insertion routing."
                ]
            }))?
        );
        return Ok(());
    }
    if args.len() >= 4 && args[0] == "section" {
        let path = &args[1];
        let section_label = &args[2];
        let output_name = &args[3];
        let max_ms: u64 = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(3500);
        if !(500..=60000).contains(&max_ms) {
            return Err("section max duration must be 500..60000 ms".into());
        }
        let devices = midi::devices()?;
        let matches: Vec<_> = devices
            .outputs
            .iter()
            .filter(|p| p.name == *output_name)
            .collect();
        if matches.len() != 1 {
            return Err("exactly one matching output is required; no fallback".into());
        }
        let port = matches[0];
        let bytes = std::fs::read(path)?;
        let document = genos_core::document::Document::parse(&bytes)?;
        let section = document
            .sections
            .iter()
            .find(|s| s.label.eq_ignore_ascii_case(section_label))
            .ok_or_else(|| format!("section label not found: {section_label}"))?;
        let mut channels: Vec<u8> = document
            .notes
            .iter()
            .filter(|n| n.start >= section.start && n.start < section.end)
            .map(|n| n.channel)
            .collect();
        channels.sort_unstable();
        channels.dedup();
        if channels.is_empty() {
            return Err("section has no paired notes to play".into());
        }
        let note_count = document
            .notes
            .iter()
            .filter(|n| n.start >= section.start && n.start < section.end)
            .count();
        eprintln!(
            "SECTION PLAY: {} / {} → {} ({} notes, channels {:?}, cancel after {}ms)",
            path, section.label, port.name, note_count, channels, max_ms
        );
        let mut transport = midi::Transport::default();
        transport.play(
            &document,
            section.id,
            channels.clone(),
            port.index,
            port.name.clone(),
            false,
        )?;
        let started = Instant::now();
        while transport.status().playing && started.elapsed() < Duration::from_millis(max_ms) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let before_stop = transport.status();
        transport.stop();
        let after_stop = transport.status();
        if after_stop.playing || before_stop.error.is_some() || after_stop.error.is_some() {
            return Err("transport still playing after stop/release".into());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "result": "SECTION_PLAYBACK_PILOT_COMPLETED",
                "stylePath": path,
                "section": {"id": section.id, "label": section.label, "start": section.start, "end": section.end},
                "output": port.name,
                "channels": channels,
                "noteCount": note_count,
                "playedUntilMs": before_stop.elapsed_ms,
                "wallMs": started.elapsed().as_millis(),
                "cancelled": before_stop.playing,
                "sendError": after_stop.error,
                "claims": {
                    "casmChordAcceptance": false,
                    "fullStyleAcceptance": false,
                    "wireTimingQualified": false,
                    "g7Closed": false
                },
                "limitations": [
                    "Application Transport send + cancel/release only; not MIDI wire or audio latency qualification.",
                    "Audible keyboard result requires separate user observation.",
                    "Does not claim CASM chord preview or full Style acceptance.",
                    "No bank/program/SysEx identity inquiry performed."
                ]
            }))?
        );
        return Ok(());
    }
    if args.len() == 2 && args[0] == "release-check" {
        let devices = midi::devices()?;
        let matches: Vec<_> = devices
            .outputs
            .iter()
            .filter(|p| p.name == args[1])
            .collect();
        if matches.len() != 1 {
            return Err("exactly one matching output is required; no fallback".into());
        }
        let port = matches[0];
        let document = release_document()?;
        let mut transport = midi::Transport::default();
        eprintln!(
            "RELEASE CHECK: hold note on {} then cancel (~800ms)",
            port.name
        );
        transport.play(
            &document,
            document.sections[0].id,
            vec![1],
            port.index,
            port.name.clone(),
            false,
        )?;
        std::thread::sleep(Duration::from_millis(800));
        let mid = transport.status();
        transport.stop();
        let after = transport.status();
        if after.playing || mid.error.is_some() || after.error.is_some() {
            return Err("transport still playing after stop/release".into());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "result": "RELEASE_CHECK_COMPLETED",
                "output": port.name,
                "heldNote": 60,
                "playedUntilMs": mid.elapsed_ms,
                "cancelled": mid.playing,
                "sendError": after.error,
                "releaseActions": ["CC64=0", "CC123", "CC120", "note-offs 0..127"],
                "claims": {
                    "physicalStuckNoteQualified": false,
                    "wireTimingQualified": false,
                    "g7Closed": false
                },
                "limitations": [
                    "Application cancel/release send completed; physical stuck-note observation is separate.",
                    "No audio capture in this probe."
                ]
            }))?
        );
        return Ok(());
    }
    if args.len() == 2 && args[0] == "panic-check" {
        let devices = midi::devices()?;
        let matches: Vec<_> = devices
            .outputs
            .iter()
            .filter(|p| p.name == args[1])
            .collect();
        if matches.len() != 1 {
            return Err("exactly one matching output is required; no fallback".into());
        }
        let port = matches[0];
        eprintln!(
            "PANIC CHECK: note-on without off on {}, then panic_release",
            port.name
        );
        // Leave a sounding note, then call the same release path as the app panic button.
        {
            let output = midir::MidiOutput::new("GenosStyleStudio panic-check")?;
            let ports = output.ports();
            let p = ports.get(port.index).ok_or("output disconnected")?;
            if output.port_name(p)? != port.name {
                return Err("device list changed".into());
            }
            let mut conn = output.connect(p, "GenosStyleStudio panic-check note")?;
            conn.send(&[0x90, 67, 100])?;
            std::thread::sleep(Duration::from_millis(600));
            // Intentionally do not send note-off before drop — panic must clear.
            drop(conn);
        }
        midi::panic_release(port.index, &port.name, &[1])?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "result": "PANIC_CHECK_COMPLETED",
                "output": port.name,
                "heldNote": 67,
                "velocity": 100,
                "channel": 1,
                "releaseApi": "midi::panic_release",
                "userAudioObservation": "REQUIRED_CONFIRM_SILENCE_AFTER_PANIC",
                "claims": {
                    "physicalStuckNoteQualified": false,
                    "wireTimingQualified": false,
                    "g7Closed": false
                },
                "limitations": [
                    "Send completed; audible silence after panic requires user observation.",
                    "Does not measure audio latency or residual Genos voice state beyond MIDI release."
                ]
            }))?
        );
        return Ok(());
    }
    if args.len() == 3 && args[0] == "thru-check" {
        // Open exact input, send notes on exact output, report whether input sees activity.
        let in_name = &args[1];
        let out_name = &args[2];
        let devices = midi::devices()?;
        let outs: Vec<_> = devices
            .outputs
            .iter()
            .filter(|p| p.name == *out_name)
            .collect();
        if outs.len() != 1 {
            return Err("exactly one matching output is required".into());
        }
        let out = outs[0];
        let mut input = MidiInput::new("GenosStyleStudio thru-check")?;
        input.ignore(Ignore::None);
        let in_ports: Vec<_> = input
            .ports()
            .into_iter()
            .filter(|p| input.port_name(p).is_ok_and(|n| n == *in_name))
            .collect();
        if in_ports.len() != 1 {
            return Err("exactly one matching input is required".into());
        }
        let (sender, receiver) = mpsc::sync_channel(4096);
        let connection = input.connect(
            &in_ports[0],
            "GenosStyleStudio thru",
            move |timestamp, data, _| {
                let _ = sender.try_send((timestamp, data.to_vec()));
            },
            (),
        )?;
        eprintln!(
            "THRU CHECK: listen {} while sending C4/E4 to {}",
            in_name, out_name
        );
        std::thread::sleep(Duration::from_millis(200));
        {
            let output = midir::MidiOutput::new("GenosStyleStudio thru out")?;
            let ports = output.ports();
            let p = ports.get(out.index).ok_or("output disconnected")?;
            if output.port_name(p)? != out.name {
                return Err("device list changed".into());
            }
            let mut conn = output.connect(p, "GenosStyleStudio thru send")?;
            for (note, vel) in [(60u8, 70u8), (64u8, 70u8)] {
                conn.send(&[0x90, note, vel])?;
                std::thread::sleep(Duration::from_millis(200));
                conn.send(&[0x80, note, 0])?;
                std::thread::sleep(Duration::from_millis(100));
            }
            drop(conn);
            midi::panic_release(out.index, &out.name, &[1])?;
        }
        let deadline = Instant::now() + Duration::from_millis(800);
        let mut captured = Vec::new();
        let mut note_ons = 0u32;
        while Instant::now() < deadline {
            if let Ok((ts, data)) = receiver.recv_timeout(Duration::from_millis(25)) {
                if matches!(data.first(), Some(0xf8 | 0xfe)) {
                    continue;
                }
                if data.len() == 3 && data[0] & 0xf0 == 0x90 && data[2] > 0 {
                    note_ons += 1;
                }
                captured.push(json!({"timestampUs": ts, "bytes": data}));
            }
        }
        drop(connection);
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "result": if note_ons > 0 { "THRU_CHECK_INPUT_SAW_NOTES" } else { "THRU_CHECK_NO_INPUT_ECHO" },
                "input": in_name,
                "output": out_name,
                "noteOnsSeen": note_ons,
                "captured": captured,
                "interpretation": if note_ons > 0 {
                    "Host input received notes during the output test. Activity is not correlated with sent packets and does not prove MIDI echo."
                } else {
                    "No input notes observed during this window. The test does not identify the keyboard routing configuration."
                },
                "claims": { "midiEchoVerified": false, "wireTimingQualified": false, "g7Closed": false },
                "limitations": [
                    "Absence of echo does not prove Genos is broken — Local Control/MIDI Out settings vary.",
                    "Timestamps are driver callbacks, not qualified wire latency."
                ]
            }))?
        );
        return Ok(());
    }
    if args.len() != 3 || args[0] != "listen" {
        return Err("usage: midi_probe scan | sections <sty-path> | capture <exact input> <seconds 1..120> [out.mid] | listen <exact input name> <seconds 1..120> | notes <exact output name> | system-effect <exact output> <reverb|chorus> <msb> <lsb> [return] | section <sty-path> <section-label> <exact output name> [max-ms] | release-check <exact output name> | panic-check <exact output name> | thru-check <exact input> <exact output>".into());
    }
    let seconds: u64 = args[2].parse()?;
    if !(1..=120).contains(&seconds) {
        return Err("listen duration must be 1..120 seconds".into());
    }
    let mut input = MidiInput::new("GenosStyleStudio passive diagnostics")?;
    input.ignore(Ignore::None);
    let matches: Vec<_> = input
        .ports()
        .into_iter()
        .filter(|p| input.port_name(p).is_ok_and(|name| name == args[1]))
        .collect();
    if matches.len() != 1 {
        return Err("exactly one matching input is required".into());
    }
    let (sender, receiver) = mpsc::sync_channel(4096);
    let dropped = Arc::new(AtomicUsize::new(0));
    let overrun = dropped.clone();
    let connection = input.connect(
        &matches[0],
        "GenosStyleStudio passive capture",
        move |timestamp, data, _| {
            if data.len() > 65536 || sender.try_send((timestamp, data.to_vec())).is_err() {
                overrun.fetch_add(1, Ordering::Relaxed);
            }
        },
        (),
    )?;
    eprintln!(
        "LISTENING: {} for {} seconds; no MIDI output opened",
        args[1], seconds
    );
    let started = Instant::now();
    let mut captured = Vec::new();
    let mut notes = 0;
    let mut offs = 0;
    let mut total = 0;
    while started.elapsed() < Duration::from_secs(seconds) {
        if let Ok((timestamp, data)) = receiver.recv_timeout(Duration::from_millis(25)) {
            total += data.len();
            if total > 2 * 1024 * 1024 {
                break;
            }
            if data.len() == 3 && data[0] & 0xf0 == 0x90 && data[2] > 0 {
                notes += 1;
            }
            if data.len() == 3
                && (data[0] & 0xf0 == 0x80 || (data[0] & 0xf0 == 0x90 && data[2] == 0))
            {
                offs += 1;
            }
            // Active sensing/clock are counted, without filling the musical log.
            if !matches!(data.first(), Some(0xf8 | 0xfe)) {
                captured.push(json!({"timestampUs":timestamp,"bytes":data}));
            }
        }
    }
    drop(connection);
    let mut note_ts: Vec<u64> = captured
        .iter()
        .filter_map(|ev| {
            let bytes = ev.get("bytes")?.as_array()?;
            let st = bytes.first()?.as_u64()? as u8;
            let vel = bytes.get(2).and_then(|v| v.as_u64()).unwrap_or(0);
            if st & 0xf0 == 0x90 && vel > 0 {
                ev.get("timestampUs")?.as_u64()
            } else {
                None
            }
        })
        .collect();
    note_ts.sort_unstable();
    let mut ioi_us: Vec<u64> = note_ts
        .windows(2)
        .map(|w| w[1].saturating_sub(w[0]))
        .collect();
    ioi_us.sort_unstable();
    let timing = if ioi_us.is_empty() {
        json!(null)
    } else {
        let mid = ioi_us[ioi_us.len() / 2];
        let p95 = ioi_us[(ioi_us.len().saturating_sub(1) * 95) / 100];
        // Cluster near-simultaneous layered voices (common on Genos multi-channel outs).
        let mut clusters = 0u32;
        let mut last: Option<u64> = None;
        for &t in &note_ts {
            let new_cluster = match last {
                Some(prev) => t.saturating_sub(prev) > 5_000,
                None => true,
            };
            if new_cluster {
                clusters += 1;
            }
            last = Some(t);
        }
        json!({
            "noteOnCount": note_ts.len(),
            "noteOnTimeClusters": clusters,
            "ioiUsMin": ioi_us.first(),
            "ioiUsMedian": mid,
            "ioiUsP95": p95,
            "ioiUsMax": ioi_us.last(),
            "source": "midir callback timestampUs deltas — not qualified wire latency"
        })
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "input": args[1],
            "durationMs": started.elapsed().as_millis(),
            "noteOns": notes,
            "noteOffs": offs,
            "captured": captured,
            "dropped": dropped.load(Ordering::Relaxed),
            "sentMessages": 0,
            "timing": timing,
            "limitations": [
                "Driver callback timestamps, not wire timing or audio latency.",
                "Device model is user-reported; port name alone is not a model identity response.",
                "Zero noteOns means no note input was observed; this does not identify the cause."
            ]
        }))?
    );
    Ok(())
}
