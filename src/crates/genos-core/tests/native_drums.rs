use genos_core::{
    document::Document,
    edit::{Command, Session},
    native_drums::{self, Change, Field},
    smf::{self, Chunk, Event},
    voice_catalog::Address,
};
use std::path::Path;
fn chunk(tag: &[u8; 4], data: Vec<u8>) -> Chunk {
    Chunk { tag: *tag, data }
}
fn sx(payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0xf0];
    smf::write_vlq(payload.len() as u64, &mut out).unwrap();
    out.extend(payload);
    out
}
fn fixture() -> Vec<u8> {
    let mut events = Vec::new();
    let mut add = |tick, data| {
        let id = events.len() as u64 + 1;
        events.push(Event {
            id,
            tick,
            order: id,
            data,
        });
    };
    add(0, vec![0xff, 6, 4, b'S', b'F', b'F', b'2']);
    for ch in [9, 10] {
        add(1, vec![0xb0 | (ch - 1), 0, 127]);
        add(1, vec![0xb0 | (ch - 1), 32, 0]);
        add(1, vec![0xc0 | (ch - 1), 0]);
    }
    add(2, sx(&[0x43, 0x10, 0x4c, 8, 8, 7, 3, 0xf7]));
    add(3, sx(&[0x43, 0x10, 0x4c, 8, 9, 7, 2, 0xf7]));
    add(4, sx(&[0x43, 0x10, 0x4c, 0x31, 36, 2, 77, 0xf7]));
    add(5, sx(&[0x43, 0x10, 0x4c, 0x31, 36, 1, 64, 0xf7]));
    add(6, sx(&[0x43, 0x10, 0x4c, 2, 1, 0, 1, 0, 0xf7]));
    add(100, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']);
    add(110, vec![0x98, 36, 90]);
    add(140, vec![0x88, 36, 23]);
    add(200, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'B']);
    add(210, vec![0x99, 38, 80]);
    add(240, vec![0x89, 38, 12]);
    add(300, vec![0xff, 0x2f, 0]);
    let mut records = vec![chunk(b"Sdec", b"Main A,Main B".to_vec())];
    for ch in [9u8, 10] {
        let mut b = vec![0; 47];
        b[0] = ch - 1;
        b[9] = ch - 1;
        b[21] = 127;
        for o in [22, 28, 34] {
            b[o + 4] = 127;
        }
        records.push(chunk(b"Ctb2", b));
    }
    records.push(chunk(b"Xtra", vec![0xfa, 0xce]));
    smf::pack(&[
        chunk(b"MThd", vec![0, 0, 0, 1, 1, 224]),
        chunk(b"MTrk", smf::encode_track(&events).unwrap()),
        chunk(
            b"CASM",
            smf::pack(&[chunk(b"CSEG", smf::pack(&records).unwrap())]).unwrap(),
        ),
        chunk(b"OTSc", vec![1, 2, 3]),
        chunk(b"TEST", vec![9, 8, 7]),
    ])
    .unwrap()
}
fn session() -> Session {
    Session::open(&fixture(), "Native drums".into()).unwrap()
}
fn cmd(changes: Vec<Change>) -> Command {
    Command::SetDrumScalars {
        channel: 9,
        address: Address {
            msb: 127,
            lsb: 0,
            program: 0,
        },
        changes,
    }
}
fn reset_cmd(notes: Vec<u8>) -> Command {
    Command::ResetDrumScalars {
        channel: 9,
        address: Address {
            msb: 127,
            lsb: 0,
            program: 0,
        },
        notes,
    }
}
fn change(note: u8, field: Field, value: i16) -> Change {
    Change { note, field, value }
}
fn add(s: &mut Session, tick: u64, data: Vec<u8>) {
    let id = s
        .document
        .style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id)
        .max()
        .unwrap()
        + 1;
    s.document.style.tracks[0].push(Event {
        id,
        tick,
        order: id,
        data,
    });
}

#[test]
fn native_scalar_batch_matches_golden_bytes_and_preserves_notes_unknowns_and_undo() {
    let mut s = session();
    let before = s.document.clone();
    let bytes = fixture();
    let info = native_drums::inspect(&s.document, 9).unwrap();
    assert_eq!(info.setup, 1);
    assert_eq!(
        info.keys.iter().find(|k| k.note == 36).unwrap().volume,
        Some(77)
    );
    s.execute(
        0,
        cmd(vec![
            change(36, Field::Volume, 101),
            change(36, Field::Pan, 20),
            change(36, Field::Pitch, -2),
        ]),
    )
    .unwrap();
    let after = native_drums::parameters(&s.document).unwrap();
    for (p, v) in [(2, 101), (4, 20), (0, 62)] {
        assert_eq!(
            after
                .iter()
                .rev()
                .find(|r| r.setup == 1 && r.note == 36 && r.parameter == p)
                .unwrap()
                .data,
            vec![v]
        );
    }
    let mut expected = before.style.clone();
    let volume = expected.tracks[0]
        .iter_mut()
        .find(|e| native_drums::payload(e) == Some(&[0x43, 0x10, 0x4c, 0x31, 36, 2, 77, 0xf7][..]))
        .unwrap();
    volume.data[8] = 101;
    // Single-byte original volume delta, plus two independently specified header packets.
    let next = expected
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap()
        + 1;
    for (i, payload) in [
        [0x43, 0x10, 0x4c, 0x31, 36, 4, 20, 0xf7],
        [0x43, 0x10, 0x4c, 0x31, 36, 0, 62, 0xf7],
    ]
    .iter()
    .enumerate()
    {
        expected.tracks[0].push(Event {
            id: next + i as u64,
            order: next + i as u64,
            tick: 99,
            data: sx(payload),
        });
    }
    expected.chunks[expected.track_chunks[0]].data =
        smf::encode_track(&expected.tracks[0]).unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), expected.bytes().unwrap());
    assert_eq!(
        serde_json::to_value(&s.document.notes).unwrap(),
        serde_json::to_value(&before.notes).unwrap()
    );
    for c in before.style.chunks.iter().filter(|c| c.tag != *b"MTrk") {
        assert!(s.document.style.chunks.contains(c));
    }
    let edited = s.document.style.bytes().unwrap();
    Document::parse(&edited).unwrap();
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), bytes);
    s.redo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), edited);
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("native.gss");
    genos_core::project::save_project(&s, &project).unwrap();
    assert_eq!(
        genos_core::project::open_file(&project)
            .unwrap()
            .document
            .style
            .bytes()
            .unwrap(),
        edited
    );
}
#[test]
fn exact_noop_and_repeated_field_edits_do_not_accumulate_packets() {
    let mut s = session();
    s.execute(0, cmd(vec![change(36, Field::Volume, 77)]))
        .unwrap();
    assert_eq!(s.revision, 0);
    assert!(!s.view().can_undo);
    s.execute(0, cmd(vec![change(36, Field::Pan, 64)])).unwrap();
    let count = native_drums::parameters(&s.document).unwrap().len();
    s.execute(1, cmd(vec![change(36, Field::Pan, 70)])).unwrap();
    assert_eq!(native_drums::parameters(&s.document).unwrap().len(), count);
}
#[test]
fn stale_invalid_duplicate_and_late_batch_failure_leave_no_history_or_byte_change() {
    for changes in [
        vec![],
        vec![change(36, Field::Volume, 100), change(36, Field::Pitch, 64)],
        vec![change(12, Field::Volume, 80)],
        vec![change(36, Field::Pan, 0)],
        vec![change(36, Field::Volume, 90), change(36, Field::Volume, 91)],
    ] {
        let mut s = session();
        let original = s.document.style.bytes().unwrap();
        assert!(s.execute(0, cmd(changes)).is_err());
        assert_eq!(s.document.style.bytes().unwrap(), original);
        assert_eq!(s.revision, 0);
        assert!(!s.view().can_undo);
    }
    assert!(
        session()
            .execute(1, cmd(vec![change(36, Field::Volume, 80)]))
            .is_err()
    );
}
#[test]
fn shared_setup_late_kit_split_unknown_and_fine_tuning_fail_closed() {
    for (tick, data) in [
        (7, sx(&[0x43, 0x10, 0x4c, 8, 0, 7, 3, 0xf7])),
        (150, vec![0xc8, 0]),
        (150, sx(&[0x43, 0x10, 0x4c, 0x31, 36, 2, 80, 0xf7])),
        (7, vec![0xf7, 1, 0xf7]),
        (7, sx(&[0x43, 0x10, 0x4c, 0x31, 36, 0x7f, 80, 0xf7])),
    ] {
        let mut s = session();
        add(&mut s, tick, data);
        assert!(native_drums::inspect(&s.document, 9).is_err());
    }
    let mut s = session();
    add(&mut s, 7, sx(&[0x43, 0x10, 0x4c, 0x31, 36, 1, 65, 0xf7]));
    assert!(native_drums::inspect(&s.document, 9).is_ok());
    assert!(
        s.execute(0, cmd(vec![change(36, Field::Pitch, 1)]))
            .is_err()
    );
}
#[test]
fn room_limit_and_routing_guards_reject_without_mutation() {
    let mut s = session();
    add(&mut s, 99, vec![0xb8, 0, 127]);
    assert!(s.execute(0, cmd(vec![change(36, Field::Pan, 60)])).is_err());
    let mut s = session();
    s.document.routes[0].destination = 10;
    assert!(native_drums::inspect(&s.document, 9).is_err());
    let mut s = session();
    for _ in 0..511 {
        add(&mut s, 8, sx(&[0x43, 0x10, 0x4c, 0x31, 36, 2, 77, 0xf7]));
    }
    assert!(native_drums::inspect(&s.document, 9).is_err());
}
#[test]
fn all_corpus_parameters_match_independent_smf_oracle_and_every_supported_edit_undoes_exactly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(
        &std::fs::read(root.join("development/native-drums/corpus-native-drums.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rows.len(), 84);
    let mut compared = 0;
    let mut edited = 0;
    for row in rows {
        let bytes = std::fs::read(
            root.join("research/artifacts/style files")
                .join(row["file"].as_str().unwrap()),
        )
        .unwrap();
        let mut s = Session::open(&bytes, "Corpus".into()).unwrap();
        let expected: Vec<Vec<u8>> = row["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["hex"].as_str().unwrap())
            .filter(|hex| matches!(&hex[8..10], "30" | "31"))
            .map(|hex| {
                (2..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect()
            })
            .collect();
        let actual = native_drums::parameters(&s.document).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (r, p) in actual.iter().zip(expected) {
            assert_eq!(
                [0x43, 0x10, 0x4c, 0x30 + r.setup, r.note, r.parameter]
                    .into_iter()
                    .chain(r.data.clone())
                    .chain([0xf7])
                    .collect::<Vec<_>>(),
                p
            );
            compared += 1;
        }
        for channel in [9, 10] {
            if let Ok(header) = native_drums::inspect(&s.document, channel) {
                let notes = serde_json::to_value(&s.document.notes).unwrap();
                s.execute(
                    s.revision,
                    Command::SetDrumScalars {
                        channel,
                        address: header.address,
                        changes: vec![change(36, Field::Volume, 87), change(36, Field::Pan, 64)],
                    },
                )
                .unwrap();
                assert_eq!(serde_json::to_value(&s.document.notes).unwrap(), notes);
                s.undo().unwrap();
                assert_eq!(s.document.style.bytes().unwrap(), bytes);
                edited += 1;
            }
        }
    }
    assert_eq!(compared, 37);
    assert!(edited > 0, "positive corpus binding required");
    println!("NATIVE DRUM CORPUS: {compared} parameters, {edited} supported edits");
}

#[test]
fn initialization_order_reset_and_type_one_headers_fail_closed() {
    for (tick, data) in [
        (0, sx(&[0x43, 0x10, 0x4c, 0x31, 36, 2, 80, 0xf7])),
        (7, sx(&[0x43, 0x10, 0x4c, 8, 8, 7, 3, 0xf7])),
        (7, sx(&[0x43, 0x10, 0x4c, 0, 0, 0x7d, 1, 0xf7])),
        (150, sx(&[0x7e, 0x7f, 9, 1, 0xf7])),
        (7, sx(&[0x7e, 0, 9, 1, 0xf7])),
    ] {
        let mut s = session();
        add(&mut s, tick, data);
        assert!(native_drums::inspect(&s.document, 9).is_err());
    }
    let mut s = session();
    s.document.style.chunks[0].data[1] = 1;
    let doc = Document::parse(&s.document.style.bytes().unwrap()).unwrap();
    assert!(native_drums::inspect(&doc, 9).is_err());
    let mut s = session();
    for _ in 0..510 {
        add(&mut s, 8, sx(&[0x43, 0x10, 0x4c, 0x31, 36, 2, 77, 0xf7]));
    }
    assert_eq!(
        native_drums::inspect(&s.document, 9)
            .unwrap()
            .parameter_count,
        512
    );
    assert!(s.execute(0, cmd(vec![change(36, Field::Pan, 64)])).is_err());
}

#[test]
fn reset_drum_key_removes_scalar_overrides_and_undoes() {
    let mut s = session();
    let before = s.document.style.bytes().unwrap();
    assert_eq!(
        native_drums::inspect(&s.document, 9)
            .unwrap()
            .keys
            .iter()
            .find(|k| k.note == 36)
            .unwrap()
            .volume,
        Some(77)
    );
    s.execute(0, reset_cmd(vec![36])).unwrap();
    let after = native_drums::inspect(&s.document, 9).unwrap();
    assert!(
        after
            .keys
            .iter()
            .find(|k| k.note == 36)
            .unwrap()
            .volume
            .is_none()
    );
    // Fine-tuning parameter (1) is preserved; only volume/pan/pitch scalars cleared.
    assert!(
        native_drums::parameters(&s.document)
            .unwrap()
            .iter()
            .any(|r| r.setup == 1 && r.note == 36 && r.parameter == 1)
    );
    assert!(s.view().can_undo);
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}

#[test]
fn reset_all_drums_clears_every_scalar_override() {
    let mut s = session();
    s.execute(0, cmd(vec![change(36, Field::Pan, 64)])).unwrap();
    s.execute(1, reset_cmd(vec![])).unwrap();
    let params = native_drums::parameters(&s.document).unwrap();
    assert!(
        !params
            .iter()
            .any(|r| r.setup == 1 && [0u8, 2, 4].contains(&r.parameter))
    );
}
