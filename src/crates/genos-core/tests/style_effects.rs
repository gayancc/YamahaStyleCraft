use genos_core::{
    document::Document,
    edit::{Command, Session},
    smf::{self, Chunk, Event},
    style_effects::{self, Address, Block, Change},
};
use std::path::Path;
fn chunk(tag: &[u8; 4], data: Vec<u8>) -> Chunk {
    Chunk { tag: *tag, data }
}
fn sx(p: &[u8]) -> Vec<u8> {
    let mut b = vec![0xf0];
    smf::write_vlq(p.len() as u64, &mut b).unwrap();
    b.extend(p);
    b
}
fn param(low: u8, data: &[u8]) -> Vec<u8> {
    let mut p = vec![0x43, 0x10, 0x4c, 2, 1, low];
    p.extend(data);
    p.push(0xf7);
    sx(&p)
}
fn bulk(low: u8, data: &[u8]) -> Vec<u8> {
    let mut p = vec![
        0x43,
        0,
        0x4c,
        ((data.len() >> 7) & 127) as u8,
        (data.len() & 127) as u8,
        2,
        1,
        low,
    ];
    p.extend(data);
    let sum = p[3..].iter().map(|b| *b as u32).sum::<u32>();
    p.push(((128 - sum % 128) % 128) as u8);
    p.push(0xf7);
    sx(&p)
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
    add(1, sx(&[0x43, 0x10, 0x4c, 0, 0, 0x7e, 0, 0xf7]));
    let mut rev = vec![20; 14];
    rev[0] = 1;
    rev[1] = 0;
    rev[12] = 64;
    add(2, bulk(0, &rev));
    add(3, param(32, &[65, 0]));
    add(4, param(44, &[32]));
    add(5, vec![0xb8, 91, 90]);
    add(6, vec![0xb8, 93, 70]);
    add(7, sx(&[0x43, 0x10, 0x4c, 8, 9, 7, 2, 0xf7]));
    add(100, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']);
    add(110, vec![0x98, 36, 90]);
    add(140, vec![0x88, 36, 0]);
    add(200, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'B']);
    add(210, vec![0x90, 60, 80]);
    add(240, vec![0x80, 60, 10]);
    add(300, vec![0xff, 0x2f, 0]);
    smf::pack(&[
        chunk(b"MThd", vec![0, 0, 0, 1, 1, 224]),
        chunk(b"MTrk", smf::encode_track(&events).unwrap()),
        chunk(
            b"CASM",
            smf::pack(&[chunk(b"Xtra", vec![1, 2, 3])]).unwrap(),
        ),
        chunk(b"OTSc", vec![9, 8, 7]),
        chunk(b"TEST", vec![6, 5, 4]),
    ])
    .unwrap()
}
fn session() -> Session {
    Session::open(&fixture(), "Style effects".into()).unwrap()
}
fn change(block: Block, effect: Option<(u8, u8)>, level: Option<u8>) -> Change {
    Change {
        block,
        effect: effect.map(|(msb, lsb)| Address { msb, lsb }),
        return_level: level,
    }
}
fn execute(s: &mut Session, changes: Vec<Change>) -> genos_core::Result<()> {
    s.execute(s.revision, Command::SetStyleEffects { changes })
}
fn add(s: &mut Session, tick: u64, data: Vec<u8>) {
    let next = s
        .document
        .style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap()
        + 1;
    s.document.style.tracks[0].push(Event {
        id: next,
        tick,
        order: next,
        data,
    });
}

#[test]
fn header_decoder_reads_parameter_and_checksum_verified_full_bulk_blocks() {
    let s = session();
    let h = style_effects::inspect(&s.document).unwrap();
    assert_eq!(
        (h.reverb.msb, h.reverb.lsb, h.reverb.return_level),
        (Some(1), Some(0), Some(64))
    );
    assert_eq!(
        (h.chorus.msb, h.chorus.lsb, h.chorus.return_level),
        (Some(65), Some(0), Some(32))
    );
    assert!(h.edit_problem.is_none());
    let mut s = session();
    let mut chorus = vec![3; 15];
    chorus[0] = 66;
    chorus[1] = 1;
    chorus[12] = 99;
    add(&mut s, 8, bulk(32, &chorus));
    let h = style_effects::inspect(&s.document).unwrap();
    assert_eq!(
        (h.chorus.msb, h.chorus.lsb, h.chorus.return_level),
        (Some(66), Some(1), Some(99))
    );
}
#[test]
fn typed_batch_appends_golden_packets_and_preserves_every_original_event_and_chunk() {
    let mut s = session();
    let before = s.document.clone();
    let original = fixture();
    execute(
        &mut s,
        vec![
            change(Block::Reverb, Some((1, 35)), Some(100)),
            change(Block::Chorus, None, Some(70)),
        ],
    )
    .unwrap();
    let mut expected = before.style.clone();
    let next = expected
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap()
        + 1;
    let frames = [
        vec![0xf0, 9, 0x43, 0x10, 0x4c, 2, 1, 0, 1, 35, 0xf7],
        vec![0xf0, 8, 0x43, 0x10, 0x4c, 2, 1, 12, 100, 0xf7],
        vec![0xf0, 8, 0x43, 0x10, 0x4c, 2, 1, 44, 70, 0xf7],
    ];
    for (i, data) in frames.into_iter().enumerate() {
        expected.tracks[0].push(Event {
            id: next + i as u64,
            tick: 99,
            order: next + i as u64,
            data,
        });
    }
    expected.chunks[expected.track_chunks[0]].data =
        smf::encode_track(&expected.tracks[0]).unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), expected.bytes().unwrap());
    for e in before.style.tracks.iter().flatten() {
        assert!(
            s.document
                .style
                .tracks
                .iter()
                .flatten()
                .any(|n| n.id == e.id && n.data == e.data && n.tick == e.tick)
        );
    }
    for c in before.style.chunks.iter().filter(|c| c.tag != *b"MTrk") {
        assert!(s.document.style.chunks.contains(c));
    }
    assert_eq!(
        serde_json::to_value(&s.document.notes).unwrap(),
        serde_json::to_value(&before.notes).unwrap()
    );
    let bytes = s.document.style.bytes().unwrap();
    let reopened = Document::parse(&bytes).unwrap();
    assert_eq!(
        style_effects::inspect(&reopened)
            .unwrap()
            .reverb
            .return_level,
        Some(100)
    );
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), original);
    s.redo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), bytes);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("effects.gss");
    genos_core::project::save_project(&s, &path).unwrap();
    assert_eq!(
        genos_core::project::open_file(&path)
            .unwrap()
            .document
            .style
            .bytes()
            .unwrap(),
        bytes
    );
}
#[test]
fn unchanged_effect_values_preserve_exact_bytes_and_history() {
    let mut s = session();
    execute(
        &mut s,
        vec![
            change(Block::Reverb, Some((1, 0)), Some(64)),
            change(Block::Chorus, Some((65, 0)), None),
        ],
    )
    .unwrap();
    assert_eq!(s.revision, 0);
    assert!(!s.view().can_undo);
    assert_eq!(s.document.style.bytes().unwrap(), fixture());
    execute(&mut s, vec![change(Block::Reverb, None, Some(70))]).unwrap();
    let bytes = s.document.style.bytes().unwrap();
    let revision = s.revision;
    execute(&mut s, vec![change(Block::Reverb, None, Some(70))]).unwrap();
    assert_eq!(s.revision, revision);
    assert_eq!(s.document.style.bytes().unwrap(), bytes);
}
#[test]
fn partial_unknown_values_are_not_defaults_and_complete_pair_can_override_them() {
    let mut s = session();
    add(&mut s, 8, param(1, &[5]));
    let h = style_effects::inspect(&s.document).unwrap();
    assert_eq!((h.reverb.msb, h.reverb.lsb), (None, None));
    assert_eq!(h.reverb.return_level, Some(64));
    assert!(h.edit_problem.is_none());
    execute(&mut s, vec![change(Block::Reverb, Some((1, 0)), None)]).unwrap();
    assert_eq!(s.revision, 1);
    let h = style_effects::inspect(&s.document).unwrap();
    assert_eq!((h.reverb.msb, h.reverb.lsb), (Some(1), Some(0)));
}
#[test]
fn malformed_bulk_foreign_layout_and_split_headers_are_read_only() {
    let mut invalid = bulk(0, &[1; 14]);
    let last = invalid.len() - 2;
    invalid[last] ^= 1;
    let mut foreign = param(0, &[1, 2]);
    foreign[3] = 0x11;
    for data in [
        invalid,
        foreign,
        param(0, &[1, 2, 3]),
        vec![0xf7, 1, 0xf7],
        vec![0xf0, 3, 0x43, 0x10, 0x4c],
    ] {
        let mut s = session();
        add(&mut s, 8, data);
        assert!(
            style_effects::inspect(&s.document)
                .unwrap()
                .edit_problem
                .is_some()
        );
        let before = s.document.style.bytes().unwrap();
        assert!(execute(&mut s, vec![change(Block::Reverb, Some((1, 35)), None)]).is_err());
        assert_eq!(s.document.style.bytes().unwrap(), before);
        assert!(!s.view().can_undo);
    }
}
#[test]
fn initial_resets_clear_metadata_while_later_effects_are_preserved_and_flagged() {
    let mut s = session();
    add(&mut s, 8, sx(&[0x7e, 0x7f, 9, 1, 0xf7]));
    let h = style_effects::inspect(&s.document).unwrap();
    assert_eq!((h.reverb.msb, h.chorus.return_level), (None, None));
    let mut s = session();
    let later = param(0, &[2, 0]);
    add(&mut s, 150, later.clone());
    let h = style_effects::inspect(&s.document).unwrap();
    assert!(h.reverb.late_changes);
    assert!(!h.chorus.late_changes);
    assert_eq!(h.reverb.msb, Some(1));
    execute(&mut s, vec![change(Block::Reverb, Some((1, 35)), None)]).unwrap();
    assert!(
        s.document
            .style
            .tracks
            .iter()
            .flatten()
            .any(|e| e.tick == 150 && e.data == later)
    );
}
#[test]
fn duplicate_empty_range_stale_and_unsupported_style_edits_are_atomic() {
    for changes in [
        vec![],
        vec![change(Block::Reverb, None, None)],
        vec![
            change(Block::Reverb, Some((1, 35)), None),
            change(Block::Reverb, None, Some(20)),
        ],
        vec![
            change(Block::Reverb, Some((1, 35)), None),
            change(Block::Chorus, Some((128, 0)), None),
        ],
    ] {
        let mut s = session();
        assert!(execute(&mut s, changes).is_err());
        assert_eq!(s.document.style.bytes().unwrap(), fixture());
        assert_eq!(s.revision, 0);
        assert!(!s.view().can_undo);
    }
    assert!(
        session()
            .execute(
                1,
                Command::SetStyleEffects {
                    changes: vec![change(Block::Reverb, None, Some(20))]
                }
            )
            .is_err()
    );
    let mut s = session();
    s.document.style.format = 1;
    assert!(style_effects::inspect(&s.document).is_err());
}
#[test]
fn all_corpus_metadata_matches_independent_python_and_overrides_undo_exactly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(
        &std::fs::read(root.join("development/style-effects/corpus-effects.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rows.len(), 84);
    let mut edited = 0;
    let mut messages = 0;
    for row in rows {
        let bytes = std::fs::read(
            root.join("research/artifacts/style files")
                .join(row["file"].as_str().unwrap()),
        )
        .unwrap();
        let mut s = Session::open(&bytes, "Corpus effects".into()).unwrap();
        let h = style_effects::inspect(&s.document).unwrap();
        messages += row["packets"].as_array().unwrap().len();
        assert_eq!(h.boundary, row["boundary"].as_u64().unwrap());
        for block in [Block::Reverb, Block::Chorus] {
            let values = serde_json::to_value(h.values(block)).unwrap();
            for field in ["msb", "lsb", "returnLevel"] {
                assert_eq!(
                    values[field],
                    row["state"][block.name()][field],
                    "{} {} {field}",
                    row["file"],
                    block.name()
                );
            }
        }
        if h.edit_problem.is_none() {
            let notes = serde_json::to_value(&s.document.notes).unwrap();
            execute(&mut s, vec![change(Block::Reverb, Some((1, 35)), Some(99))]).unwrap();
            assert_eq!(serde_json::to_value(&s.document.notes).unwrap(), notes);
            s.undo().unwrap();
            assert_eq!(s.document.style.bytes().unwrap(), bytes);
            edited += 1;
        }
    }
    assert_eq!(messages, 569);
    assert!(edited > 0);
    println!("STYLE EFFECTS CORPUS: {messages} messages, {edited} eligible Styles");
}
#[test]
fn preview_emits_only_explicit_typed_values_and_default_plan_stays_sysex_free() {
    let s = session();
    let setup = style_effects::preview_setup(&s.document).unwrap();
    assert_eq!(
        setup.packets,
        vec![
            vec![0xf0, 0x43, 0x10, 0x4c, 2, 1, 0, 1, 0, 0xf7],
            vec![0xf0, 0x43, 0x10, 0x4c, 2, 1, 12, 64, 0xf7],
            vec![0xf0, 0x43, 0x10, 0x4c, 2, 1, 32, 65, 0, 0xf7],
            vec![0xf0, 0x43, 0x10, 0x4c, 2, 1, 44, 32, 0xf7],
        ]
    );
    let section = s.document.sections[0].id;
    let plain = genos_core::midi::plan(&s.document, section, &[9]).unwrap();
    let disabled =
        genos_core::midi::plan_with_initial_effects(&s.document, section, &[9], false).unwrap();
    let enabled =
        genos_core::midi::plan_with_initial_effects(&s.document, section, &[9], true).unwrap();
    assert!(plain.setup.is_empty() && disabled.setup.is_empty());
    assert_eq!(enabled.setup, setup.packets);
    assert_eq!(enabled.duration, plain.duration);
    for (a, b) in enabled.packets.iter().zip(&plain.packets) {
        assert_eq!(a.at, b.at);
        assert_eq!(a.data, b.data);
    }
    assert_eq!(enabled.packets.len(), plain.packets.len());
    assert!(enabled.packets.iter().all(|p| p.data[0] != 0xf0));
    assert!(genos_core::midi::plan_with_initial_effects(&s.document, section, &[], true).is_err());
}
#[test]
fn preview_skips_unknown_fields_and_rejects_unverified_or_empty_setup() {
    let mut s = session();
    add(&mut s, 8, param(1, &[9]));
    add(&mut s, 150, param(32, &[66, 0]));
    let setup = style_effects::preview_setup(&s.document).unwrap();
    assert_eq!(setup.packets.len(), 3);
    assert_eq!(
        setup.packets[0],
        vec![0xf0, 0x43, 0x10, 0x4c, 2, 1, 12, 64, 0xf7]
    );
    assert!(setup.header.chorus.late_changes);
    add(&mut s, 9, sx(&[0x7e, 0x7f, 9, 1, 0xf7]));
    assert!(style_effects::preview_setup(&s.document).is_err());
    add(&mut s, 10, param(0, &[1, 35, 20]));
    assert!(style_effects::preview_setup(&s.document).is_err());
}
