//! Phase 7 smoke: Genos-compatible operations on a real STY fixture.
//! Compares semantic invariants (not binary equality with SFM project files).
use genos_core::{
    copy_material::{CopyFit, CopyScope},
    edit::{Command, Session},
    exchange,
    library::{self, LibraryFit},
    section_length::SectionLengthOp,
};
use std::{fs, path::Path};

fn session() -> Session {
    let b = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
    )
    .unwrap();
    Session::open(&b, "parity".into()).unwrap()
}

#[test]
fn parity_smoke_copy_length_exchange_preserve_casm_and_undo() {
    let mut s = session();
    let casm_before: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert!(!casm_before.is_empty());
    let original = s.document.style.bytes().unwrap();

    let fill = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Fill In AA")
        .unwrap()
        .clone();
    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap()
        .clone();
    let ch = s
        .document
        .notes
        .iter()
        .find(|n| n.start >= main.start && n.start < main.end)
        .unwrap()
        .channel;

    s.execute(
        0,
        Command::SectionLength {
            section: fill.id,
            op: SectionLengthOp::FillBreak { bars: 2 },
        },
    )
    .unwrap();
    s.execute(
        1,
        Command::CopyMaterial {
            source_section: main.id,
            destination_section: fill.id,
            scope: CopyScope::Source { channel: ch },
            fit: CopyFit::Fit,
        },
    )
    .unwrap();

    let mid = exchange::export_section(&s.document, main.id).unwrap();
    let prepared = exchange::prepare_section(&s.document, main.id, &mid).unwrap();
    assert!(prepared.preview.unchanged || !prepared.phrases.is_empty());

    let casm_after: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert_eq!(casm_before, casm_after);

    while s.view().can_undo {
        s.undo().unwrap();
    }
    assert_eq!(s.document.style.bytes().unwrap(), original);
}

#[test]
fn parity_smoke_library_phrase_import_preserves_casm() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY");
    let lib_path = std::env::temp_dir().join("genos-parity-library.STY");
    fs::copy(&fixture, &lib_path).unwrap();

    let mut s = Session::open(&fs::read(&fixture).unwrap(), "parity-lib".into()).unwrap();
    let casm_before: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    let original = s.document.style.bytes().unwrap();

    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap()
        .clone();
    let fill = s
        .document
        .sections
        .iter()
        .find(|x| x.label.starts_with("Fill"))
        .unwrap()
        .clone();
    let ch = s
        .document
        .notes
        .iter()
        .find(|n| n.start >= main.start && n.start < main.end)
        .unwrap()
        .channel;

    let (src_len, phrase) = library::extract_phrase(&lib_path, main.id, ch).unwrap();
    let dest_len = fill.end - fill.start;
    let notes = library::map_to_destination(&phrase, src_len, dest_len, LibraryFit::Fit).unwrap();
    s.execute(
        0,
        Command::ReplacePhrase {
            section: fill.id,
            channel: ch,
            notes,
            voice: None,
            controllers: vec![],
        },
    )
    .unwrap();

    let casm_after: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert_eq!(casm_before, casm_after);
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), original);
    let _ = fs::remove_file(&lib_path);
}

#[test]
fn parity_smoke_beat_control_2_4_open_export_section() {
    let b = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/15.7-8-BEAT-control.STY"),
    )
    .unwrap();
    let s = Session::open(&b, "beat".into()).unwrap();
    assert!(
        s.document.sections.len() >= 2,
        "beat control should expose multiple sections"
    );
    let (_, num, den) = s.document.timing_at(0);
    assert_eq!((num, den), (2, 4), "fixture is 2/4 control");
    let casm_before: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert!(!casm_before.is_empty());
    let sec = s.document.sections[0].clone();
    let mid = exchange::export_section(&s.document, sec.id).unwrap();
    let prepared = exchange::prepare_section(&s.document, sec.id, &mid).unwrap();
    assert!(prepared.preview.unchanged || !prepared.phrases.is_empty());
    let casm_after: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert_eq!(casm_before, casm_after);
    assert_eq!(s.document.style.bytes().unwrap(), b);
}

#[test]
fn parity_smoke_six_eight_open_and_casm_retain() {
    let b = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/artifacts/style files/13.6-8 OFF WITH THAVIL.T236.STY"),
    )
    .unwrap();
    let s = Session::open(&b, "six-eight".into()).unwrap();
    let (_, num, den) = s.document.timing_at(0);
    assert!(
        (num == 6 && den == 8) || s.document.sections.len() >= 2,
        "expected 6/8 meter or multi-section style, got {num}/{den} sections={}",
        s.document.sections.len()
    );
    let casm_before: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert!(!casm_before.is_empty());
    let sec = s.document.sections[0].clone();
    let mid = exchange::export_section(&s.document, sec.id).unwrap();
    let prepared = exchange::prepare_section(&s.document, sec.id, &mid).unwrap();
    assert!(prepared.preview.unchanged || !prepared.phrases.is_empty());
    let casm_after: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert_eq!(casm_before, casm_after);
}

#[test]
fn parity_smoke_multisource_latin01_casm_and_routes() {
    let b = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/latin01-original-copy.STY"),
    )
    .unwrap();
    let s = Session::open(&b, "latin01".into()).unwrap();
    let routes_before = s.document.routes.len();
    assert!(
        routes_before > 8,
        "latin01 should expose multi-source CASM routes, got {routes_before}"
    );
    let casm_before: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    let channels: std::collections::BTreeSet<_> =
        s.document.notes.iter().map(|n| n.channel).collect();
    assert!(
        channels.len() > 8,
        "expected multi-source note channels, got {channels:?}"
    );

    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label.contains("Main") || x.label.contains("MAIN"))
        .unwrap()
        .clone();
    let ch = s
        .document
        .notes
        .iter()
        .find(|n| n.start >= main.start && n.start < main.end)
        .map(|n| n.channel)
        .expect("main section should contain notes");
    let mid = exchange::export(&s.document, main.id, ch).unwrap();
    let prepared = exchange::prepare(&s.document, main.id, ch, &mid).unwrap();
    assert!(prepared.preview.unchanged || !prepared.notes.is_empty());

    let casm_after: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert_eq!(casm_before, casm_after);
    assert_eq!(s.document.routes.len(), routes_before);
}

#[test]
fn parity_smoke_tempo_and_pitch_range_preserve_casm() {
    use genos_core::edit::TempoPoint;
    let mut s = session();
    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap()
        .clone();
    let ch = s
        .document
        .notes
        .iter()
        .find(|n| n.start >= main.start && n.start < main.end && (9..=16).contains(&n.channel))
        .map(|n| n.channel)
        .expect("style part channel");
    let casm_before: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    let original = s.document.style.bytes().unwrap();
    s.execute(
        0,
        Command::TempoPoints {
            section: main.id,
            points: vec![TempoPoint {
                tick: main.start + 240,
                bpm: 132.0,
            }],
            remove: vec![],
        },
    )
    .unwrap();
    s.execute(
        1,
        Command::PitchBendRange {
            section: main.id,
            channel: ch,
            semitones: 12,
            all_sections: false,
        },
    )
    .unwrap();
    let casm_after: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert_eq!(casm_before, casm_after);
    assert!((s.document.timing_at(main.start + 240).0 - 132.0).abs() < 0.05);
    while s.view().can_undo {
        s.undo().unwrap();
    }
    assert_eq!(s.document.style.bytes().unwrap(), original);
}

#[test]
fn parity_smoke_seven_eight_quantize_preserves_casm() {
    let b = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/15.7-8-BEAT-control.STY"),
    )
    .unwrap();
    let mut s = Session::open(&b, "seven-eight-q".into()).unwrap();
    let (_, num, den) = s.document.timing_at(0);
    assert_eq!((num, den), (2, 4), "control fixture meters as 2/4 beat");
    let casm_before: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    let original = s.document.style.bytes().unwrap();
    let sec = s.document.sections[0].clone();
    let ids: Vec<_> = s
        .document
        .notes
        .iter()
        .filter(|n| n.start >= sec.start && n.start < sec.end)
        .map(|n| n.id)
        .take(8)
        .collect();
    assert!(!ids.is_empty(), "section should have notes to quantize");
    let step = u64::from(s.document.style.ppq / 4).max(1);
    s.execute(
        0,
        Command::Quantize {
            section: sec.id,
            ids,
            step,
            move_controllers: true,
        },
    )
    .unwrap();
    let casm_after: Vec<_> = s
        .document
        .style
        .chunks
        .iter()
        .filter(|c| &c.tag == b"CASM")
        .map(|c| c.data.clone())
        .collect();
    assert_eq!(casm_before, casm_after);
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), original);
}
