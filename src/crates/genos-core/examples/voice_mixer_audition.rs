//! Opt-in hardware listening check (never ordinary CI).
//!
//! Applies the app's own voice and mixer edits (`Command::SetVoice`, `Command::MixerCc`) to a
//! Style section and plays one part through the app transport, so a listener can confirm the
//! instrument follows them.
//!
//! Usage: voice_mixer_audition <sty> <section-label> <exact output> <channel 1-16> <ms 500..20000>
//!        <voice search or -> [edit ...]
//!
//! Edits: `<controller>=<value>` (Mixer CC), `eqlow=<v>` / `eqhigh=<v>` (Mixer EQ),
//! `drum:<note>:<volume|pan|pitch>=<v>` (drum per-key setup of the part's kit).
//!
//! `<sty>` may be `synthetic:chord` (a held C-E-G-C on the given channel) or
//! `synthetic:hihat` (eighth-note key 42 on a factory drum kit) or `synthetic:hits` (a C-E-G-C
//! hit every second for 16 s) for steady measurements. With `hits`, `alt:<cc>=<a>/<b>` and
//! `alt:eqhigh|eqlow=<a>/<b>` write the app's CC/EQ lane so hits alternate between a and b.
use genos_core::{
    channel_eq::EqBand,
    edit::{Command, Session},
    midi,
    native_drums::{Change, Field},
    voice_catalog,
};
use serde_json::json;
use std::{fs, time::Duration};

/// One "Main A" section of 4 bars at 120 BPM with steady material for measurement.
/// Synthetic layout shared by the stimulus and the alternating lanes.
const START: u64 = 4 * 480;
/// One hit per half note (one second at 120 BPM).
const HIT: u64 = 960;

fn synthetic(kind: &str, channel: u8) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use genos_core::smf::{self, Chunk, Event};
    let ch = channel - 1;
    // SFF2 layout: format marker and SInt setup bar, then Main A from bar 2.
    let bars: u64 = if kind == "hits" { 8 * 4 * 480 } else { 4 * 4 * 480 };
    let mut data: Vec<(u64, Vec<u8>)> = vec![
        (0, vec![0xff, 0x51, 3, 0x07, 0xa1, 0x20]),
        (0, vec![0xff, 6, 4, b'S', b'F', b'F', b'2']),
        (0, vec![0xff, 6, 4, b'S', b'I', b'n', b't']),
        (START, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']),
    ];
    match kind {
        "chord" => {
            for pitch in [60u8, 64, 67, 72] {
                data.push((START, vec![0x90 | ch, pitch, 110]));
                data.push((START + bars - 1, vec![0x80 | ch, pitch, 0]));
            }
        }
        "hits" => {
            for hit in 0..16u64 {
                for pitch in [60u8, 64, 67, 72] {
                    data.push((START + hit * HIT, vec![0x90 | ch, pitch, 110]));
                    data.push((START + hit * HIT + 240, vec![0x80 | ch, pitch, 0]));
                }
            }
        }
        "hihat" => {
            let kit = voice_catalog::factory()
                .search_drum_kits("Real")?
                .voices
                .into_iter()
                .next()
                .ok_or("no factory drum kit")?;
            data.push((0, vec![0xb0 | ch, 0, kit.msb]));
            data.push((0, vec![0xb0 | ch, 32, kit.lsb]));
            data.push((0, vec![0xc0 | ch, kit.program]));
            for step in 0..32u64 {
                data.push((START + step * 240, vec![0x90 | ch, 42, 110]));
                data.push((START + step * 240 + 120, vec![0x80 | ch, 42, 0]));
            }
        }
        _ => return Err("synthetic material is chord or hihat".into()),
    }
    data.push((START + bars, vec![0xff, 0x2f, 0]));
    data.sort_by_key(|(tick, _)| *tick);
    let events: Vec<Event> = data
        .into_iter()
        .enumerate()
        .map(|(i, (tick, data))| Event {
            id: i as u64 + 1,
            tick,
            order: i as u64 + 1,
            data,
        })
        .collect();
    Ok(smf::pack(&[
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 6 {
        return Err("usage: voice_mixer_audition <sty> <section> <output> <channel> <ms> <voice|-> [cc=value ...]".into());
    }
    let channel: u8 = args[3].parse()?;
    let ms: u64 = args[4].parse()?;
    if !(1..=16).contains(&channel) || !(500..=20_000).contains(&ms) {
        return Err("channel must be 1..16 and ms 500..20000".into());
    }
    let bytes = match args[0].strip_prefix("synthetic:") {
        Some(kind) => synthetic(kind, channel)?,
        None => fs::read(&args[0])?,
    };
    let mut session = Session::open(&bytes, "audition".into())?;
    let section = session
        .document
        .sections
        .iter()
        .find(|s| s.label == args[1])
        .ok_or("section not found")?
        .id;
    let mut voice = None;
    if args[5] != "-" {
        let hit = voice_catalog::factory()
            .search(&args[5])?
            .voices
            .into_iter()
            .next()
            .ok_or("no voice matches the search")?;
        session.execute(
            session.revision,
            Command::SetVoice {
                section,
                channel,
                address: hit.address(),
            },
        )?;
        voice = Some(json!({ "name": hit.name, "msb": hit.msb, "lsb": hit.lsb, "program": hit.program }));
    }
    let mut controllers = Vec::new();
    for change in &args[6..] {
        let (target, value) = change.split_once('=').ok_or("edits look like 7=40")?;
        if let Some(lane) = target.strip_prefix("alt:") {
            let (a, b) = value.split_once('/').ok_or("alternating edits look like alt:7=100/40")?;
            let (a, b): (u8, u8) = (a.parse()?, b.parse()?);
            // Each value is set a little before its hit and holds through the hit's tail.
            let points: Vec<genos_core::edit::ControllerPoint> = (0..16u64)
                .map(|hit| genos_core::edit::ControllerPoint {
                    tick: (START + hit * HIT).saturating_sub(u64::from(hit > 0) * 120),
                    value: if hit % 2 == 0 { a } else { b },
                })
                .collect();
            let command = match lane {
                "eqhigh" | "eqlow" => Command::MixerEqPoints {
                    section,
                    channel,
                    band: if lane == "eqhigh" { EqBand::EqHigh } else { EqBand::EqLow },
                    points,
                    clear_ticks: Vec::new(),
                },
                cc => Command::ControllerPoints {
                    section,
                    channel,
                    controller: cc.parse()?,
                    points,
                    remove: Vec::new(),
                },
            };
            session.execute(session.revision, command)?;
            controllers.push(change.clone());
            continue;
        }
        let command = if let Some(band) = match target {
            "eqlow" => Some(EqBand::EqLow),
            "eqhigh" => Some(EqBand::EqHigh),
            _ => None,
        } {
            Command::MixerEq {
                section,
                channel,
                band,
                value: value.parse()?,
                all_sections: false,
            }
        } else if let Some(key) = target.strip_prefix("drum:") {
            let (note, field) = key.split_once(':').ok_or("drum edits look like drum:42:volume=1")?;
            let field = match field {
                "volume" => Field::Volume,
                "pan" => Field::Pan,
                "pitch" => Field::Pitch,
                _ => return Err("drum field must be volume, pan or pitch".into()),
            };
            let start = session.document.sections.iter().find(|s| s.id == section).map_or(0, |s| s.start);
            Command::SetDrumScalars {
                channel,
                address: voice_catalog::address_at(&session.document, start, channel)
                    .ok_or("the part has no drum kit address")?,
                changes: vec![Change {
                    note: note.parse()?,
                    field,
                    value: value.parse()?,
                }],
            }
        } else {
            Command::MixerCc {
                section,
                channel,
                controller: target.parse()?,
                value: value.parse()?,
                all_sections: false,
            }
        };
        session.execute(session.revision, command)?;
        controllers.push(change.clone());
    }
    // Output "-" prints the part's controller and program messages instead of playing.
    if args[2] == "-" {
        let plan = midi::plan(&session.document, section, &[channel])?;
        for packet in plan
            .packets
            .iter()
            .filter(|p| matches!(p.data[0] & 0xf0, 0xb0 | 0xc0))
        {
            println!("{:>8.1} ms  {:02X?}", packet.at.as_secs_f64() * 1000.0, packet.data);
        }
        return Ok(());
    }
    let port = midi::devices()?
        .outputs
        .into_iter()
        .find(|p| p.name == args[2])
        .ok_or("output not found")?;
    let mut transport = midi::Transport::default();
    transport.play(&session.document, section, vec![channel], port.index, port.name.clone(), true)?;
    std::thread::sleep(Duration::from_millis(ms));
    let status = transport.status();
    transport.stop();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "section": args[1],
            "channel": channel,
            "voice": voice,
            "controllers": controllers,
            "playing": status.playing,
            "error": status.error,
        }))?
    );
    Ok(())
}
