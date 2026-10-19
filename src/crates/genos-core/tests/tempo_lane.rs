//! SFM-TEMPO-LANE: section-safe SMF tempo meta (FF 51).
//! Capture tick math stays on the fixed apply-time BPM (see capture::tests); mid-section
//! FF 51 must not be required for recording quantization.
use genos_core::edit::{Command, Session, TempoPoint};
use std::{fs, path::Path};

fn session() -> Session {
    let b = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
    )
    .unwrap();
    Session::open(&b, "tempo".into()).unwrap()
}

fn tempo_bpm_at(s: &Session, tick: u64) -> f64 {
    s.document.timing_at(tick).0
}

#[test]
fn tempo_points_write_undo_and_survive_document_tempo() {
    let mut s = session();
    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap()
        .clone();
    let before = s.document.style.bytes().unwrap();
    let t = main.start + 480;
    let baseline = tempo_bpm_at(&s, 0);

    s.execute(
        0,
        Command::TempoPoints {
            section: main.id,
            points: vec![TempoPoint { tick: t, bpm: 140.0 }],
            remove: vec![],
        },
    )
    .unwrap();
    assert!((tempo_bpm_at(&s, t) - 140.0).abs() < 0.05);
    assert!((tempo_bpm_at(&s, 0) - baseline).abs() < 0.05);

    // Document-level SetTempoMeter must not erase mid-section automation.
    s.execute(
        1,
        Command::SetTempoMeter {
            tempo_bpm: 100.0,
            numerator: 3,
            denominator: 4,
        },
    )
    .unwrap();
    assert!((tempo_bpm_at(&s, 0) - 100.0).abs() < 0.05);
    assert!((tempo_bpm_at(&s, t) - 140.0).abs() < 0.05);

    let id = s
        .document
        .style
        .tracks
        .iter()
        .flatten()
        .find(|e| e.tick == t && e.meta(0x51).is_some())
        .map(|e| e.id)
        .expect("tempo meta at section tick");
    s.execute(
        2,
        Command::TempoPoints {
            section: main.id,
            points: vec![],
            remove: vec![id],
        },
    )
    .unwrap();
    assert!((tempo_bpm_at(&s, t) - 100.0).abs() < 0.05);

    while s.view().can_undo {
        s.undo().unwrap();
    }
    assert_eq!(s.document.style.bytes().unwrap(), before);
}

#[test]
fn tempo_points_reject_outside_section_and_bad_bpm() {
    let mut s = session();
    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap()
        .clone();
    assert!(
        s.execute(
            0,
            Command::TempoPoints {
                section: main.id,
                points: vec![TempoPoint {
                    tick: main.end,
                    bpm: 120.0
                }],
                remove: vec![],
            },
        )
        .is_err()
    );
    assert!(
        s.execute(
            0,
            Command::TempoPoints {
                section: main.id,
                points: vec![TempoPoint {
                    tick: main.start,
                    bpm: 19.0
                }],
                remove: vec![],
            },
        )
        .is_err()
    );
}
