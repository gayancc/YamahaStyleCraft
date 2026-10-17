use genos_core::{blank, document::Document, project, smf};
use std::{fs, path::Path};

fn fixture() -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
    )
    .unwrap()
}

#[test]
fn blank_from_reference_clears_notes_keeps_casm_and_applies_composer_tempo() {
    let original = Document::parse(&fixture()).unwrap();
    assert!(!original.notes.is_empty());
    let casm = original
        .style
        .chunks
        .iter()
        .find(|c| c.tag == *b"CASM")
        .unwrap()
        .clone();
    let session = blank::create_blank_from_reference(
        &fixture(),
        blank::BlankOptions {
            name: "  My Yamaha Style  ".into(),
            tempo_bpm: 120.0,
            numerator: 4,
            denominator: 4,
        },
    )
    .unwrap();
    let view = session.view();
    assert_eq!(view.name, "My Yamaha Style");
    assert!(view.dirty);
    assert!(view.notes.is_empty());
    assert!(
        view.sections
            .iter()
            .any(|s| s.label.to_ascii_uppercase().starts_with("MAIN "))
    );
    assert!((view.tempo - 120.0).abs() < 0.05);
    assert_eq!((view.numerator, view.denominator), (4, 4));
    let blank_casm = session
        .document
        .style
        .chunks
        .iter()
        .find(|c| c.tag == *b"CASM")
        .unwrap();
    assert_eq!(blank_casm, &casm);
    assert_eq!(
        session.project_extensions["authoring"]["mode"],
        "blank_from_reference"
    );
    // Controllers/programs from the reference remain; only note events were cleared.
    assert!(
        session
            .document
            .style
            .tracks
            .iter()
            .flatten()
            .any(|e| e.data.first().is_some_and(|s| matches!(s >> 4, 0xb | 0xc)))
    );
}

#[test]
fn blank_rejects_missing_main_and_invalid_options() {
    let chunks = [
        smf::Chunk {
            tag: *b"MThd",
            data: vec![0, 0, 0, 1, 1, 224],
        },
        smf::Chunk {
            tag: *b"MTrk",
            data: smf::encode_track(&[smf::Event {
                id: 1,
                tick: 0,
                order: 1,
                data: vec![0xff, 6, 7, b'I', b'N', b'T', b'R', b'O', b' ', b'I'],
            }])
            .unwrap(),
        },
    ];
    let bytes = smf::pack(&chunks).unwrap();
    let err = blank::create_blank_from_reference(
        &bytes,
        blank::BlankOptions {
            name: "Test".into(),
            tempo_bpm: 120.0,
            numerator: 4,
            denominator: 4,
        },
    )
    .err()
    .expect("missing MAIN must fail")
    .to_string();
    assert!(err.contains("MAIN"));
    let err = blank::create_blank_from_reference(
        &fixture(),
        blank::BlankOptions {
            name: String::new(),
            tempo_bpm: 120.0,
            numerator: 4,
            denominator: 4,
        },
    )
    .err()
    .expect("empty name must fail")
    .to_string();
    assert!(err.contains("name"));
}

#[test]
fn blank_project_save_reopen_preserves_zero_notes_and_extensions() {
    let session = blank::create_blank_from_reference(
        &fixture(),
        blank::BlankOptions {
            name: "Blank Acoustic".into(),
            tempo_bpm: 100.0,
            numerator: 3,
            denominator: 4,
        },
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blank.gss");
    project::save_project(&session, &path).unwrap();
    let reopened = project::open_file(&path).unwrap();
    assert_eq!(reopened.name, "Blank Acoustic");
    assert!(reopened.document.notes.is_empty());
    assert_eq!(
        reopened.project_extensions["authoring"]["mode"],
        "blank_from_reference"
    );
    assert!((reopened.view().tempo - 100.0).abs() < 0.05);
    assert_eq!(
        (reopened.view().numerator, reopened.view().denominator),
        (3, 4)
    );
}

#[test]
fn composer_rejects_sff1_and_midi_only_references_even_with_main_markers() {
    for marker in [b"SFF1", b"MIDI"] {
        let mut style = smf::Style::parse(&fixture()).unwrap();
        for (i, track) in style.tracks.iter_mut().enumerate() {
            for event in track.iter_mut().filter(|e| e.meta(6) == Some(b"SFF2")) {
                event.data = vec![0xff, 6, 4, marker[0], marker[1], marker[2], marker[3]];
            }
            style.chunks[style.track_chunks[i]].data = smf::encode_track(track).unwrap();
        }
        assert!(
            blank::create_blank_from_reference(
                &style.bytes().unwrap(),
                blank::BlankOptions {
                    name: "Unsupported reference".into(),
                    tempo_bpm: 120.0,
                    numerator: 4,
                    denominator: 4
                }
            )
            .is_err()
        );
    }
}

#[test]
fn header_authoring_preserves_later_changes_and_updates_other_tracks() {
    let events = vec![
        smf::Event {
            id: 1,
            tick: 100,
            order: 1,
            data: vec![0xff, 0x51, 3, 7, 161, 32],
        },
        smf::Event {
            id: 2,
            tick: 300,
            order: 2,
            data: vec![0xff, 0x2f, 0],
        },
    ];
    let other = vec![
        smf::Event {
            id: 3,
            tick: 0,
            order: 3,
            data: vec![0xff, 0x51, 3, 7, 161, 32],
        },
        smf::Event {
            id: 4,
            tick: 300,
            order: 4,
            data: vec![0xff, 0x2f, 0],
        },
    ];
    let chunks = [
        smf::Chunk {
            tag: *b"MThd",
            data: vec![0, 1, 0, 2, 1, 224],
        },
        smf::Chunk {
            tag: *b"MTrk",
            data: smf::encode_track(&events).unwrap(),
        },
        smf::Chunk {
            tag: *b"MTrk",
            data: smf::encode_track(&other).unwrap(),
        },
    ];
    let mut session =
        genos_core::edit::Session::open(&smf::pack(&chunks).unwrap(), "Timing map".into()).unwrap();
    session
        .execute(
            0,
            genos_core::edit::Command::SetTempoMeter {
                tempo_bpm: 100.0,
                numerator: 3,
                denominator: 4,
            },
        )
        .unwrap();
    let bytes = session.document.style.bytes().unwrap();
    let doc = Document::parse(&bytes).unwrap();
    assert!((doc.timing_at(0).0 - 100.0).abs() < 0.01);
    assert!((doc.timing_at(100).0 - 120.0).abs() < 0.01);
    assert_eq!(
        doc.style.tracks[0]
            .iter()
            .find(|e| e.tick == 100 && e.meta(0x51).is_some())
            .unwrap()
            .meta(0x51),
        Some(&[7, 161, 32][..])
    );
    assert!(
        doc.style
            .tracks
            .iter()
            .flatten()
            .filter(|e| e.tick == 0)
            .filter_map(|e| e.meta(0x51))
            .all(|b| b == [9, 39, 192])
    );
    assert_eq!((doc.timing_at(0).1, doc.timing_at(0).2), (3, 4));
}

#[test]
fn composer_name_limit_counts_unicode_characters_not_utf8_bytes() {
    let options = |count| blank::BlankOptions {
        name: "🎹".repeat(count),
        tempo_bpm: 120.0,
        numerator: 4,
        denominator: 4,
    };
    assert_eq!(
        blank::create_blank_from_reference(&fixture(), options(120))
            .unwrap()
            .name
            .chars()
            .count(),
        120
    );
    assert!(blank::create_blank_from_reference(&fixture(), options(121)).is_err());
}
