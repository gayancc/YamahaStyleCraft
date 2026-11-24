use genos_core::{
    document::{Document, Route},
    live_preview::{self, Chord, Config},
    smf::{Chunk, Event},
};
fn fixture() -> Document {
    let mut events = Vec::new();
    for (tick, data) in [
        (0, vec![0xb0, 0, 0]),
        (0, vec![0xb0, 32, 0]),
        (0, vec![0xc0, 4]),
        (0, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']),
        (0, vec![0x90, 60, 90]),
        (0, vec![0x90, 64, 80]),
        (480, vec![0x80, 60, 0]),
        (480, vec![0x80, 64, 0]),
        (1920, vec![0xff, 0x2f, 0]),
    ] {
        let id = events.len() as u64 + 1;
        events.push(Event {
            id,
            order: id,
            tick,
            data,
        });
    }
    let bytes = genos_core::smf::pack(&[
        Chunk {
            tag: *b"MThd",
            data: vec![0, 0, 0, 1, 1, 224],
        },
        Chunk {
            tag: *b"MTrk",
            data: genos_core::smf::encode_track(&events).unwrap(),
        },
    ])
    .unwrap();
    let mut doc = Document::parse(&bytes).unwrap();
    let mut b = vec![0; 47];
    b[9] = 15;
    b[11] = 15;
    b[12] = 255;
    b[13] = 3;
    b[14] = 255;
    b[15] = 255;
    b[16] = 255;
    b[17] = 255;
    b[21] = 127;
    for o in [22, 28, 34] {
        b[o + 1] = 2;
        b[o + 2] = 11;
        b[o + 4] = 127;
        b[o + 5] = 3;
    }
    doc.routes.push(Route {
        id: 0,
        sections: vec!["Main A".into()],
        source: 1,
        destination: 16,
        bytes: b,
        source_root: 0,
        source_chord: 0,
        middle_ntr: 0,
        middle_ntt: 2,
        middle_ntt_bass: false,
        chunk: 0,
        offset: 0,
    });
    doc
}
fn config(doc: &Document) -> Config {
    Config {
        section: doc.sections[0].id,
        sources: vec![1],
        chords: vec![
            Chord {
                root: 0,
                quality: "Major".into(),
                bars: 1,
                bass: None,
                bass_quality: None,
            },
            Chord {
                root: 9,
                quality: "Minor".into(),
                bars: 1,
                bass: None,
                bass_quality: None,
            },
        ],
        effects: false,
        drums: false,
    }
}
#[test]
#[ignore = "requires the local Genos firmware tables and rhythm replay"]
fn rhythm_note_on_matches_firmware_replay() {
    let path = std::env::var_os("GENOS_RHYTHM_REPLAY").unwrap();
    let rows: Vec<(Vec<u8>, u8, u8, u8, Option<u8>)> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(rows.len(), 4_608);
    let mut doc = fixture();
    let section = doc.sections[0].id;
    for (mut bytes, root, quality, pitch, expected) in rows {
        let route = &mut doc.routes[0];
        route.destination = bytes[9] + 1;
        bytes[0] = route.source - 1;
        route.bytes = bytes;
        let chord = genos_core::midi::LiveChord {
            root,
            quality: match quality {
                0 => "Major",
                34 => "Genos:34",
                _ => panic!("unexpected chord type {quality}"),
            }
            .into(),
            bass: None,
            bass_quality: None,
        };
        let actual = live_preview::follow_pitch(&doc, section, 1, pitch, &chord);
        assert_eq!(
            actual, expected,
            "rhythm route {:?}, chord {chord:?}, pitch {pitch}",
            doc.routes[0].bytes
        );
    }
}

#[test]
#[ignore = "requires the local Genos firmware tables and Guitar mask replay"]
fn guitar_region_masks_match_firmware_replay() {
    let path = std::env::var_os("GENOS_GUITAR_MASK_REPLAY").unwrap();
    let csv = std::fs::read_to_string(path).unwrap();
    let mut doc = fixture();
    let section = doc.sections[0].id;
    let chord = genos_core::midi::LiveChord {
        root: 0,
        quality: "Major".into(),
        bass: Some(2),
        bass_quality: Some("Minor".into()),
    };
    assert_eq!(csv.lines().count(), 12_288);
    for line in csv.lines() {
        let values: Vec<u16> = line
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        let [ntt, flag, roots, types, pitch, expected, _] = values[..] else {
            panic!("invalid Guitar mask row: {line}");
        };
        let bytes = &mut doc.routes[0].bytes;
        bytes[11..13].copy_from_slice(&roots.to_be_bytes());
        bytes[13..18].fill(0);
        bytes[16..18].copy_from_slice(&types.to_be_bytes());
        bytes[28..34].copy_from_slice(&[2, ntt as u8 | (flag as u8) << 7, 11, 0, 127, 1]);
        let actual = live_preview::follow_pitch(&doc, section, 1, pitch as u8, &chord);
        assert_eq!(actual.unwrap_or(255), expected as u8, "{line}");
    }
}

#[test]
fn follow_pitch_repitches_through_the_route_and_falls_back_to_root_shift() {
    use genos_core::midi::LiveChord;
    let doc = fixture();
    let section = doc.sections[0].id;
    let chord = |root: u8, quality: &str| LiveChord {
        root,
        quality: quality.into(),
        bass: None,
        bass_quality: None,
    };
    // Routed source 1: C Major source chord, followed chord D Major -> up a whole tone.
    assert_eq!(
        live_preview::follow_pitch(&doc, section, 1, 60, &chord(2, "Major")),
        Some(62)
    );
    assert_eq!(
        live_preview::follow_pitch(&doc, section, 1, 64, &chord(2, "Minor")),
        Some(65)
    );
    // Unrouted source channel: plain root transposition (G goes down a fourth).
    assert_eq!(
        live_preview::follow_pitch(&doc, section, 2, 60, &chord(7, "Major")),
        Some(55)
    );
}
#[test]
fn routes_progression_and_preserves_document() {
    let doc = fixture();
    let before = doc.style.bytes().unwrap();
    let compiled = live_preview::compile(&doc, &config(&doc)).unwrap();
    let ons = compiled
        .plan
        .packets
        .iter()
        .filter(|p| p.data[0] & 0xf0 == 0x90)
        .map(|p| p.data.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ons,
        vec![
            vec![0x9f, 60, 90],
            vec![0x9f, 64, 80],
            vec![0x9f, 69, 90],
            vec![0x9f, 72, 80]
        ]
    );
    assert_eq!(compiled.report.duration_ms, 4000);
    assert_eq!(compiled.report.changed_pitches, 2);
    assert_eq!(
        compiled
            .plan
            .packets
            .iter()
            .filter(|p| p.data[0] & 0xf0 == 0x80)
            .count(),
        4
    );
    assert_eq!(doc.style.bytes().unwrap(), before);
    assert!(compiled.plan.setup.iter().any(|p| p == &vec![0xcf, 4]));
    assert!(
        !compiled
            .plan
            .packets
            .iter()
            .any(|p| p.data[0] & 0xf0 == 0xc0)
    );
}
#[test]
fn mask_uses_all_34_bits_and_suppresses_minor() {
    let mut doc = fixture();
    for b in &mut doc.routes[0].bytes[13..18] {
        *b = 0;
    }
    doc.routes[0].bytes[17] = 1;
    let c = live_preview::compile(&doc, &config(&doc)).unwrap();
    assert_eq!(c.report.note_ons, 2);
    assert_eq!(c.report.suppressed_notes, 2);
    doc.routes[0].bytes[17] = 0;
    doc.routes[0].bytes[13] = 2;
    let mut conf = config(&doc);
    conf.chords = vec![Chord {
        root: 0,
        quality: "sus2".into(),
        bars: 1,
        bass: None,
        bass_quality: None,
    }];
    assert!(live_preview::compile(&doc, &conf).is_ok());
}

#[test]
fn unflagged_note_mute_filters_chord_root_and_preserves_controls() {
    let mut doc = fixture();
    add_event(&mut doc, 240, vec![0xb0, 10, 42]);
    for root in 0..12 {
        doc.routes[0].bytes[11] = 0;
        doc.routes[0].bytes[12] = 0;
        doc.routes[0].bytes[12 - (root / 8) as usize] = 1 << (root % 8);
        let mut c = config(&doc);
        c.chords = vec![
            Chord {
                root,
                quality: "Major".into(),
                bars: 1,
                bass: Some((root + 1) % 12),
                bass_quality: None,
            },
            Chord {
                root: (root + 1) % 12,
                quality: "Minor".into(),
                bars: 1,
                bass: Some(root),
                bass_quality: None,
            },
        ];
        let result = live_preview::compile(&doc, &c).unwrap();
        assert_eq!(result.report.note_ons, 2, "root {root}");
        assert_eq!(result.report.suppressed_notes, 2);
        assert_eq!(
            result
                .plan
                .packets
                .iter()
                .filter(|p| p.data == vec![0xbf, 10, 42])
                .count(),
            2
        );
    }
    // Auto Start is a pre-chord flag; explicit progression always has a chord.
    let mut c = config(&doc);
    c.chords = vec![Chord {
        root: 11,
        quality: "Major".into(),
        bars: 1,
        bass: None,
        bass_quality: None,
    }];
    let a = live_preview::compile(&doc, &c).unwrap();
    doc.routes[0].bytes[13] ^= 4;
    let b = live_preview::compile(&doc, &c).unwrap();
    assert_eq!(a.report.note_ons, b.report.note_ons);
}

#[test]
fn mixed_bass_regions_apply_root_masks_to_their_own_chord_root() {
    let mut doc = fixture();
    let section = config(&doc).section;
    let source = doc.routes[0].source;
    let bytes = &mut doc.routes[0].bytes;
    bytes[11..13].copy_from_slice(&[0, 4]); // Allow D, mute C.
    bytes[20] = 61;
    bytes[22..28].copy_from_slice(&[0, 0, 11, 0, 127, 1]);
    bytes[28..34].copy_from_slice(&[0, 128, 11, 0, 127, 1]);
    let chord = genos_core::midi::LiveChord {
        root: 0,
        quality: "Major".into(),
        bass: Some(2),
        bass_quality: None,
    };
    assert_eq!(
        live_preview::follow_pitch(&doc, section, source, 60, &chord),
        None
    );
    assert_eq!(
        live_preview::follow_pitch(&doc, section, source, 64, &chord),
        Some(66)
    );

    let mut cfg = config(&doc);
    cfg.chords = vec![Chord {
        root: 0,
        quality: "Major".into(),
        bars: 1,
        bass: Some(2),
        bass_quality: None,
    }];
    let result = live_preview::compile(&doc, &cfg).unwrap();
    assert_eq!(result.report.note_ons, 1);
    assert_eq!(result.report.suppressed_notes, 1);
    assert!(result.plan.packets.iter().any(|p| p.data == [0x9f, 66, 80]));
    doc.routes[0].bytes[12] = 5;
    doc.routes[0].bytes[13..18].copy_from_slice(&[0, 0, 0, 0, 1]);
    cfg.chords[0].bass_quality = Some("Minor".into());
    let result = live_preview::compile(&doc, &cfg).unwrap();
    assert_eq!(result.report.note_ons, 1);
    assert_eq!(result.report.suppressed_notes, 1);
    assert!(result.plan.packets.iter().any(|p| p.data == [0x9f, 60, 90]));
}
#[test]
#[ignore = "requires the local Genos region replay"]
fn source_zone_boundaries_match_firmware_replay() {
    let path = std::env::var_os("GENOS_REGION_REPLAY").unwrap();
    let csv = std::fs::read_to_string(path).unwrap();
    let mut doc = fixture();
    let section = doc.sections[0].id;
    let chord = genos_core::midi::LiveChord {
        root: 2,
        quality: "Major".into(),
        bass: None,
        bass_quality: None,
    };
    let bytes = &mut doc.routes[0].bytes;
    bytes[22..28].copy_from_slice(&[1, 0, 11, 0, 127, 1]);
    bytes[28..34].copy_from_slice(&[0, 0, 11, 0, 127, 1]);
    bytes[34..40].copy_from_slice(&[0, 0, 0, 0, 127, 1]);
    assert_eq!(csv.lines().count(), 1_280);
    for line in csv.lines() {
        let values: Vec<u8> = line
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        let [low, high, note, expected] = values[..] else {
            panic!("invalid region replay row: {line}");
        };
        doc.routes[0].bytes[20..22].copy_from_slice(&[low, high]);
        let actual = live_preview::follow_pitch(&doc, section, 1, note, &chord);
        assert_eq!(actual.unwrap_or(255), expected, "{line}");
    }
}

#[test]
#[ignore = "requires the local Genos inverted output limits replay"]
fn inverted_output_limits_match_firmware_replay() {
    let path = std::env::var_os("GENOS_INVERTED_LIMITS_REPLAY").unwrap();
    let csv = std::fs::read_to_string(path).unwrap();
    let mut doc = fixture();
    let section = doc.sections[0].id;
    let chord = genos_core::midi::LiveChord {
        root: 2,
        quality: "Major".into(),
        bass: None,
        bass_quality: None,
    };
    assert_eq!(csv.lines().count(), 5_120);
    for line in csv.lines() {
        let values: Vec<u8> = line
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        let [ntr, low, high, pitch, primary] = values[..] else {
            panic!("invalid inverted limits row: {line}");
        };
        doc.routes[0].bytes[28..34].copy_from_slice(&[ntr, 0, 11, low, high, 1]);
        let actual = live_preview::follow_pitch(&doc, section, 1, pitch, &chord);
        // The converter returns a byte. The firmware mixer rejects its high bit.
        assert_eq!(actual, (primary < 128).then_some(primary), "{line}");
    }
}

#[test]
#[ignore = "requires the local Genos High Key byte replay"]
fn high_key_bytes_match_firmware_replay() {
    let path = std::env::var_os("GENOS_HIGH_KEY_REPLAY").unwrap();
    let csv = std::fs::read_to_string(path).unwrap();
    let mut doc = fixture();
    let section = doc.sections[0].id;
    let mut chord = genos_core::midi::LiveChord {
        root: 0,
        quality: "Major".into(),
        bass: None,
        bass_quality: None,
    };
    assert_eq!(csv.lines().count(), 135_168);
    for line in csv.lines() {
        let values: Vec<u8> = line
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        let [ntr, ntt, flag, key, ordinary, target, pitch, rtr, expected] = values[..] else {
            panic!("invalid High Key replay row: {line}");
        };
        chord.root = ordinary;
        chord.bass = (flag != 0).then_some(target);
        doc.routes[0].bytes[28..34].copy_from_slice(&[ntr, ntt | flag << 7, key, 0, 127, rtr]);
        let actual =
            live_preview::follow_pitch_in_window(&doc, section, 1, pitch, &chord, rtr != 1);
        assert_eq!(actual, (expected < 128).then_some(expected), "{line}");
    }
}

#[test]
#[ignore = "requires the local Genos firmware and retrigger phase replay"]
fn progression_retrigger_phase_matches_firmware_replay() {
    #[derive(serde::Deserialize)]
    struct Replay {
        phase: Vec<PhaseCase>,
    }
    #[derive(serde::Deserialize)]
    struct PhaseCase {
        division: u16,
        change: u64,
        distance: u64,
        pitch: u8,
    }

    let path = std::env::var_os("GENOS_RTR_WINDOW_REPLAY").unwrap();
    let replay: Replay = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(replay.phase.len(), 15);
    for case in replay.phase {
        let mut doc = fixture();
        doc.style.ppq = case.division;
        doc.sections[0].end = 2 * case.change;
        add_event(&mut doc, 0, vec![0xff, 0x58, 4, 7, 3, 24, 8]);
        doc.style.tracks[0].retain(|event| !event.is_on());
        let template = doc.notes[0].clone();
        doc.notes = [0, case.change, case.change + case.distance]
            .into_iter()
            .enumerate()
            .map(|(index, start)| {
                let mut note = template.clone();
                note.id = index as u64 + 1;
                note.start = start;
                note.duration = 1;
                note.pitch = 64;
                note
            })
            .collect();
        doc.routes[0].bytes[28..34].copy_from_slice(&[0, 0, 11, 0, 127, 2]);
        let mut config = config(&doc);
        config.chords[1].root = 2;
        config.chords[1].quality = "Major".into();
        let compiled = live_preview::compile(&doc, &config).unwrap();
        let pitches: Vec<_> = compiled
            .plan
            .packets
            .iter()
            .filter(|packet| packet.data[0] & 0xf0 == 0x90 && packet.data[2] != 0)
            .map(|packet| packet.data[1])
            .collect();
        assert_eq!(
            pitches,
            [64, 74, case.pitch],
            "{} / {}",
            case.division,
            case.distance
        );
    }
}

#[test]
#[ignore = "requires the local Genos source chord table replay and firmware tables"]
fn source_chord_type_bytes_match_firmware_replay() {
    let path = std::env::var_os("GENOS_SOURCE_TYPE_REPLAY").unwrap();
    let csv = std::fs::read_to_string(path).unwrap();
    let mut doc = fixture();
    let section = doc.sections[0].id;
    let chord = genos_core::midi::LiveChord {
        root: 0,
        quality: "Major".into(),
        bass: Some(2),
        bass_quality: Some("Minor".into()),
    };
    assert_eq!(csv.lines().count(), 31_152);
    for line in csv.lines() {
        let values: Vec<u8> = line
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        let [source_type, ntr, ntt, flag, note, expected] = values[..] else {
            panic!("invalid source chord replay row: {line}");
        };
        doc.routes[0].bytes[19] = source_type;
        doc.routes[0].bytes[28..34].copy_from_slice(&[ntr, ntt | flag << 7, 11, 0, 127, 1]);
        let actual = live_preview::follow_pitch(&doc, section, 1, note, &chord);
        assert_eq!(actual, (expected < 128).then_some(expected), "{line}");
    }
}

#[test]
#[ignore = "requires the local Genos target chord mask replay and firmware tables"]
fn target_chord_masks_match_firmware_replay() {
    let path = std::env::var_os("GENOS_TARGET_MASK_REPLAY").unwrap();
    let csv = std::fs::read_to_string(path).unwrap();
    let mut doc = fixture();
    let section = doc.sections[0].id;
    assert_eq!(csv.lines().count(), 9_440);
    for line in csv.lines() {
        let values: Vec<u8> = line
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        let [target, flag, ntt, slot, expected] = values[..] else {
            panic!("invalid target mask row: {line}");
        };
        let chord = genos_core::midi::LiveChord {
            root: 0,
            quality: if flag == 0 {
                format!("Genos:{target}")
            } else {
                "Major".into()
            },
            bass: Some(2),
            bass_quality: Some(if flag == 1 {
                format!("Genos:{target}")
            } else {
                "Minor".into()
            }),
        };
        let bytes = &mut doc.routes[0].bytes;
        bytes[11..13].copy_from_slice(&[0, 5]);
        bytes[13..18].copy_from_slice(&(1_u64 << slot).to_be_bytes()[3..]);
        bytes[28..34].copy_from_slice(&[0, ntt | flag << 7, 11, 0, 127, 1]);
        let actual = live_preview::follow_pitch(&doc, section, 1, 64, &chord);
        assert_eq!(actual, (expected < 128).then_some(expected), "{line}");
    }
    doc.routes[0].bytes[11..18].fill(255);
    let mut config = config(&doc);
    config.chords.truncate(1);
    config.chords[0].quality = "Genos:58".into();
    config.chords[0].bass = Some(2);
    config.chords[0].bass_quality = Some("Genos:42".into());
    assert!(
        live_preview::compile(&doc, &config)
            .unwrap()
            .report
            .note_ons
            > 0
    );
}

#[test]
fn zones_and_identity_limits_are_enforced() {
    let mut doc = fixture();
    let b = &mut doc.routes[0].bytes;
    b[20] = 61;
    b[21] = 63;
    b[22 + 3] = 72;
    b[22 + 4] = 72;
    b[34 + 3] = 52;
    b[34 + 4] = 52;
    let mut conf = config(&doc);
    conf.chords.truncate(1);
    let c = live_preview::compile(&doc, &conf).unwrap();
    let pitches = c
        .plan
        .packets
        .iter()
        .filter(|p| p.data[0] & 0xf0 == 0x90)
        .map(|p| p.data[1])
        .collect::<Vec<_>>();
    assert_eq!(pitches, vec![72, 52]);
}
#[test]
fn unsupported_rules_and_ambiguous_routes_fail_before_transport() {
    let mut doc = fixture();
    doc.routes[0].bytes[28] = 2;
    assert!(live_preview::compile(&doc, &config(&doc)).is_err());
    doc.routes[0].bytes[28] = 0;
    doc.routes[0].bytes[29] = 11;
    assert!(live_preview::compile(&doc, &config(&doc)).is_err());
    doc.routes[0].bytes[29] = 2;
    doc.routes.push(doc.routes[0].clone());
    assert!(live_preview::compile(&doc, &config(&doc)).is_err());
}

#[test]
fn compiled_sff2_preview_uses_the_firmware_cseg_winner() {
    use genos_core::smf::pack;

    let mut doc = fixture();
    add_event(&mut doc, 0, vec![0xff, 6, 4, b'S', b'F', b'F', b'2']);
    let track = doc.style.track_chunks[0];
    doc.style.chunks[track].data = genos_core::smf::encode_track(&doc.style.tracks[0]).unwrap();
    let cseg = |routes: &[(u8, usize)], legacy: bool| {
        let mut records = vec![Chunk {
            tag: *b"Sdec",
            data: b"Main A".to_vec(),
        }];
        if legacy {
            let mut ctab = vec![0; 26];
            ctab[9] = 15;
            records.push(Chunk {
                tag: *b"Ctab",
                data: ctab,
            });
            records.push(Chunk {
                tag: *b"Cntt",
                data: vec![0, 0x8a],
            });
        }
        for &(destination, size) in routes {
            let mut data = doc.routes[0].bytes.clone();
            data[9] = destination - 1;
            data.resize(size, 0x55);
            records.push(Chunk {
                tag: *b"Ctb2",
                data,
            });
        }
        Chunk {
            tag: *b"CSEG",
            data: pack(&records).unwrap(),
        }
    };
    let casm = pack(&[
        cseg(&[], true),
        cseg(&[(16, 40), (14, 57)], true),
        cseg(&[(15, 47)], false),
    ])
    .unwrap();
    doc.style.chunks.push(Chunk {
        tag: *b"CASM",
        data: casm,
    });
    let doc = Document::from_style(doc.style).unwrap();
    assert_eq!(doc.routes.len(), 3);
    let reparsed = Document::parse(&doc.style.bytes().unwrap()).unwrap();
    assert_eq!(
        reparsed.style.chunks.last().unwrap().data,
        doc.style.chunks.last().unwrap().data
    );
    let compiled = live_preview::compile(&reparsed, &config(&reparsed)).unwrap();
    let note_statuses: Vec<_> = compiled
        .plan
        .packets
        .iter()
        .filter(|packet| packet.data[0] & 0xf0 == 0x90)
        .map(|packet| packet.data[0])
        .collect();
    assert_eq!(note_statuses, [0x9d; 4]);
}

#[test]
fn sff1_preview_converts_legacy_routes_and_preserves_the_file() {
    use genos_core::smf::{encode_track, pack};

    let mut doc = fixture();
    add_event(&mut doc, 0, vec![0xff, 6, 4, b'S', b'F', b'F', b'1']);
    let track = doc.style.track_chunks[0];
    doc.style.chunks[track].data = encode_track(&doc.style.tracks[0]).unwrap();
    let mut ctab = doc.routes[0].bytes[..20].to_vec();
    ctab[9] = 13;
    ctab.extend([1, 4, 11, 0, 127, 3]);
    let group = pack(&[
        Chunk {
            tag: *b"Sdec",
            data: b"Main A".to_vec(),
        },
        Chunk {
            tag: *b"Ctab",
            data: ctab,
        },
        Chunk {
            tag: *b"Cntt",
            data: vec![0, 0],
        },
        Chunk {
            tag: *b"Ctb2",
            data: doc.routes[0].bytes.clone(),
        },
    ])
    .unwrap();
    doc.style.chunks.push(Chunk {
        tag: *b"CASM",
        data: pack(&[Chunk {
            tag: *b"CSEG",
            data: group,
        }])
        .unwrap(),
    });
    let bytes = doc.style.bytes().unwrap();
    let doc = Document::parse(&bytes).unwrap();
    let compiled = live_preview::compile(&doc, &config(&doc)).unwrap();
    let notes: Vec<_> = compiled
        .plan
        .packets
        .iter()
        .filter(|packet| packet.data[0] & 0xf0 == 0x90)
        .map(|packet| (packet.data[0], packet.data[1]))
        .collect();
    assert_eq!(notes, [(0x9d, 60), (0x9d, 64), (0x9d, 60), (0x9d, 64)]);
    assert_eq!(doc.style.bytes().unwrap(), bytes);
}

#[test]
fn held_notes_stop_or_rearticulate_with_explicit_warning() {
    let mut doc = fixture();
    doc.notes[0].duration = 3500;
    // Two-bar source, chord changes after one bar.
    doc.sections[0].end = 3840;
    doc.routes[0].bytes[33] = 1;
    let c = live_preview::compile(&doc, &config(&doc)).unwrap();
    assert_eq!(c.report.boundary_rearticulations, 1);
    assert!(
        c.report
            .warnings
            .iter()
            .any(|w| w.contains("smooth Yamaha"))
    );
    doc.routes[0].bytes[33] = 0;
    let c = live_preview::compile(&doc, &config(&doc)).unwrap();
    assert_eq!(c.report.note_ons, 2);
    assert_eq!(c.report.suppressed_notes, 1);
}
#[test]
fn timing_and_configuration_guards_are_strict() {
    let mut doc = fixture();
    let mut c = config(&doc);
    c.sources = vec![1, 1];
    assert!(live_preview::compile(&doc, &c).is_err());
    c.sources = vec![1];
    c.chords[0].bars = 0;
    assert!(live_preview::compile(&doc, &c).is_err());
    c.chords[0].bars = 1;
    doc.sections[0].end = 1919;
    assert!(live_preview::compile(&doc, &c).is_err());
}

#[test]
fn all_eleven_non_guitar_tables_compile_through_casm_without_document_mutation() {
    let mut doc = fixture();
    for table in 0..=10 {
        doc.routes[0].bytes[29] = table | 128;
        let before = doc.style.bytes().unwrap();
        let c = live_preview::compile(&doc, &config(&doc)).unwrap();
        assert_eq!(c.report.note_ons, 4, "NTT {table}");
        assert_eq!(
            c.plan
                .packets
                .iter()
                .filter(|p| p.data[0] & 0xf0 == 0x80)
                .count(),
            4
        );
        assert_eq!(doc.style.bytes().unwrap(), before);
    }
}

#[test]
fn transposed_bypass_matches_144_observed_jjazz_engine_cases() {
    // Actual released fitting-helper output, not a reconstructed formula.
    // Scope: Root Transposition + Bypass, no high-key wrap or range folding.
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/jjazz-bypass-oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let mut route = fixture().routes[0].bytes.clone();
        route[18] = case["sourceRoot"].as_u64().unwrap() as u8;
        route[19] = case["sourceChord"].as_u64().unwrap() as u8;
        for offset in [22, 28, 34] {
            route[offset] = 0;
            route[offset + 1] = 0;
        }
        let mut rows = vec![
            (0, vec![0xff, 6, 4, b'S', b'F', b'F', b'2']),
            (0, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']),
            (0, vec![0xb0, 0, 0]),
            (0, vec![0xb0, 32, 0]),
            (0, vec![0xc0, 0]),
        ];
        for pitch in 60..72 {
            let tick = u64::from(pitch - 60) * 120;
            rows.push((tick, vec![0x90, pitch, 80]));
            rows.push((tick + 60, vec![0x80, pitch, 0]));
        }
        rows.push((1920, vec![0xff, 0x2f, 0]));
        rows.sort_by_key(|row| row.0);
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
        let chunk = |tag: &[u8; 4], data| Chunk { tag: *tag, data };
        let bytes = genos_core::smf::pack(&[
            chunk(b"MThd", vec![0, 0, 0, 1, 1, 224]),
            chunk(b"MTrk", genos_core::smf::encode_track(&events).unwrap()),
            chunk(
                b"CASM",
                genos_core::smf::pack(&[chunk(
                    b"CSEG",
                    genos_core::smf::pack(&[
                        chunk(b"Sdec", b"Main A".to_vec()),
                        chunk(b"Ctb2", route),
                    ])
                    .unwrap(),
                )])
                .unwrap(),
            ),
        ])
        .unwrap();
        let doc = Document::parse(&bytes).unwrap();
        let mut cfg = config(&doc);
        cfg.chords = vec![Chord {
            root: case["targetRoot"].as_u64().unwrap() as u8,
            quality: case["targetQuality"].as_str().unwrap().into(),
            bars: 1,
            bass: None,
            bass_quality: None,
        }];
        let before = doc.style.bytes().unwrap();
        let compiled = live_preview::compile(&doc, &cfg).unwrap();
        let on = compiled
            .plan
            .packets
            .iter()
            .filter(|p| p.data[0] & 240 == 144 && p.data[2] > 0)
            .collect::<Vec<_>>();
        let expected: Vec<u8> = serde_json::from_value(case["pitches"].clone()).unwrap();
        assert_eq!(
            on.iter().map(|p| p.data[1]).collect::<Vec<_>>(),
            expected,
            "{case}"
        );
        assert_eq!(on.len(), 12);
        for (i, packet) in on.iter().enumerate() {
            assert_eq!(packet.data[0], 0x9f);
            assert_eq!(packet.data[2], 80);
            assert_eq!(packet.at.as_micros(), i as u128 * 125_000);
            assert!(
                compiled
                    .plan
                    .packets
                    .iter()
                    .any(|off| off.data == [0x8f, packet.data[1], 0]
                        && off.at.as_micros() == packet.at.as_micros() + 62_500)
            );
        }
        assert_eq!(doc.style.bytes().unwrap(), before);
    }
}

#[test]
fn transposed_bypass_accepts_valid_chord_bytes_and_retains_caps_limits_and_validation() {
    let mut doc = fixture();
    doc.routes[0].bytes[18] = 7;
    for offset in [22, 28, 34] {
        doc.routes[0].bytes[offset + 1] = 0;
    }
    let pitches = |doc: &Document| {
        live_preview::compile(doc, &config(doc))
            .unwrap()
            .plan
            .packets
            .iter()
            .filter(|p| p.data[0] & 240 == 144 && p.data[2] > 0)
            .map(|p| p.data[1])
            .collect::<Vec<_>>()
    };
    for source_chord in 0..=58 {
        doc.routes[0].bytes[19] = source_chord;
        assert_eq!(
            pitches(&doc),
            [53, 57, 62, 66],
            "source chord {source_chord}"
        );
    }
    doc.routes[0].bytes[19] = 59;
    assert!(live_preview::compile(&doc, &config(&doc)).is_err());
    doc.routes[0].bytes[19] = 17;
    doc.routes[0].bytes[29] = 2;
    assert!(live_preview::compile(&doc, &config(&doc)).is_err());
    doc.routes[0].bytes[29] = 0;
    doc.routes[0].bytes[18] = 0;
    for offset in [22, 28, 34] {
        doc.routes[0].bytes[offset + 2] = 4;
        doc.routes[0].bytes[offset + 3] = 60;
        doc.routes[0].bytes[offset + 4] = 72;
    }
    assert_eq!(pitches(&doc), [60, 64, 69, 61]);
}

#[test]
fn routed_bypass_applies_same_root_high_key_and_ordered_limits() {
    let mut doc = fixture();
    let section = doc
        .sections
        .iter()
        .find(|s| s.label == "Main A")
        .unwrap()
        .id;
    let source = doc.routes[0].source;
    let chord = genos_core::midi::LiveChord {
        root: 1,
        quality: "Major".into(),
        bass: None,
        bass_quality: None,
    };
    doc.routes[0].bytes[18] = 1;
    doc.routes[0].bytes[28..34].copy_from_slice(&[0, 0, 0, 0, 127, 1]);
    assert_eq!(
        live_preview::follow_pitch(&doc, section, source, 24, &chord),
        Some(12)
    );
    doc.routes[0].bytes[28..34].copy_from_slice(&[1, 0, 0, 61, 65, 1]);
    assert_eq!(
        live_preview::follow_pitch(&doc, section, source, 60, &chord),
        Some(60)
    );
    doc.routes[0].bytes[31..33].copy_from_slice(&[0, 0]);
    assert_eq!(
        live_preview::follow_pitch(&doc, section, source, 10, &chord),
        None
    );
    doc.routes[0].bytes[31..33].copy_from_slice(&[128 | 61, 65]);
    assert_eq!(
        live_preview::follow_pitch(&doc, section, source, 60, &chord),
        None
    );
    doc.routes[0].bytes[31..33].copy_from_slice(&[61, 128 | 65]);
    assert_eq!(
        live_preview::follow_pitch(&doc, section, source, 60, &chord),
        None
    );
    assert_eq!(
        live_preview::follow_pitch(&doc, section, source, 64, &chord),
        Some(64)
    );
}

#[test]
fn root_fixed_bypass_is_literal_for_pitched_parts_and_ignores_valid_source_chord() {
    let mut doc = fixture();
    doc.routes[0].bytes[28] = 1;
    doc.routes[0].bytes[29] = 0;
    doc.routes[0].bytes[19] = 58;
    let c = live_preview::compile(&doc, &config(&doc)).unwrap();
    let pitches = c
        .plan
        .packets
        .iter()
        .filter(|p| p.data[0] & 0xf0 == 0x90)
        .map(|p| p.data[1])
        .collect::<Vec<_>>();
    assert_eq!(pitches, vec![60, 64, 60, 64]);
    doc.routes[0].bytes[19] = 59;
    assert!(live_preview::compile(&doc, &config(&doc)).is_err());
}

#[test]
fn fixed_bypass_held_notes_continue_across_harmony_and_to_root_rtr_still_retargets() {
    let mut doc = fixture();
    doc.notes[0].duration = 3500;
    doc.sections[0].end = 3840;
    doc.routes[0].bytes[28] = 1;
    doc.routes[0].bytes[29] = 0;
    doc.routes[0].bytes[33] = 0;
    let c = live_preview::compile(&doc, &config(&doc)).unwrap();
    assert_eq!(c.report.note_ons, 2);
    assert_eq!(c.report.boundary_rearticulations, 0);
    // A changing, pitched route with RTR To Root must still follow the new root.
    doc.routes[0].bytes[28] = 0;
    doc.routes[0].bytes[29] = 2;
    doc.routes[0].bytes[33] = 4;
    doc.notes[0].pitch = 64;
    let c = live_preview::compile(&doc, &config(&doc)).unwrap();
    assert!(
        c.plan
            .packets
            .iter()
            .any(|p| p.at.as_millis() == 2000 && p.data == vec![0x9f, 69, 90])
    );
}

fn add_event(doc: &mut Document, tick: u64, data: Vec<u8>) {
    let id = doc
        .style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap_or(0)
        + 1;
    doc.style.tracks[0].push(Event {
        id,
        order: id,
        tick,
        data,
    });
}
fn sysex(payload: &[u8]) -> Vec<u8> {
    let mut b = vec![0xf0];
    genos_core::smf::write_vlq(payload.len() as u64, &mut b).unwrap();
    b.extend(payload);
    b
}
#[test]
fn channel_state_is_chased_at_section_wrap_without_replaying_programs() {
    let mut doc = fixture();
    add_event(&mut doc, 0, vec![0xb0, 10, 12]);
    add_event(&mut doc, 960, vec![0xb0, 10, 111]);
    let mut c = config(&doc);
    c.chords = vec![Chord {
        root: 0,
        quality: "Major".into(),
        bars: 2,
        bass: None,
        bass_quality: None,
    }];
    let result = live_preview::compile(&doc, &c).unwrap();
    assert!(
        result
            .plan
            .packets
            .iter()
            .any(|p| p.at.as_millis() == 2000 && p.data == vec![0xbf, 10, 12])
    );
    assert!(!result.plan.packets.iter().any(|p| p.data[0] & 0xf0 == 0xc0));
}
#[test]
fn typed_effect_setup_never_replays_proprietary_source_packets() {
    let mut doc = fixture();
    // Move musical section after a real SFF2 header.
    doc.sections[0].start = 100;
    doc.sections[0].end = 2020;
    for e in &mut doc.style.tracks[0] {
        if e.is_on() || e.is_off() || e.meta(6).is_some() || e.meta(0x2f).is_some() {
            e.tick += 100;
        }
    }
    for n in &mut doc.notes {
        n.start += 100;
    }
    add_event(&mut doc, 0, vec![0xff, 6, 4, b'S', b'F', b'F', b'2']);
    add_event(
        &mut doc,
        1,
        sysex(&[0x43, 0x10, 0x4c, 2, 1, 0, 1, 35, 0xf7]),
    );
    add_event(&mut doc, 2, sysex(&[0x43, 0x10, 0x4c, 2, 1, 12, 90, 0xf7]));
    add_event(&mut doc, 3, sysex(&[0x43, 0x76, 0, 1, 0xf7]));
    let mut c = config(&doc);
    c.effects = true;
    let result = live_preview::compile(&doc, &c).unwrap();
    assert!(
        result
            .plan
            .setup
            .contains(&vec![0xf0, 0x43, 0x10, 0x4c, 2, 1, 0, 1, 35, 0xf7])
    );
    assert!(
        result
            .plan
            .setup
            .contains(&vec![0xf0, 0x43, 0x10, 0x4c, 2, 1, 12, 90, 0xf7])
    );
    assert!(
        !result
            .plan
            .setup
            .iter()
            .any(|p| p.starts_with(&[0xf0, 0x43, 0x76]))
    );
}
#[test]
fn factory_kit_program_precedes_mode_and_per_key_setup_with_reset_validation() {
    let mut doc = fixture();
    doc.sections[0].start = 100;
    doc.sections[0].end = 2020;
    for e in &mut doc.style.tracks[0] {
        if e.is_on() || e.is_off() || e.meta(6).is_some() || e.meta(0x2f).is_some() {
            e.tick += 100;
        }
    }
    for n in &mut doc.notes {
        n.channel = 10;
        n.start += 100;
    }
    for e in &mut doc.style.tracks[0] {
        if e.channel().is_some() {
            e.data[0] = (e.data[0] & 0xf0) | 9;
            if e.data[0] == 0xb9 && e.data[1] == 0 {
                e.data[2] = 127;
            }
        }
    }
    let r = &mut doc.routes[0];
    r.source = 10;
    r.destination = 10;
    r.bytes[0] = 9;
    r.bytes[9] = 9;
    for o in [22, 28, 34] {
        r.bytes[o] = 1;
        r.bytes[o + 1] = 0;
    }
    add_event(&mut doc, 0, vec![0xff, 6, 4, b'S', b'F', b'F', b'2']);
    add_event(&mut doc, 1, sysex(&[0x43, 0x10, 0x4c, 8, 9, 7, 2, 0xf7]));
    add_event(
        &mut doc,
        2,
        sysex(&[0x43, 0x10, 0x4c, 0x30, 60, 2, 110, 0xf7]),
    );
    let mut c = config(&doc);
    c.sources = vec![10];
    c.drums = true;
    let result = live_preview::compile(&doc, &c).unwrap();
    let setup = &result.plan.setup;
    let pc = setup.iter().position(|p| p == &vec![0xc9, 4]).unwrap();
    let mode = setup
        .iter()
        .position(|p| p == &vec![0xf0, 0x43, 0x10, 0x4c, 8, 9, 7, 2, 0xf7])
        .unwrap();
    let param = setup
        .iter()
        .position(|p| p == &vec![0xf0, 0x43, 0x10, 0x4c, 0x30, 60, 2, 110, 0xf7])
        .unwrap();
    assert!(pc < mode && mode < param);
    add_event(&mut doc, 3, sysex(&[0x43, 0x10, 0x4c, 0, 0, 0x7e, 0, 0xf7]));
    assert!(live_preview::compile(&doc, &c).is_err());
}
#[test]
fn converted_overlaps_coalesce_without_unpaired_output() {
    let mut doc = fixture();
    let mut n = doc.notes[0].clone();
    n.start = 100;
    n.duration = 700;
    n.velocity = 50;
    doc.notes.push(n);
    let result = live_preview::compile(&doc, &config(&doc)).unwrap();
    assert_eq!(result.report.coalesced_note_ons, 2);
    let mut active = std::collections::BTreeSet::new();
    for p in result.plan.packets {
        match p.data[0] & 0xf0 {
            0x90 => assert!(active.insert((p.data[0] & 15, p.data[1]))),
            0x80 => assert!(active.remove(&(p.data[0] & 15, p.data[1]))),
            _ => {}
        }
    }
    assert!(active.is_empty());
}

#[test]
fn supplied_corpus_is_either_fully_routed_or_explicitly_rejected_without_mutation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let corpus = root.join("docs/research/artifacts/style files");
    let mut rows = Vec::new();
    let mut accepted = 0;
    let mut rejected = 0;
    for entry in std::fs::read_dir(corpus).unwrap() {
        let file = entry.unwrap().path();
        if file
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("sty"))
        {
            continue;
        }
        let bytes = std::fs::read(&file).unwrap();
        let doc = Document::parse(&bytes).unwrap();
        let before = doc.style.bytes().unwrap();
        let section = doc.sections.iter().find(|s| s.label == "Main A").unwrap();
        let mut c = config(&doc);
        c.section = section.id;
        c.sources = doc
            .notes
            .iter()
            .filter(|n| n.start < section.end && n.start + n.duration > section.start)
            .map(|n| n.channel)
            .collect();
        c.sources.sort();
        c.sources.dedup();
        let result = live_preview::compile(&doc, &c);
        let row = match result {
            Ok(compiled) => {
                accepted += 1;
                assert_eq!(compiled.report.routes.len(), c.sources.len());
                let mut active = std::collections::BTreeSet::new();
                for p in &compiled.plan.packets {
                    match p.data[0] & 0xf0 {
                        0x90 => assert!(active.insert((p.data[0] & 15, p.data[1]))),
                        0x80 => assert!(active.remove(&(p.data[0] & 15, p.data[1]))),
                        _ => {}
                    }
                }
                assert!(active.is_empty());
                serde_json::json!({"file":file.file_name().unwrap().to_string_lossy(),"status":"compiled","report":compiled.report})
            }
            Err(e) => {
                rejected += 1;
                serde_json::json!({"file":file.file_name().unwrap().to_string_lossy(),"status":"rejected","reason":e.to_string()})
            }
        };
        rows.push(row);
        assert_eq!(doc.style.bytes().unwrap(), before);
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
    }
    assert!(accepted > 0 && rejected > 0);
    rows.sort_by_key(|r| r["file"].as_str().unwrap().to_owned());
    let report = serde_json::json!({"accepted":accepted,"rejected":rejected,"total":rows.len(),"scope":"Main A, all note sources, C Major and A Minor, typed effects/drum fields off; strict unsupported rejection","rows":rows});
    std::fs::write(
        root.join("docs/development/live-preview/corpus-matrix.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
fn unclosed_sources_megavoices_and_mid_section_voice_or_mode_changes_reject() {
    let mut doc = fixture();
    add_event(&mut doc, 100, vec![0x90, 70, 90]);
    assert!(live_preview::compile(&doc, &config(&doc)).is_err());
    doc.style.tracks[0].pop();
    for data in [
        vec![0xb0, 0, 8],
        vec![0xc0, 15],
        vec![0xb0, 123, 0],
        vec![0xa0, 60, 90],
    ] {
        add_event(&mut doc, 100, data);
        assert!(live_preview::compile(&doc, &config(&doc)).is_err());
        doc.style.tracks[0].pop();
    }
    doc.style.tracks[0][0].data[2] = 8;
    assert!(live_preview::compile(&doc, &config(&doc)).is_err());
}

#[test]
fn slash_bass_substitutes_target_only_in_the_notes_active_flagged_region() {
    let mut doc = fixture();
    let before = doc.style.bytes().unwrap();
    let mut c = config(&doc);
    c.chords.truncate(1);
    c.chords[0].bass = Some(2);
    let pitches = |d: &Document, c: &Config| {
        live_preview::compile(d, c)
            .unwrap()
            .plan
            .packets
            .into_iter()
            .filter(|p| p.data[0] & 0xf0 == 0x90)
            .map(|p| p.data[1])
            .collect::<Vec<_>>()
    };
    assert_eq!(pitches(&doc, &c), vec![60, 64]);
    doc.routes[0].bytes[29] |= 128;
    assert_eq!(pitches(&doc, &c), vec![62, 66]);
    doc.routes[0].bytes[20] = 61;
    doc.routes[0].bytes[21] = 63;
    doc.routes[0].bytes[23] |= 128;
    assert_eq!(pitches(&doc, &c), vec![62, 64]);
    assert_eq!(doc.style.bytes().unwrap(), before);
    let report = live_preview::compile(&doc, &c).unwrap().report;
    assert_eq!(report.steps[0].label, "C Major/D");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("NTT Bass enabled"))
    );
}
#[test]
fn omitted_or_root_bass_is_backward_compatible_and_invalid_bass_rejects() {
    let doc = fixture();
    let legacy = serde_json::json!({"section":doc.sections[0].id,"sources":[1],"chords":[{"root":0,"quality":"Major","bars":1}],"effects":false,"drums":false});
    let mut c: Config = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(c.chords[0].bass, None);
    let a = live_preview::compile(&doc, &c).unwrap();
    c.chords[0].bass = Some(0);
    let b = live_preview::compile(&doc, &c).unwrap();
    assert_eq!(
        a.plan
            .packets
            .iter()
            .map(|p| (&p.at, &p.data))
            .collect::<Vec<_>>(),
        b.plan
            .packets
            .iter()
            .map(|p| (&p.at, &p.data))
            .collect::<Vec<_>>()
    );
    assert_eq!(b.report.steps[0].label, "C Major");
    c.chords[0].bass = Some(12);
    assert!(live_preview::compile(&doc, &c).is_err());
    let mut negative = legacy;
    negative["chords"][0]["bass"] = serde_json::json!(-1);
    assert!(serde_json::from_value::<Config>(negative).is_err());
}
#[test]
fn repeated_identical_chords_preserve_held_notes_even_for_rtr_stop() {
    let mut doc = fixture();
    doc.sections[0].end = 3840;
    doc.notes[0].duration = 3500;
    doc.routes[0].bytes[33] = 0;
    let mut c = config(&doc);
    c.chords[1] = c.chords[0].clone();
    c.chords[1].bass = Some(0);
    let result = live_preview::compile(&doc, &c).unwrap();
    assert_eq!(result.report.steps.len(), 1);
    assert_eq!(result.report.duration_ms, 4000);
    assert_eq!(result.report.boundary_rearticulations, 0);
    assert_eq!(result.report.note_ons, 2);
    assert_eq!(result.report.suppressed_notes, 0);
    assert!(
        !result
            .plan
            .packets
            .iter()
            .any(|p| p.at.as_millis() == 2000 && matches!(p.data[0] & 0xf0, 0x80 | 0x90))
    );
    assert!(
        result
            .plan
            .packets
            .iter()
            .any(|p| p.at.as_millis() == 3645 && p.data == vec![0x8f, 60, 0])
    );
}
#[test]
fn slash_only_changes_preserve_unflagged_parts_but_retarget_flagged_parts() {
    let mut doc = fixture();
    doc.sections[0].end = 3840;
    doc.notes[0].duration = 3500;
    doc.routes[0].bytes[33] = 3;
    let mut c = config(&doc);
    c.chords[1] = c.chords[0].clone();
    c.chords[1].bass = Some(4);
    let unflagged = live_preview::compile(&doc, &c).unwrap();
    assert_eq!(unflagged.report.note_ons, 2);
    assert_eq!(unflagged.report.boundary_rearticulations, 0);
    doc.routes[0].bytes[29] |= 128;
    let flagged = live_preview::compile(&doc, &c).unwrap();
    assert_eq!(flagged.report.boundary_rearticulations, 1);
    assert!(
        flagged
            .plan
            .packets
            .iter()
            .any(|p| p.at.as_millis() == 2000 && p.data == vec![0x9f, 64, 90])
    );
}
#[test]
fn repeated_spans_use_wide_bar_counts_and_keep_source_cycle_restarts() {
    let doc = fixture();
    let mut c = config(&doc);
    c.chords = vec![
        Chord {
            root: 0,
            quality: "Major".into(),
            bars: 8,
            bass: None,
            bass_quality: None,
        };
        32
    ];
    let result = live_preview::compile(&doc, &c).unwrap();
    assert_eq!(result.report.duration_ms, 512000);
    assert_eq!(result.report.steps.len(), 1);
    assert_eq!(result.report.note_ons, 512);
}

#[test]
fn chord_masks_gate_notes_without_discarding_the_controller_timeline() {
    let mut doc = fixture();
    doc.routes[0].bytes[17] &= !1;
    add_event(&mut doc, 200, vec![0xb0, 7, 40]);
    let result = live_preview::compile(&doc, &config(&doc)).unwrap();
    assert_eq!(result.report.note_ons, 2);
    assert_eq!(result.report.suppressed_notes, 2);
    assert!(
        result
            .plan
            .packets
            .iter()
            .any(|p| p.at.as_millis() == 208 && p.data == vec![0xbf, 7, 40])
    );
}
#[test]
fn fixed_drum_held_notes_continue_across_changes_in_harmony() {
    let mut doc = fixture();
    doc.sections[0].end = 3840;
    doc.notes[0].duration = 3500;
    // Change the explicit source kit and bind its fixed/bypass rhythm route.
    doc.style.tracks[0][0].data[2] = 127;
    let r = &mut doc.routes[0];
    r.destination = 10;
    r.bytes[9] = 9;
    for o in [22, 28, 34] {
        r.bytes[o] = 1;
        r.bytes[o + 1] = 0;
        r.bytes[o + 5] = 0;
    }
    let result = live_preview::compile(&doc, &config(&doc)).unwrap();
    assert_eq!(result.report.note_ons, 2);
    assert_eq!(result.report.suppressed_notes, 0);
    assert!(
        !result
            .plan
            .packets
            .iter()
            .any(|p| p.at.as_millis() == 2000 && matches!(p.data[0] & 0xf0, 0x80 | 0x90))
    );
}
