//! Proof of concept: author a Sri Lankan 6/8 Style with tabla and conga in every section,
//! using only the app's own authoring path (blank from a reference scaffold, `SetVoice`,
//! `AddNotes`, export).
//!
//! Usage: sri_lankan_6_8_poc <3/4 or 6/8 SFF2 scaffold.sty> <out.sty>
//!
//! Voices come from the installed expansion on the user's Genos (panel: Expansion / Drum Kit,
//! which shows programs 1-based: #062-005-038 is MIDI program 37). Stroke keys were measured on
//! the instrument: see docs/development/reports/genos-hardware/sri-lankan-poc/README.md.
use genos_core::{
    blank::{BlankOptions, create_blank_from_reference},
    edit::{Command, NoteInput, Session},
    voice_catalog::Address,
};
use std::fs;

// Tabla kit "Tabla KIT", panel #062-005-038 (MIDI program 37). Stroke keys were measured on
// the instrument: each key against its own silent window, classified by pitch and decay.
const GE: u8 = 48; // open bass (bayan), 88 Hz, strongly tonal
const GHE: u8 = 56; // resonant bass, 127 Hz, long decay
const KE: u8 = 41; // closed bass: low peak with a broad noisy spectrum
const NA: u8 = 43; // treble (dayan), 430 Hz, crisp fast decay
const TIN: u8 = 67; // ringing treble, 718 Hz
const TE: u8 = 39; // bright noisy slap, 5 kHz
const TA: u8 = 51; // ringing rim, 970 Hz

// Conga kit, panel #062-005-041 (MIDI program 40): measured keys.
const CONGA_OPEN: u8 = 62; // low conga open tone, 127 Hz, most tonal key
const CONGA_MUTE: u8 = 51; // 405 Hz, fastest decay
const CONGA_SLAP: u8 = 64; // bright, 1.5 kHz
const CONGA_HIGH: u8 = 61; // high conga open, 378 Hz
const BONGO: u8 = 68; // 633 Hz, the kit's loudest key

const TABLA_KIT: Address = Address {
    msb: 62,
    lsb: 5,
    program: 37,
};
const CONGA_KIT: Address = Address {
    msb: 62,
    lsb: 5,
    program: 40,
};

/// One stroke on the 6/8 grid: (sixteenth 0..12 within the bar, key, velocity).
type Hit = (u64, u8, u8);

/// Dadra taal (Dha Dhi Na | Dha Ti Na) and its variations, per section character.
fn tabla(kind: &str, last_bar: bool) -> Vec<Hit> {
    let dadra = vec![
        (0, GE, 104),
        (0, NA, 96), // Dha
        (2, GE, 70),
        (2, TIN, 84), // Dhi
        (4, NA, 80),  // Na
        (6, GHE, 98),
        (6, NA, 92),  // Dha
        (8, TE, 76),  // Ti
        (10, NA, 82), // Na
    ];
    let mut hits = match kind {
        "intro" => vec![
            (0, GE, 100),
            (0, NA, 90),
            (4, NA, 70),
            (6, GHE, 90),
            (10, NA, 74),
        ],
        "main-a" => dadra,
        "main-b" => [dadra, vec![(9, TE, 60), (11, TE, 64)]].concat(),
        "main-c" => [
            dadra,
            vec![(5, KE, 72), (7, TE, 58), (9, TA, 66), (11, TE, 62)],
        ]
        .concat(),
        "main-d" => {
            let mut drive = vec![(0, GE, 110), (3, GE, 78), (6, GHE, 104), (9, GE, 80)];
            drive.extend((0..12).map(|s| {
                (
                    s,
                    if s % 2 == 0 { NA } else { TE },
                    if s % 3 == 0 { 96 } else { 66 },
                )
            }));
            drive
        }
        "fill" => (0..12)
            .map(|s| (s, if s < 6 { TE } else { NA }, 60 + s as u8 * 5))
            .chain([(0, GE, 100), (6, GHE, 104)])
            .collect(),
        "ending" => vec![
            (0, GE, 108),
            (0, NA, 100),
            (4, TE, 80),
            (6, GHE, 100),
            (8, NA, 84),
            (10, TE, 84),
        ],
        _ => dadra,
    };
    if last_bar && kind != "fill" {
        // Tihai-style pickup into the next bar: Te-Te-Na on the last three sixteenths.
        hits.extend([(9, TE, 74), (10, TE, 82), (11, NA, 94)]);
    }
    hits
}

/// Conga/bongo parts that answer the tabla (tumbao feel adapted to 6/8).
fn conga(kind: &str, last_bar: bool) -> Vec<Hit> {
    let base = vec![
        (0, CONGA_MUTE, 70),
        (4, CONGA_SLAP, 92),
        (6, CONGA_OPEN, 100),
        (8, CONGA_HIGH, 80),
        (10, CONGA_OPEN, 96),
    ];
    let mut hits = match kind {
        "intro" => vec![(6, CONGA_OPEN, 92), (10, CONGA_OPEN, 88)],
        "main-a" => base,
        "main-b" => [base, vec![(1, BONGO, 60), (3, BONGO, 66), (5, BONGO, 62)]].concat(),
        "main-c" => [
            base,
            vec![
                (2, CONGA_SLAP, 74),
                (7, BONGO, 64),
                (9, BONGO, 60),
                (11, BONGO, 70),
            ],
        ]
        .concat(),
        "main-d" => (0..12)
            .map(|s| match s % 6 {
                0 => (s, CONGA_OPEN, 100),
                2 | 4 => (s, CONGA_SLAP, 88),
                _ => (s, BONGO, 64),
            })
            .collect(),
        "fill" => vec![
            (0, CONGA_HIGH, 90),
            (2, CONGA_HIGH, 84),
            (4, CONGA_SLAP, 92),
            (6, CONGA_OPEN, 96),
            (8, CONGA_OPEN, 100),
            (10, CONGA_SLAP, 104),
            (11, CONGA_OPEN, 110),
        ],
        "ending" => vec![
            (0, CONGA_OPEN, 104),
            (6, CONGA_OPEN, 100),
            (8, CONGA_SLAP, 92),
        ],
        _ => base,
    };
    if last_bar && kind == "main-a" {
        hits.push((11, CONGA_SLAP, 90));
    }
    hits
}

/// Baila bass on the source chord (C major 7 scaffold): root on 1, fifth on 4.
fn bass(kind: &str) -> Vec<(u64, u8, u8, u64)> {
    match kind {
        "intro" | "ending" => vec![(0, 36, 100, 5)],
        "main-a" => vec![(0, 36, 104, 3), (6, 43, 96, 3)],
        "main-b" => vec![(0, 36, 104, 3), (4, 40, 84, 2), (6, 43, 96, 3)],
        "main-c" | "main-d" => vec![
            (0, 36, 108, 3),
            (4, 40, 88, 2),
            (6, 43, 100, 3),
            (10, 40, 86, 2),
        ],
        "fill" => vec![(0, 36, 104, 2), (6, 43, 100, 2), (10, 47, 96, 2)],
        _ => vec![(0, 36, 100, 3)],
    }
}

/// Offbeat guitar strums (C, E, G, B) on eighths 2-3 and 5-6.
fn chord(kind: &str) -> Vec<(u64, u8, u8, u64)> {
    let strum = |at: u64, vel: u8| [60u8, 64, 67, 71].map(|p| (at, p, vel, 1));
    match kind {
        "intro" => strum(6, 70).to_vec(),
        "ending" => [60u8, 64, 67, 71, 72].map(|p| (0, p, 86, 10)).to_vec(),
        "fill" => [strum(2, 80), strum(8, 84)].concat(),
        "main-a" => [strum(2, 76), strum(4, 70), strum(8, 78), strum(10, 72)].concat(),
        _ => [strum(2, 82), strum(4, 74), strum(8, 84), strum(10, 76)].concat(),
    }
}

fn kind(label: &str) -> &'static str {
    let upper = label.to_ascii_uppercase();
    if upper.starts_with("INTRO") {
        "intro"
    } else if upper.starts_with("ENDING") {
        "ending"
    } else if upper.starts_with("FILL") || upper.starts_with("BREAK") {
        "fill"
    } else {
        match upper.as_str() {
            "MAIN A" => "main-a",
            "MAIN B" => "main-b",
            "MAIN C" => "main-c",
            _ => "main-d",
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: sri_lankan_6_8_poc <scaffold.sty> <out.sty>".into());
    }
    let mut session: Session = create_blank_from_reference(
        &fs::read(&args[0])?,
        BlankOptions {
            name: "SL 6-8 Tabla Conga".into(),
            tempo_bpm: 168.0, // dotted quarter = 112
            numerator: 6,
            denominator: 8,
        },
    )?;
    // The scaffold has no Fill In DD; add it the way the section bar's "Add section" does.
    if !session
        .document
        .sections
        .iter()
        .any(|s| s.label == "Fill In DD")
    {
        let donor = session
            .document
            .sections
            .iter()
            .find(|s| s.label == "Fill In CC")
            .ok_or("scaffold has no Fill In CC to share CASM with")?
            .id;
        session.execute(
            session.revision,
            Command::AddSection {
                label: "Fill In DD".into(),
                bars: 1,
                casm_from: donor,
                copy_from: None,
                fit: None,
            },
        )?;
    }
    let ppq = u64::from(session.document.style.ppq);
    let sixteenth = ppq / 4;
    let bar = 3 * ppq;
    let sections: Vec<_> = session
        .document
        .sections
        .iter()
        .map(|s| (s.id, s.label.clone(), s.start, s.end))
        .collect();
    for (id, label, start, end) in &sections {
        let kind = kind(label);
        let bars = ((end - start) / bar).max(1);
        for (channel, address) in [(9u8, TABLA_KIT), (10, CONGA_KIT)] {
            session.execute(
                session.revision,
                Command::SetVoice {
                    section: *id,
                    channel,
                    address,
                },
            )?;
        }
        let mut notes = Vec::new();
        for b in 0..bars {
            let at = start + b * bar;
            let last = b + 1 == bars;
            for (ch, hits) in [(9u8, tabla(kind, last)), (10, conga(kind, last))] {
                for (step, key, velocity) in hits {
                    notes.push(NoteInput {
                        track: 0,
                        channel: ch,
                        pitch: key,
                        velocity,
                        start: at + step * sixteenth,
                        duration: sixteenth,
                    });
                }
            }
            for (ch, part) in [(11u8, bass(kind)), (12, chord(kind))] {
                for (step, pitch, velocity, length) in part {
                    notes.push(NoteInput {
                        track: 0,
                        channel: ch,
                        pitch,
                        velocity,
                        start: at + step * sixteenth,
                        duration: length * sixteenth - 10,
                    });
                }
            }
        }
        notes.retain(|n| n.start + n.duration <= *end);
        session.execute(
            session.revision,
            Command::AddNotes {
                section: *id,
                notes,
                channel_events: Vec::new(),
            },
        )?;
        println!("{label:<12} {kind:<7} {bars} bar(s) of 6/8");
    }
    // Every part written in a section needs a CASM route there, or the Genos stays silent on
    // it: copy the part's route from a section that has one (Main A first).
    for (label, channel, _) in session.document.unrouted_tracks() {
        let target = session
            .document
            .sections
            .iter()
            .find(|s| s.label == label)
            .map(|s| s.id)
            .ok_or("section")?;
        let template = ["Main A", "Main B", "Main C", "Main D"]
            .into_iter()
            .find(|l| session.document.source_routed(l, channel))
            .and_then(|l| session.document.sections.iter().find(|s| s.label == l))
            .map(|s| s.id)
            .ok_or("no section routes this part")?;
        session.execute(
            session.revision,
            Command::AdoptRoute {
                section: target,
                source: channel,
                from_section: template,
            },
        )?;
        println!("routed channel {channel} in {label}");
    }
    // Mix (the Mixer panel's volume with "apply to all sections"): percussion up front.
    let main_a = sections
        .iter()
        .find(|s| s.1 == "Main A")
        .ok_or("no Main A")?
        .0;
    for (channel, value) in [(9u8, 120u8), (10, 120), (11, 100), (12, 90)] {
        session.execute(
            session.revision,
            Command::MixerCc {
                section: main_a,
                channel,
                controller: 7,
                value,
                all_sections: true,
            },
        )?;
    }
    let bytes = session.document.style.bytes()?;
    // Reopen the export exactly as the app would.
    let check = genos_core::document::Document::parse(&bytes)?;
    fs::write(&args[1], &bytes)?;
    // Notes the scaffold's CASM leaves unrouted: the Genos will not play them.
    for (section, channel, notes) in check.unrouted_tracks() {
        println!(
            "not played by the Genos: {section} channel {channel} ({notes} notes, no CASM route)"
        );
    }
    println!(
        "wrote {} ({} bytes): {} sections, {} notes, warnings {:?}",
        args[1],
        bytes.len(),
        check.sections.len(),
        check.notes.len(),
        check.warnings
    );
    Ok(())
}
