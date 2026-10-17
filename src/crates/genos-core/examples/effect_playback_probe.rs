//! Opt-in hardware diagnostic. --prepare never opens MIDI; --play requires an exact port.
//! Synthetic source-note files test header authoring/reopen/playback, not Genos Style loading.
use genos_core::{
    document::Document,
    edit::{Command, Session},
    genos_system_effect,
    midi::{self, PlaybackOptions, Transport},
    project,
    smf::{self, Chunk, Event},
    style_effects::{self, Address, Block, Change},
};
use serde_json::json;
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn fixture() -> genos_core::Result<Vec<u8>> {
    let mut rows = vec![(0, vec![0xff, 6, 4, b'S', b'F', b'F', b'2'])];
    let mut tick = 1;
    for block in ["reverb", "chorus"] {
        for wire in genos_system_effect::system_effect_packets(block, Some(0), Some(0), Some(0))? {
            let mut data = vec![0xf0];
            smf::write_vlq((wire.len() - 1) as u64, &mut data)?;
            data.extend_from_slice(&wire[1..]);
            rows.push((tick, data));
            tick += 1;
        }
    }
    rows.extend([
        (5, vec![0xb0, 91, 127]),
        (6, vec![0xb0, 93, 0]),
        (480, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']),
        (480, vec![0x90, 60, 64]),
        (1920, vec![0x80, 60, 0]),
        (4800, vec![0xff, 0x2f, 0]),
    ]);
    let events = rows
        .into_iter()
        .enumerate()
        .map(|(i, (tick, data))| Event {
            id: i as u64 + 1,
            order: i as u64 + 1,
            tick,
            data,
        })
        .collect::<Vec<_>>();
    smf::pack(&[
        Chunk {
            tag: *b"MThd",
            data: vec![0, 0, 0, 1, 1, 224],
        },
        Chunk {
            tag: *b"MTrk",
            data: smf::encode_track(&events)?,
        },
    ])
}
fn authored() -> genos_core::Result<Vec<u8>> {
    let mut session = Session::open(&fixture()?, "Effect startup wet".into())?;
    session.execute(
        0,
        Command::SetStyleEffects {
            changes: vec![Change {
                block: Block::Reverb,
                effect: Some(Address { msb: 1, lsb: 35 }),
                return_level: Some(127),
            }],
        },
    )?;
    session.document.style.bytes()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 2 || !matches!(args[0].as_str(), "--prepare" | "--play") {
        return Err("Usage: effect_playback_probe --prepare <existing-dir> | --play <existing-dir> <exact-output>".into());
    }
    let dir = Path::new(&args[1]);
    if !dir.is_dir() {
        return Err("Output directory must already exist".into());
    }
    if args[0] == "--prepare" {
        project::write_new(&dir.join("startup-dry.STY"), &fixture()?)?;
        project::write_new(&dir.join("startup-wet.STY"), &authored()?)?;
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"prepared":true,"physicalMidiSent":false,"syntheticSourceOnly":true})
            )?
        );
        return Ok(());
    }
    if args.len() != 3 {
        return Err("Supply the exact output name".into());
    }
    let devices = midi::devices()?;
    let ports = devices
        .outputs
        .iter()
        .filter(|p| p.name == args[2])
        .collect::<Vec<_>>();
    if ports.len() != 1 {
        return Err("Exactly one matching output required; no fallback".into());
    }
    let port = ports[0];
    let mut steps = Vec::new();
    // Verify both files against the locally prepared fixtures before transmitting anything.
    let originals = [fixture()?, authored()?];
    for (label, expected) in ["dry", "wet"].into_iter().zip(&originals) {
        if fs::read(dir.join(format!("startup-{label}.STY")))? != *expected {
            return Err("Prepared fixture bytes changed".into());
        }
    }
    for label in ["dry", "wet"] {
        let doc = Document::parse(&fs::read(dir.join(format!("startup-{label}.STY")))?)?;
        let setup = style_effects::preview_setup(&doc)?;
        eprintln!(
            "INITIAL EFFECT PLAYBACK: {label}, {} typed startup messages, C4 velocity64 / 1.5s hold + 3s tail",
            setup.packets.len()
        );
        let mut transport = Transport::default();
        transport.play_with_options(
            &doc,
            PlaybackOptions {
                section: doc.sections[0].id,
                channels: vec![1],
                index: port.index,
                name: port.name.clone(),
                looping: false,
                initial_effects: true,
            },
        )?;
        let start = Instant::now();
        while transport.status().playing && start.elapsed() < Duration::from_secs(7) {
            thread::sleep(Duration::from_millis(10));
        }
        let status = transport.status();
        transport.stop();
        if status.playing || status.error.is_some() {
            return Err(format!("Playback failed: {:?}", status.error).into());
        }
        steps.push(json!({"label":label,"setup":setup,"elapsedMs":start.elapsed().as_millis(),"sendError":status.error}));
        if label == "dry" {
            thread::sleep(Duration::from_millis(800));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"result":"INITIAL_EFFECT_PLAYBACK_SEND_COMPLETED","output":port.name,"steps":steps,"audibleConfirmed":false,"keyboardStyleFileLoadTested":false,"wireTimingQualified":false,"limitations":["Synthetic source-note playback, not keyboard Style loading or CASM.","No audio capture; user observation required.","No claim about effect-engine settling latency.","Leaves known wet reverb (1,35), Return127, chorus No Effect/Return0; ch1 CC91=127 CC93=0."]})
        )?
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn fixtures_reopen_with_identical_paired_phrase_and_changed_initial_reverb() {
        let dry = super::Document::parse(&super::fixture().unwrap()).unwrap();
        let wet = super::Document::parse(&super::authored().unwrap()).unwrap();
        assert_eq!(dry.notes.len(), 1);
        assert_eq!(dry.notes[0].duration, 1440);
        let musical = |d: &super::Document| {
            d.notes
                .iter()
                .map(|n| (n.start, n.duration, n.track, n.channel, n.pitch, n.velocity))
                .collect::<Vec<_>>()
        };
        assert_eq!(musical(&dry), musical(&wet));
        let d = super::style_effects::preview_setup(&dry).unwrap();
        let w = super::style_effects::preview_setup(&wet).unwrap();
        assert_eq!(d.header.reverb.msb, Some(0));
        assert_eq!(w.header.reverb.lsb, Some(35));
        assert_eq!(w.header.reverb.return_level, Some(127));
        assert_eq!(d.packets.len(), 4);
        assert_eq!(w.packets.len(), 4);
    }
}
