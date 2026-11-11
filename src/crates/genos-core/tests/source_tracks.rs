use genos_core::{
    casm::{Region, RouteChange as Change, RuleField, SourceAction as Action},
    document::Document,
    edit::{Command, Session},
    smf::{self, Chunk, Event},
};

fn chunk(tag: &[u8; 4], data: Vec<u8>) -> Chunk {
    Chunk { tag: *tag, data }
}
fn route(source: u8) -> Vec<u8> {
    let mut b = vec![0; 47];
    b[0] = source - 1;
    b[9] = 8;
    b[13] = 0xfc;
    b[17] = 0x81;
    b[21] = 127;
    b[46] = 0x55;
    for o in [22, 28, 34] {
        b[o + 4] = 127;
    }
    b[29] = 0x8a;
    b
}
fn fixture(malformed: bool) -> Vec<u8> {
    let mut es = Vec::new();
    let mut add = |tick, data| {
        let id = es.len() as u64 + 1;
        es.push(Event {
            id,
            tick,
            order: id,
            data,
        });
    };
    add(0, vec![0xb0, 0, 121]);
    add(0, vec![0xb0, 32, 5]);
    add(0, vec![0xc0, 34]);
    add(10, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']);
    add(10, vec![0xb0, 7, 90]);
    add(10, vec![0xe0, 0, 64]);
    add(20, vec![0x90, 60, 100]);
    add(100, vec![0x80, 60, 0]);
    add(30, vec![0x91, 62, 80]);
    add(120, vec![0x81, 62, 0]);
    add(480, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'B']);
    add(500, vec![0x90, 64, 90]);
    add(600, vec![0x80, 64, 0]);
    add(960, vec![0xff, 0x2f, 0]);
    let mut records = vec![
        chunk(b"Sdec", b"Main A,Main B".to_vec()),
        chunk(b"Ctb2", route(1)),
        chunk(b"Xtra", vec![0xde, 0xad, 0xbe, 0xef]),
        chunk(b"Ctb2", route(2)),
    ];
    if malformed {
        records.push(chunk(b"Ctb2", vec![0; 12]));
    }
    smf::pack(&[
        chunk(b"MThd", vec![0, 0, 0, 1, 1, 224]),
        chunk(b"MTrk", smf::encode_track(&es).unwrap()),
        chunk(
            b"CASM",
            smf::pack(&[
                chunk(b"CSEG", smf::pack(&records).unwrap()),
                chunk(b"Tail", vec![4, 5, 6]),
            ])
            .unwrap(),
        ),
        chunk(b"OTSc", vec![99, 88]),
    ])
    .unwrap()
}
fn session(malformed: bool) -> Session {
    Session::open(&fixture(malformed), "Synthetic source-track test".into()).unwrap()
}
fn apply(s: &mut Session, actions: Vec<Action>) -> genos_core::Result<()> {
    let section = s.document.sections[0].id;
    s.execute(s.revision, Command::SourceTracks { section, actions })
}
fn edit(changes: Vec<Change>, chords: Option<Vec<u8>>, exclusive: bool) -> Action {
    Action::Edit {
        source: 1,
        destination: 9,
        changes,
        chords,
        exclusive,
    }
}
fn mask(b: &[u8], c: u8) -> bool {
    b[17 - (c / 8) as usize] & (1 << (c % 8)) != 0
}

#[test]
fn edits_short_and_extended_ctb2_without_changing_opaque_bytes() {
    for size in [40, 57, 128] {
        let mut style = smf::Style::parse(&fixture(false)).unwrap();
        style.tracks[0].push(Event {
            id: u64::MAX,
            order: u64::MAX,
            tick: 0,
            data: vec![0xff, 6, 4, b'S', b'F', b'F', b'2'],
        });
        style.chunks[style.track_chunks[0]].data = smf::encode_track(&style.tracks[0]).unwrap();
        let casm = style.chunks.iter_mut().find(|c| c.tag == *b"CASM").unwrap();
        let mut groups = smf::chunks(&casm.data).unwrap();
        let mut records = smf::chunks(&groups[0].data).unwrap();
        records[1].data.resize(size, 0xa5);
        let original = records[1].data.clone();
        groups[0].data = smf::pack(&records).unwrap();
        casm.data = smf::pack(&groups).unwrap();
        let before = style.bytes().unwrap();
        let mut s = Session::open(&before, "Variable Ctb2 payload".into()).unwrap();

        apply(&mut s, vec![edit(vec![Change::Root { value: 7 }], None, false)]).unwrap();
        let reopened = Document::parse(&s.document.style.bytes().unwrap()).unwrap();
        let mut expected = original.clone();
        expected[18] = 7;
        assert_eq!(selected(&reopened, "Main A", 1), &expected[..size.min(57)]);
        assert_eq!(selected(&reopened, "Main B", 1), &original[..size.min(57)]);
        let saved = reopened.style.chunks.iter().find(|c| c.tag == *b"CASM").unwrap();
        let groups = smf::chunks(&saved.data).unwrap();
        let edited = smf::chunks(&groups[1].data).unwrap();
        assert_eq!(edited[0].data, b"Main A");
        assert_eq!(edited[1].data, expected);
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), before);
    }
}

#[test]
fn sff2_authoring_targets_the_playback_winner_and_rejects_shadowed_routes() {
    let mut style = smf::Style::parse(&fixture(false)).unwrap();
    let track = style.track_chunks[0];
    style.tracks[0].push(Event {
        id: u64::MAX,
        order: u64::MAX,
        tick: 0,
        data: vec![0xff, 6, 4, b'S', b'F', b'F', b'2'],
    });
    style.chunks[track].data = smf::encode_track(&style.tracks[0]).unwrap();
    let mut shadow = route(1);
    shadow[9] = 15;
    let mut earlier = route(1);
    earlier[18] = 4;
    let winner = route(1);
    let sibling = route(2);
    let group = |names: &[u8], routes: &[Vec<u8>]| {
        let mut records = vec![chunk(b"Sdec", names.to_vec())];
        records.extend(routes.iter().map(|route| chunk(b"Ctb2", route.clone())));
        chunk(b"CSEG", smf::pack(&records).unwrap())
    };
    let casm = style.chunks.iter_mut().find(|c| c.tag == *b"CASM").unwrap();
    let shadow_group = group(b"Main A", &[shadow, sibling.clone(), route(3)]);
    let shared = chunk(b"CSEG", smf::pack(&[
        chunk(b"Sdec", b"Main A".to_vec()),
        chunk(b"Ctb2", earlier.clone()),
        chunk(b"Sdec", b"Main B".to_vec()),
        chunk(b"Ctb2", winner.clone()),
        chunk(b"Ctb2", sibling.clone()),
        chunk(b"Sdec", b"Main C".to_vec()),
    ]).unwrap());
    casm.data = smf::pack(&[
        shared,
        shadow_group.clone(),
    ]).unwrap();
    let before = style.bytes().unwrap();
    let mut s = Session::open(&before, "Overlapping CASM".into()).unwrap();
    assert!(apply(&mut s, vec![Action::Edit {
        source: 1,
        destination: 16,
        changes: vec![Change::Root { value: 7 }],
        chords: None,
        exclusive: false,
    }]).is_err());
    assert_eq!(s.document.style.bytes().unwrap(), before);

    apply(&mut s, vec![edit(vec![Change::Root { value: 7 }], Some(vec![0]), true)]).unwrap();
    let edited = Document::parse(&s.document.style.bytes().unwrap()).unwrap();
    let casm = edited.style.chunks.iter().find(|c| c.tag == *b"CASM").unwrap();
    let groups = smf::chunks(&casm.data).unwrap();
    assert_eq!(groups.len(), 3);
    let main_b = smf::chunks(&groups[0].data).unwrap();
    let main_a = smf::chunks(&groups[1].data).unwrap();
    assert_eq!(main_b[0].data, b"Main B");
    assert_eq!(main_b[2].data, winner);
    assert_eq!(main_b[3].data, sibling);
    assert_eq!(main_b[4].data, b"Main C");
    assert_eq!(main_a[0].data, b"Main A");
    assert_eq!(main_a[1].data, earlier);
    assert_eq!(main_a[2].data[18], 7);
    assert!(mask(&main_a[2].data, 0));
    assert!(!mask(&main_a[3].data, 0));
    assert_eq!(groups[2].data, shadow_group.data);
    if let Some(path) = std::env::var_os("GENOS_AUTHORED_CASM_REPLAY") {
        std::fs::write(path, smf::pack(std::slice::from_ref(casm)).unwrap()).unwrap();
    }
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}

#[test]
fn firmware_named_sdec_groups_support_section_edits_and_clones() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY");
    let fixture = std::fs::read(fixture).unwrap();
    let export = std::env::var_os("GENOS_SDEC_AUTHORING_REPLAY").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        std::fs::create_dir_all(dir).unwrap();
    }
    let mut donor = route(11);
    donor[9] = 10;
    let mut sibling = donor.clone();
    sibling[0] = 11;
    for (name, leading, middle, alias) in [
        ("embedded", &b"XMain A!"[..], Some(&b"Main A\0Main B\0"[..]), false),
        ("single", &b"Main A\0Main B!"[..], None, false),
        ("suffix", &b"Main A "[..], Some(&b"xMain B?"[..]), true),
        ("unshared", &b"XMain A\0"[..], None, false),
    ] {
        let mut style = smf::Style::parse(&fixture).unwrap();
        let mut records = vec![
            chunk(b"Sdec", leading.to_vec()),
            chunk(b"Ctb2", donor.clone()),
        ];
        if let Some(middle) = middle {
            records.push(chunk(b"Sdec", middle.to_vec()));
        }
        records.push(chunk(b"Ctb2", sibling.clone()));
        let trailing = b"\0XMain C!";
        records.push(chunk(b"Sdec", trailing.to_vec()));
        let mut groups = vec![chunk(b"CSEG", smf::pack(&records).unwrap())];
        if name == "unshared" {
            groups.push(chunk(b"CSEG", smf::pack(&[
                chunk(b"Sdec", b"Main B".to_vec()),
                chunk(b"Ctb2", donor.clone()),
                chunk(b"Ctb2", sibling.clone()),
            ]).unwrap()));
        }
        style.chunks.iter_mut().find(|c| c.tag == *b"CASM").unwrap().data =
            smf::pack(&groups).unwrap();
        let label = if alias {
            let marker = style.tracks[0].iter_mut()
                .find(|event| event.meta(6) == Some(b"Main A")).unwrap();
            marker.data = b"\xff\x06\x07Main AX".to_vec();
            style.chunks[style.track_chunks[0]].data = smf::encode_track(&style.tracks[0]).unwrap();
            "Main AX"
        } else {
            "Main A"
        };
        let before = style.bytes().unwrap();
        let mut s = Session::open(&before, name.into()).unwrap();
        let section = s.document.sections.iter().find(|section| section.label == label).unwrap();
        let section = section.id;
        assert_eq!(selected(&s.document, label, 11), donor);
        s.execute(s.revision, Command::SourceTracks {
            section,
            actions: vec![Action::Edit {
                source: 11,
                destination: 11,
                changes: vec![Change::Root { value: 7 }],
                chords: Some(vec![0]),
                exclusive: true,
            }],
        }).unwrap();
        let edited = s.document.style.bytes().unwrap();
        let reopened = Document::parse(&edited).unwrap();
        assert_eq!(selected(&reopened, label, 11)[18], 7, "{name}");
        assert!(!mask(selected(&reopened, label, 12), 0));
        assert_eq!(selected(&reopened, "Main B", 11), donor);
        assert_eq!(selected(&reopened, "Main B", 12), sibling);

        s.execute(s.revision, Command::SourceTracks {
            section,
            actions: vec![Action::Clone { source: 11, destination: 11, target: 1 }],
        }).unwrap();
        let cloned = s.document.style.bytes().unwrap();
        let reopened = Document::parse(&cloned).unwrap();
        assert_eq!(selected(&reopened, label, 1)[18], 7);
        assert!(reopened.notes.iter().any(|note| note.channel == 1));
        let casm = reopened.style.chunks.iter().find(|c| c.tag == *b"CASM").unwrap();
        let groups = smf::chunks(&casm.data).unwrap();
        let first = smf::chunks(&groups[0].data).unwrap();
        assert_eq!(first.last().unwrap().data, trailing);
        if name == "unshared" {
            assert_eq!(first[0].data, leading);
        }
        if let Some(dir) = &export {
            for (state, bytes) in [("before", &before), ("edited", &edited), ("cloned", &cloned)] {
                std::fs::write(dir.join(format!("{name}-{state}.STY")), bytes).unwrap();
            }
        }
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), edited);
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), before);
        if name == "unshared" {
            records.insert(2, chunk(b"Sdec", b"[Main A]".to_vec()));
            let casm = style.chunks.iter_mut().find(|c| c.tag == *b"CASM").unwrap();
            let mut groups = smf::chunks(&casm.data).unwrap();
            groups[0].data = smf::pack(&records).unwrap();
            let mut other = smf::chunks(&groups[1].data).unwrap();
            other[1].data[18] = 3;
            groups[1].data = smf::pack(&other).unwrap();
            other[0].data = b"Main C".to_vec();
            other[1].data[18] = 5;
            groups.push(chunk(b"CSEG", smf::pack(&other).unwrap()));
            casm.data = smf::pack(&groups).unwrap();
            let duplicate = style.bytes().unwrap();
            let mut session = Session::open(&duplicate, "Repeated CASM binding".into()).unwrap();
            for label in ["Main A", "Main B", "Main C"] {
                let section = session.document.sections.iter().find(|s| s.label == label).unwrap().id;
                let result = session.execute(session.revision, Command::SourceTracks {
                    section,
                    actions: vec![Action::Edit {
                        source: 11,
                        destination: 11,
                        changes: vec![Change::Root { value: 7 }],
                        chords: None,
                        exclusive: false,
                    }],
                });
                if label == "Main C" {
                    result.unwrap();
                    assert_ne!(session.document.style.bytes().unwrap(), duplicate);
                    session.undo().unwrap();
                } else {
                    assert!(result.unwrap_err().to_string().contains("ambiguous section binding"));
                }
                assert_eq!(session.document.style.bytes().unwrap(), duplicate);
            }
            if let Some(dir) = &export {
                std::fs::write(dir.join("duplicate.STY"), &duplicate).unwrap();
            }
        }
        if alias {
            let marker = style.tracks[0].iter_mut()
                .find(|event| event.meta(6) == Some(b"Main C")).unwrap();
            marker.data = b"\xff\x06\x07Main AZ".to_vec();
            style.chunks[style.track_chunks[0]].data = smf::encode_track(&style.tracks[0]).unwrap();
            let ambiguous = style.bytes().unwrap();
            let mut session = Session::open(&ambiguous, "Duplicate firmware marker".into()).unwrap();
            assert!(session.execute(session.revision, Command::SourceTracks {
                section,
                actions: vec![Action::Edit {
                    source: 11,
                    destination: 11,
                    changes: vec![Change::Root { value: 7 }],
                    chords: None,
                    exclusive: false,
                }],
            }).is_err());
            assert_eq!(session.document.style.bytes().unwrap(), ambiguous);
        }
    }
}

#[test]
fn gui_saved_stylemagic_root_and_defaults_match_real_editor_transactions() {
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/stylemagic-gui-oracle.json")).unwrap();
    let payload = |key: &str| -> Vec<u8> { serde_json::from_value(oracle[key].clone()).unwrap() };
    for (key, destination, changes) in [
        ("rootG", 11, vec![Change::Root { value: 7 }]),
        ("bassDefaults", 11, vec![Change::PartDefaults]),
        ("rhythmDefaults", 9, vec![Change::PartDefaults]),
    ] {
        let mut before = payload("baseline");
        before[9] = destination - 1;
        let events = smf::encode_track(&[
            Event {
                id: 1,
                tick: 0,
                order: 1,
                data: vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A'],
            },
            Event {
                id: 2,
                tick: 480,
                order: 2,
                data: vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'B'],
            },
            Event {
                id: 3,
                tick: 960,
                order: 3,
                data: vec![0xff, 0x2f, 0],
            },
        ])
        .unwrap();
        let records = smf::pack(&[
            chunk(b"Sdec", b"Main A,Main B".to_vec()),
            chunk(b"Ctb2", before.clone()),
            chunk(b"Xtra", vec![1, 3, 5, 7]),
        ])
        .unwrap();
        let data = smf::pack(&[
            chunk(b"MThd", vec![0, 0, 0, 1, 1, 224]),
            chunk(b"MTrk", events),
            chunk(b"CASM", smf::pack(&[chunk(b"CSEG", records)]).unwrap()),
        ])
        .unwrap();
        let mut s = Session::open(&data, "Observed GUI CASM records".into()).unwrap();
        let section = s
            .document
            .sections
            .iter()
            .find(|s| s.label == "Main A")
            .unwrap()
            .id;
        s.execute(
            s.revision,
            Command::SourceTracks {
                section,
                actions: vec![Action::Edit {
                    source: 6,
                    destination,
                    changes,
                    chords: None,
                    exclusive: false,
                }],
            },
        )
        .unwrap();
        assert_eq!(selected(&s.document, "Main A", 6), payload(key), "{key}");
        assert_eq!(
            selected(&s.document, "Main B", 6),
            before,
            "other section {key}"
        );
        let saved = s.document.style.bytes().unwrap();
        let reopened = Session::open(&saved, "Reopened observed record".into()).unwrap();
        assert_eq!(selected(&reopened.document, "Main A", 6), payload(key));
        assert!(saved.windows(4).any(|b| b == [1, 3, 5, 7]));
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), data, "exact undo {key}");
    }
}

#[test]
fn part_defaults_preserve_reserved_fields_and_allow_later_overrides_atomically() {
    let mut s = session(false);
    let original = s.document.style.bytes().unwrap();
    let before = selected(&s.document, "Main A", 1).to_vec();
    apply(
        &mut s,
        vec![edit(
            vec![
                Change::PartDefaults,
                Change::Root { value: 7 },
                Change::AutoStart { enabled: false },
            ],
            None,
            false,
        )],
    )
    .unwrap();
    let after = selected(&s.document, "Main A", 1);
    assert_eq!(after[13] & 0xf8, before[13] & 0xf8);
    assert_eq!(after[11] & 0xf0, before[11] & 0xf0);
    assert_eq!(&after[..11], &before[..11]);
    assert_eq!(&after[40..], &before[40..]);
    assert_eq!(after[18], 7);
    assert_eq!(after[13] & 4, 0);
    for offset in [22, 28, 34] {
        assert_eq!(&after[offset..offset + 6], &[1, 0, 6, 0, 127, 1]);
    }
    for chord in 0..34 {
        assert!(mask(after, chord));
    }
    assert_eq!(selected(&s.document, "Main B", 1), before);
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), original);
    assert!(
        apply(
            &mut s,
            vec![edit(
                vec![Change::PartDefaults, Change::Root { value: 12 }],
                None,
                false
            )]
        )
        .is_err()
    );
    assert_eq!(s.document.style.bytes().unwrap(), original);
}
fn selected<'a>(d: &'a Document, label: &str, source: u8) -> &'a [u8] {
    &d.routes
        .iter()
        .find(|r| r.sections.iter().any(|s| s == label) && r.source == source)
        .unwrap()
        .bytes
}

#[test]
fn source_regions_split_shared_groups_preserve_unknowns_and_undo_exactly() {
    let mut s = session(false);
    let before = s.document.style.bytes().unwrap();
    apply(
        &mut s,
        vec![edit(
            vec![
                Change::Root { value: 7 },
                Change::Zone { low: 36, high: 84 },
                Change::Rule {
                    region: Region::Middle,
                    field: RuleField::Ntr,
                    value: 2,
                },
                Change::Rule {
                    region: Region::High,
                    field: RuleField::NoteLow,
                    value: 10,
                },
            ],
            None,
            false,
        )],
    )
    .unwrap();
    let b = selected(&s.document, "Main A", 1);
    assert_eq!(b[18], 7);
    assert_eq!(&b[20..22], &[36, 84]);
    assert_eq!(b[28], 2);
    assert_eq!(b[29], 128);
    assert_eq!(b[37], 10);
    assert_eq!(b[46], 0x55);
    assert_eq!(selected(&s.document, "Main B", 1), route(1));
    assert_eq!(selected(&s.document, "Main A", 2), route(2));
    let original = Document::parse(&before).unwrap();
    for (a, b) in original.style.chunks.iter().zip(&s.document.style.chunks) {
        if a.tag != *b"CASM" {
            assert_eq!(a, b);
        }
    }
    let groups = smf::chunks(&s.document.style.chunks[2].data).unwrap();
    assert_eq!(groups.last().unwrap(), &chunk(b"Tail", vec![4, 5, 6]));
    for group in groups.iter().filter(|g| g.tag == *b"CSEG") {
        assert!(
            smf::chunks(&group.data)
                .unwrap()
                .contains(&chunk(b"Xtra", vec![0xde, 0xad, 0xbe, 0xef]))
        );
    }
    let exported = s.document.style.bytes().unwrap();
    let reopened = Document::parse(&exported).unwrap();
    assert_eq!(selected(&reopened, "Main A", 1), b);
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
    s.redo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), exported);
}

#[test]
fn root_mask_and_autostart_follow_reference_bits_and_preserve_sections_reserved_bits_and_undo() {
    let mut s = session(false);
    let before = s.document.style.bytes().unwrap();
    for root in 0..12 {
        let rev = s.revision;
        apply(
            &mut s,
            vec![edit(
                vec![Change::RootEnabled {
                    root,
                    enabled: true,
                }],
                None,
                false,
            )],
        )
        .unwrap();
        let a = selected(&s.document, "Main A", 1);
        assert_eq!(a[12 - (root / 8) as usize], 1 << (root % 8));
        assert_eq!(&a[13..], &route(1)[13..]);
        assert_eq!(selected(&s.document, "Main B", 1), route(1));
        assert_eq!(selected(&s.document, "Main A", 2), route(2));
        s.undo().unwrap();
        assert!(s.revision > rev);
        assert_eq!(s.document.style.bytes().unwrap(), before);
    }
    apply(
        &mut s,
        vec![edit(
            vec![Change::AutoStart { enabled: false }],
            Some(vec![0, 33]),
            false,
        )],
    )
    .unwrap();
    let a = selected(&s.document, "Main A", 1);
    assert_eq!(a[13] & 4, 0);
    assert_eq!(a[13] & 0xf8, 0xf8);
    assert!(mask(a, 0) && mask(a, 33));
    apply(
        &mut s,
        vec![edit(vec![Change::AutoStart { enabled: true }], None, false)],
    )
    .unwrap();
    assert_eq!(selected(&s.document, "Main A", 1)[13] & 4, 4);
    let stable = s.document.style.bytes().unwrap();
    assert!(
        apply(
            &mut s,
            vec![edit(
                vec![
                    Change::RootEnabled {
                        root: 0,
                        enabled: true
                    },
                    Change::RootEnabled {
                        root: 12,
                        enabled: true
                    }
                ],
                None,
                false
            )]
        )
        .is_err()
    );
    assert_eq!(s.document.style.bytes().unwrap(), stable);
}

#[test]
fn root_mask_and_autostart_match_608_observed_stylemagic_helper_results() {
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/stylemagic-bit-oracle.json")).unwrap();
    let roots = oracle["rows"]["root"].as_array().unwrap();
    let starts = oracle["rows"]["autoStart"].as_array().unwrap();
    assert_eq!(roots.len(), 96);
    assert_eq!(starts.len(), 512);
    let template = route(1);
    for (kind, rows) in [("root", roots), ("autoStart", starts)] {
        for row in rows {
            let a = row.as_array().unwrap();
            let n = |i: usize| a[i].as_u64().unwrap() as u16;
            let mut raw = fixture(false);
            let pos = raw.windows(47).position(|w| w == template).unwrap();
            let (change, expected) = if kind == "root" {
                raw[pos + 11..pos + 13].copy_from_slice(&n(2).to_le_bytes());
                (
                    Change::RootEnabled {
                        root: n(0) as u8,
                        enabled: n(1) != 0,
                    },
                    n(3),
                )
            } else {
                raw[pos + 13] = n(1) as u8;
                (Change::AutoStart { enabled: n(0) != 0 }, n(2))
            };
            let mut s = Session::open(&raw, "StyleMagic bit oracle".into()).unwrap();
            apply(&mut s, vec![edit(vec![change], None, false)]).unwrap();
            let result = selected(&s.document, "Main A", 1);
            let observed = if kind == "root" {
                u16::from_le_bytes([result[11], result[12]])
            } else {
                u16::from(result[13])
            };
            assert_eq!(observed, expected, "{kind}: {row}");
        }
    }
}
#[test]
fn source_chords_exclusive_preserves_reserved_bits_and_other_sections() {
    let mut s = session(false);
    apply(&mut s, vec![edit(vec![], Some(vec![0, 7, 17, 33]), true)]).unwrap();
    for c in 0..34 {
        assert_eq!(
            mask(selected(&s.document, "Main A", 1), c),
            [0, 7, 17, 33].contains(&c)
        );
    }
    assert_eq!(selected(&s.document, "Main A", 1)[13] & 0xfc, 0xfc);
    for c in [0, 7, 17, 33] {
        assert!(!mask(selected(&s.document, "Main A", 2), c));
    }
    assert_eq!(selected(&s.document, "Main B", 1), route(1));
    assert_eq!(selected(&s.document, "Main B", 2), route(2));
}
#[test]
fn source_invalid_siblings_and_late_batch_failure_are_atomic() {
    for malformed in [false, true] {
        let mut s = session(malformed);
        let before = s.document.style.bytes().unwrap();
        let actions = if malformed {
            vec![edit(vec![], Some(vec![0]), true)]
        } else {
            vec![
                edit(vec![Change::Root { value: 7 }], None, false),
                edit(vec![Change::Zone { low: 90, high: 80 }], None, false),
            ]
        };
        assert!(apply(&mut s, actions).is_err());
        assert_eq!(s.document.style.bytes().unwrap(), before);
        assert_eq!(s.revision, 0);
        assert!(!s.view().can_undo);
    }
    for change in [
        Change::Root { value: 12 },
        Change::Chord { value: 34 },
        Change::Rule {
            region: Region::Low,
            field: RuleField::Ntr,
            value: 3,
        },
        Change::Rule {
            region: Region::High,
            field: RuleField::NoteLow,
            value: 128,
        },
        Change::Rule {
            region: Region::Middle,
            field: RuleField::Retrigger,
            value: 5,
        },
        Change::Rule {
            region: Region::Middle,
            field: RuleField::Ntt,
            value: 11,
        },
    ] {
        let mut s = session(false);
        assert!(apply(&mut s, vec![edit(vec![change], None, false)]).is_err());
        assert_eq!(s.revision, 0);
    }
}
#[test]
fn source_clone_copies_only_selected_source_window_baseline_and_inactive_route() {
    let mut s = session(false);
    let before = s.document.style.bytes().unwrap();
    let original = s.document.clone();
    apply(
        &mut s,
        vec![Action::Clone {
            source: 1,
            destination: 9,
            target: 3,
        }],
    )
    .unwrap();
    let new: Vec<_> = s.document.notes.iter().filter(|n| n.channel == 3).collect();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].pitch, 60);
    assert_eq!(new[0].start, 20);
    assert_eq!(new[0].duration, 80);
    assert!(!original.notes.iter().any(|n| n.id == new[0].id));
    let mut expected = route(1);
    expected[0] = 2;
    for c in 0..34 {
        let i = 17 - (c / 8) as usize;
        expected[i] &= !(1 << (c % 8));
    }
    assert_eq!(selected(&s.document, "Main A", 3), expected);
    assert!(
        !s.document
            .routes
            .iter()
            .any(|r| r.source == 3 && r.sections.iter().any(|l| l == "Main B"))
    );
    for source in [1, 2] {
        for label in ["Main A", "Main B"] {
            assert_eq!(
                selected(&s.document, label, source),
                selected(&original, label, source)
            );
        }
    }
    let events: Vec<_> = s.document.style.tracks[0]
        .iter()
        .filter(|e| e.channel() == Some(3))
        .map(|e| (e.tick, e.data.clone()))
        .collect();
    assert_eq!(
        &events[..3],
        &[
            (10, vec![0xb2, 0, 121]),
            (10, vec![0xb2, 32, 5]),
            (10, vec![0xc2, 34])
        ]
    );
    assert!(events.contains(&(10, vec![0xb2, 7, 90])));
    assert!(events.contains(&(10, vec![0xe2, 0, 64])));
    assert_eq!(
        s.document.style.tracks[0]
            .iter()
            .filter(|e| e.channel() != Some(3))
            .collect::<Vec<_>>(),
        original.style.tracks[0].iter().collect::<Vec<_>>()
    );
    let bytes = s.document.style.bytes().unwrap();
    assert_eq!(
        Document::parse(&bytes).unwrap().notes.len(),
        original.notes.len() + 1
    );
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}
#[test]
fn source_clone_rejects_used_reserved_invalid_channels_crossings_and_ambiguous_routes() {
    for target in [0, 1, 2, 17] {
        let mut s = session(false);
        assert!(
            apply(
                &mut s,
                vec![Action::Clone {
                    source: 1,
                    destination: 9,
                    target
                }]
            )
            .is_err()
        );
        assert_eq!(s.revision, 0);
    }
    let mut s = session(false);
    let mut style = s.document.style.clone();
    let id = style.tracks[0].iter().map(|e| e.id).max().unwrap() + 1;
    style.tracks[0].push(Event {
        id,
        tick: 100,
        order: id,
        data: vec![0xb2, 10, 64],
    });
    style.tracks[0].sort_by_key(|e| (e.tick, e.order));
    style.chunks[1].data = smf::encode_track(&style.tracks[0]).unwrap();
    s = Session::open(&style.bytes().unwrap(), "Reserved controller".into()).unwrap();
    assert!(
        apply(
            &mut s,
            vec![Action::Clone {
                source: 1,
                destination: 9,
                target: 3
            }]
        )
        .is_err()
    );
    let mut s = session(false);
    let off = s
        .document
        .notes
        .iter()
        .find(|n| n.channel == 1)
        .unwrap()
        .off_id;
    let mut style = s.document.style.clone();
    style.tracks[0]
        .iter_mut()
        .find(|e| e.id == off)
        .unwrap()
        .tick = 490;
    style.chunks[1].data = smf::encode_track(&style.tracks[0]).unwrap();
    s = Session::open(&style.bytes().unwrap(), "Boundary crossing".into()).unwrap();
    assert!(
        apply(
            &mut s,
            vec![Action::Clone {
                source: 1,
                destination: 9,
                target: 3
            }]
        )
        .is_err()
    );
    let mut s = session(false);
    let mut style = s.document.style.clone();
    let mut groups = smf::chunks(&style.chunks[2].data).unwrap();
    let mut records = smf::chunks(&groups[0].data).unwrap();
    records.push(chunk(b"Ctb2", route(1)));
    groups[0].data = smf::pack(&records).unwrap();
    style.chunks[2].data = smf::pack(&groups).unwrap();
    s = Session::open(&style.bytes().unwrap(), "Duplicate".into()).unwrap();
    assert!(
        apply(
            &mut s,
            vec![edit(vec![Change::Root { value: 2 }], None, false)]
        )
        .is_err()
    );
}
#[test]
fn source_noop_does_not_split_or_create_history_and_unknown_rule_values_survive() {
    let mut s = session(false);
    let before = s.document.style.bytes().unwrap();
    apply(
        &mut s,
        vec![edit(vec![Change::Root { value: 0 }], None, false)],
    )
    .unwrap();
    assert_eq!(s.revision, 0);
    assert_eq!(s.document.style.bytes().unwrap(), before);
    let mut style = s.document.style.clone();
    let route = s.document.routes[0].clone();
    style.chunks[route.chunk].data[route.offset + 29] = 255;
    s = Session::open(&style.bytes().unwrap(), "Unknown rule".into()).unwrap();
    apply(
        &mut s,
        vec![edit(vec![Change::Root { value: 5 }], None, false)],
    )
    .unwrap();
    assert_eq!(selected(&s.document, "Main A", 1)[29], 255);
    apply(
        &mut s,
        vec![edit(
            vec![
                Change::Rule {
                    region: Region::Middle,
                    field: RuleField::Ntr,
                    value: 2,
                },
                Change::Rule {
                    region: Region::Middle,
                    field: RuleField::Ntt,
                    value: 2,
                },
            ],
            None,
            false,
        )],
    )
    .unwrap();
    assert_eq!(selected(&s.document, "Main A", 1)[29], 130);
    assert!(
        apply(
            &mut s,
            vec![edit(
                vec![Change::Rule {
                    region: Region::Middle,
                    field: RuleField::Ntt,
                    value: 3
                }],
                None,
                false
            )]
        )
        .is_err()
    );
}

#[test]
fn source_section_local_edits_preserve_every_other_route_across_supplied_corpus() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/research/artifacts/style files");
    let mut tested = 0;
    for entry in std::fs::read_dir(corpus).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("sty"))
        {
            continue;
        }
        let original = std::fs::read(&path).unwrap();
        let mut s = Session::open(&original, path.display().to_string()).unwrap();
        let before = s.document.clone();
        let route = before
            .routes
            .iter()
            .find(|r| {
                before
                    .sections
                    .iter()
                    .any(|section| r.sections.contains(&section.label))
            })
            .unwrap();
        let section = before
            .sections
            .iter()
            .find(|section| route.sections.contains(&section.label))
            .unwrap();
        let root = (route.source_root + 1) % 12;
        s.execute(
            0,
            Command::SourceTracks {
                section: section.id,
                actions: vec![Action::Edit {
                    source: route.source,
                    destination: route.destination,
                    changes: vec![Change::Root { value: root }],
                    chords: None,
                    exclusive: false,
                }],
            },
        )
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for previous in &before.routes {
            for label in &previous.sections {
                let current = s
                    .document
                    .routes
                    .iter()
                    .find(|r| {
                        r.source == previous.source
                            && r.destination == previous.destination
                            && r.sections.contains(label)
                    })
                    .unwrap();
                let mut expected = previous.bytes.clone();
                if label == &section.label && previous.id == route.id {
                    expected[18] = root;
                }
                assert_eq!(
                    current.bytes,
                    expected,
                    "{}: {label}, source {}",
                    path.display(),
                    previous.source
                );
            }
        }
        for (a, b) in before.style.chunks.iter().zip(&s.document.style.chunks) {
            if a.tag != *b"CASM" {
                assert_eq!(a, b);
            }
        }
        let exported = s.document.style.bytes().unwrap();
        assert!(Document::parse(&exported).is_ok());
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), original);
        tested += 1;
    }
    assert!(tested > 0, "corpus must not be empty");
    println!("SOURCE TRACK CORPUS VERIFIED: {tested} files");
}

#[test]
fn source_ambiguous_section_names_reject_and_atomic_limits_can_move_past_old_range() {
    let mut s = session(false);
    apply(
        &mut s,
        vec![edit(
            vec![Change::Limits {
                region: Region::Middle,
                low: 80,
                high: 90,
            }],
            None,
            false,
        )],
    )
    .unwrap();
    apply(
        &mut s,
        vec![edit(
            vec![Change::Limits {
                region: Region::Middle,
                low: 100,
                high: 110,
            }],
            None,
            false,
        )],
    )
    .unwrap();
    assert_eq!(&selected(&s.document, "Main A", 1)[31..33], &[100, 110]);
    let mut style = s.document.style.clone();
    for e in &mut style.tracks[0] {
        if e.meta(6) == Some(b"Main B") {
            *e.data.last_mut().unwrap() = b'A';
        }
    }
    style.chunks[1].data = smf::encode_track(&style.tracks[0]).unwrap();
    let mut s = Session::open(&style.bytes().unwrap(), "Ambiguous section names".into()).unwrap();
    assert!(
        apply(
            &mut s,
            vec![edit(vec![Change::Root { value: 7 }], None, false)]
        )
        .is_err()
    );
    assert_eq!(s.revision, 0);
}

#[test]
fn source_clone_real_control_preserves_original_events_and_roundtrips_project() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY");
    let original = std::fs::read(path).unwrap();
    let mut s = Session::open(&original, "Real control clone".into()).unwrap();
    let before = s.document.clone();
    let section = before
        .sections
        .iter()
        .find(|s| s.label == "Main A")
        .unwrap();
    let count = before
        .notes
        .iter()
        .filter(|n| n.channel == 11 && n.start >= section.start && n.start < section.end)
        .count();
    assert!(count > 0);
    s.execute(
        0,
        Command::SourceTracks {
            section: section.id,
            actions: vec![Action::Clone {
                source: 11,
                destination: 11,
                target: 1,
            }],
        },
    )
    .unwrap();
    assert_eq!(s.document.notes.len(), before.notes.len() + count);
    for (old, new) in before.style.tracks.iter().zip(&s.document.style.tracks) {
        for event in old {
            assert_eq!(new.iter().find(|e| e.id == event.id), Some(event));
        }
    }
    for previous in &before.routes {
        for label in &previous.sections {
            assert_eq!(
                s.document
                    .routes
                    .iter()
                    .find(|r| r.source == previous.source
                        && r.destination == previous.destination
                        && r.sections.contains(label))
                    .unwrap()
                    .bytes,
                previous.bytes
            );
        }
    }
    let exported = s.document.style.bytes().unwrap();
    assert_eq!(
        Document::parse(&exported).unwrap().notes.len(),
        before.notes.len() + count
    );
    if let Some(path) = std::env::var_os("GENOS_CLONED_STYLE_REPLAY") {
        std::fs::write(path, &exported).unwrap();
    }
    if let Some(path) = std::env::var_os("GENOS_CLONE_PITCHES_REPLAY") {
        let cases: Vec<[u8; 7]> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(cases.len(), 7200);
        let reopened = Document::parse(&exported).unwrap();
        let section = reopened.sections.iter().find(|s| s.label == "Main A").unwrap();
        let quality = |id| match id {
            0 => "Major", 2 => "Maj7", 7 => "aug", 8 => "Minor", 10 => "m7",
            11 => "m7b5", 17 => "dim", 19 => "7", 32 => "sus4", 33 => "sus2",
            _ => panic!("Unexpected chord type {id}"),
        };
        for [source, root, kind, bass, bass_kind, pitch, expected] in cases {
            let chord = genos_core::midi::LiveChord {
                root,
                quality: quality(kind).into(),
                bass: Some(bass),
                bass_quality: Some(quality(bass_kind).into()),
            };
            let actual = genos_core::live_preview::follow_pitch(&reopened, section.id, source, pitch, &chord);
            assert_eq!(actual, (expected != 255).then_some(expected),
                       "source {source}, chord {chord:?}, input pitch {pitch}");
        }
    }
    let project = genos_core::project::session_bytes(&s).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("source-clone.gss");
    std::fs::write(&file, project).unwrap();
    let reopened = genos_core::project::open_file(&file).unwrap();
    assert_eq!(reopened.document.style.bytes().unwrap(), exported);
    assert_eq!(reopened.document.notes.len(), before.notes.len() + count);
    let auto_start_export = std::env::var_os("GENOS_CLONE_AUTOSTART_EXPORT");
    let auto_start_replay = std::env::var_os("GENOS_CLONE_AUTOSTART_PITCHES");
    if auto_start_export.is_some() || auto_start_replay.is_some() {
        s.execute(
            s.revision,
            Command::SourceTracks {
                section: section.id,
                actions: vec![Action::Edit {
                    source: 1,
                    destination: 11,
                    changes: vec![Change::AutoStart { enabled: true }],
                    chords: None,
                    exclusive: false,
                }],
            },
        )
        .unwrap();
        let enabled = s.document.style.bytes().unwrap();
        if let Some(path) = auto_start_export {
            std::fs::write(path, &enabled).unwrap();
        }
        if let Some(path) = auto_start_replay {
            let disabled_doc = Document::parse(&exported).unwrap();
            let enabled_doc = Document::parse(&enabled).unwrap();
            let cases: Vec<(String, u8, u8, u8)> =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(cases.len(), 512);
            for (state, pitch, kind, expected) in cases {
                let doc = match state.as_str() {
                    "off" => &disabled_doc,
                    "on" => &enabled_doc,
                    _ => panic!("Unknown Auto Start state {state}"),
                };
                let section = doc.sections.iter().find(|s| s.label == "Main A").unwrap();
                let chord = genos_core::midi::LiveChord {
                    root: 0,
                    quality: match kind {
                        0 => "Major",
                        34 => "Genos:34",
                        _ => panic!("Unexpected chord type {kind}"),
                    }.into(),
                    bass: None,
                    bass_quality: None,
                };
                let actual =
                    genos_core::live_preview::follow_pitch(doc, section.id, 1, pitch, &chord);
                assert_eq!(actual, (expected != 255).then_some(expected),
                    "Auto Start {state}, chord type {kind}, pitch {pitch}");
            }
        }
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), exported);
    }
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), original);
}

#[test]
fn latin_casm_edit_preserves_ots_chunk_and_exact_undo() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/research/experiments/fixtures/latin01-original-copy.STY");
    let original = std::fs::read(fixture).unwrap();
    let mut session = Session::open(&original, "Latin OTS preservation".into()).unwrap();
    let before = session.document.style.clone();
    let section = session.document.sections.iter().find(|section| section.label == "Main A").unwrap();
    session.execute(0, Command::SourceTracks {
        section: section.id,
        actions: vec![Action::Edit {
            source: 11,
            destination: 11,
            changes: vec![Change::Root { value: 7 }],
            chords: None,
            exclusive: false,
        }],
    }).unwrap();

    let edited = session.document.style.bytes().unwrap();
    let after = smf::Style::parse(&edited).unwrap();
    assert_ne!(original, edited);
    assert_eq!(before.chunks.len(), after.chunks.len());
    for (old, new) in before.chunks.iter().zip(&after.chunks) {
        assert_eq!(old.tag, new.tag);
        if old.tag == *b"CASM" {
            assert_ne!(old.data, new.data);
        } else {
            assert_eq!(old.data, new.data, "chunk {:?}", old.tag);
        }
    }
    assert!(after.chunks.iter().any(|chunk| chunk.tag == *b"OTSc"));
    if let Some(path) = std::env::var_os("GENOS_OTS_EDIT_REPLAY") {
        std::fs::write(path, &edited).unwrap();
    }
    session.undo().unwrap();
    assert_eq!(session.document.style.bytes().unwrap(), original);
}

#[test]
fn source_clone_rejects_target_orphan_that_would_steal_a_copied_off() {
    let s = session(false);
    let mut style = s.document.style.clone();
    let id = style.tracks[0].iter().map(|e| e.id).max().unwrap() + 1;
    style.tracks[0].push(Event {
        id,
        tick: 0,
        order: id,
        data: vec![0x92, 60, 90],
    });
    style.chunks[1].data = smf::encode_track(&style.tracks[0]).unwrap();
    let before = style.bytes().unwrap();
    let mut s = Session::open(&before, "Target has an unclosed header note".into()).unwrap();
    assert!(
        apply(
            &mut s,
            vec![Action::Clone {
                source: 1,
                destination: 9,
                target: 3
            }]
        )
        .is_err()
    );
    assert_eq!(s.document.style.bytes().unwrap(), before);
    assert_eq!(s.revision, 0);
}

#[test]
fn source_multiple_clones_share_one_transaction_with_exact_new_pairs() {
    let mut s = session(false);
    let before = s.document.style.bytes().unwrap();
    apply(
        &mut s,
        vec![
            Action::Clone {
                source: 1,
                destination: 9,
                target: 3,
            },
            Action::Clone {
                source: 3,
                destination: 9,
                target: 4,
            },
        ],
    )
    .unwrap();
    for channel in [3, 4] {
        let notes: Vec<_> = s
            .document
            .notes
            .iter()
            .filter(|n| n.channel == channel)
            .collect();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].pitch, 60);
        assert_eq!(notes[0].duration, 80);
    }
    assert_eq!(s.revision, 1);
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}
