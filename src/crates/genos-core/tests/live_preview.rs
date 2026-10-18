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
            },
            Chord {
                root: 9,
                quality: "Minor".into(),
                bars: 1,
                bass: None,
            },
        ],
        effects: false,
        drums: false,
    }
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
    }];
    assert!(live_preview::compile(&doc, &conf).is_ok());
}

#[test]
fn note_mute_filters_chord_root_not_source_pitch_or_slash_bass_and_preserves_controls() {
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
            },
            Chord {
                root: (root + 1) % 12,
                quality: "Minor".into(),
                bars: 1,
                bass: Some(root),
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
    }];
    let a = live_preview::compile(&doc, &c).unwrap();
    doc.routes[0].bytes[13] ^= 4;
    let b = live_preview::compile(&doc, &c).unwrap();
    assert_eq!(a.report.note_ons, b.report.note_ons);
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
fn root_fixed_bypass_is_literal_for_pitched_parts_and_ignores_valid_source_chord() {
    let mut doc = fixture();
    doc.routes[0].bytes[28] = 1;
    doc.routes[0].bytes[29] = 0;
    doc.routes[0].bytes[19] = 33;
    let c = live_preview::compile(&doc, &config(&doc)).unwrap();
    let pitches = c
        .plan
        .packets
        .iter()
        .filter(|p| p.data[0] & 0xf0 == 0x90)
        .map(|p| p.data[1])
        .collect::<Vec<_>>();
    assert_eq!(pitches, vec![60, 64, 60, 64]);
    doc.routes[0].bytes[19] = 34;
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
            bass: None
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
