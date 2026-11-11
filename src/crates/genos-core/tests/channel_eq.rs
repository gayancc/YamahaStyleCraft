//! SFM-MIXER-TAB-EQ-HIGH-LOW: static Mixer EQ via NRPN round-trip.
use genos_core::{
    channel_eq::{self, EqBand},
    edit::{Command, ControllerPoint, Session},
};
use std::{fs, path::Path};

fn session() -> Session {
    let b = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
    )
    .unwrap();
    Session::open(&b, "eq".into()).unwrap()
}

#[test]
fn timed_eq_batch_replaces_and_clears_only_the_selected_band() {
    let mut s = session();
    let section = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap();
    let (id, start) = (section.id, section.start);
    let (first, second) = (start + 20, start + 40);
    for (revision, band) in [EqBand::EqLow, EqBand::EqHigh].into_iter().enumerate() {
        s.execute(
            revision as u64,
            Command::MixerEqPoints {
                section: id,
                channel: 11,
                band,
                points: vec![
                    ControllerPoint {
                        tick: first,
                        value: 70,
                    },
                    ControllerPoint {
                        tick: second,
                        value: 71,
                    },
                ],
                clear_ticks: vec![],
            },
        )
        .unwrap();
    }
    let before = s.document.style.bytes().unwrap();
    s.execute(
        2,
        Command::MixerEqPoints {
            section: id,
            channel: 11,
            band: EqBand::EqLow,
            points: vec![ControllerPoint {
                tick: first,
                value: 80,
            }],
            clear_ticks: vec![second],
        },
    )
    .unwrap();
    let events: Vec<_> = s.document.style.tracks.iter().flatten().cloned().collect();
    assert_eq!(
        channel_eq::value_at(&events, 11, first, EqBand::EqLow),
        Some(80)
    );
    assert_eq!(
        channel_eq::value_at(&events, 11, second, EqBand::EqLow),
        None
    );
    assert_eq!(
        channel_eq::value_at(&events, 11, first, EqBand::EqHigh),
        Some(70)
    );
    assert_eq!(
        channel_eq::value_at(&events, 11, second, EqBand::EqHigh),
        Some(71)
    );
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}

#[test]
fn mixer_eq_writes_nrpn_reads_back_and_undoes() {
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
        .find(|n| n.start >= main.start && n.start < main.end)
        .map(|n| n.channel)
        .unwrap_or(11);
    let before = s.document.style.bytes().unwrap();

    s.execute(
        0,
        Command::MixerEq {
            section: main.id,
            channel: ch,
            band: EqBand::EqLow,
            value: 80,
            all_sections: false,
        },
    )
    .unwrap();
    s.execute(
        1,
        Command::MixerEq {
            section: main.id,
            channel: ch,
            band: EqBand::EqHigh,
            value: 40,
            all_sections: false,
        },
    )
    .unwrap();

    let events: Vec<_> = s.document.style.tracks.iter().flatten().cloned().collect();
    assert_eq!(
        channel_eq::value_at(&events, ch, main.start, EqBand::EqLow),
        Some(80)
    );
    assert_eq!(
        channel_eq::value_at(&events, ch, main.start, EqBand::EqHigh),
        Some(40)
    );

    s.undo().unwrap();
    let events: Vec<_> = s.document.style.tracks.iter().flatten().cloned().collect();
    assert_eq!(
        channel_eq::value_at(&events, ch, main.start, EqBand::EqHigh),
        None
    );
    assert_eq!(
        channel_eq::value_at(&events, ch, main.start, EqBand::EqLow),
        Some(80)
    );

    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}

#[test]
fn mixer_cc_batch_safe_fx_one_undo() {
    let mut s = session();
    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap()
        .clone();
    let ch = 11u8;
    let before = s.document.style.bytes().unwrap();
    s.execute(
        0,
        Command::MixerCcBatch {
            section: main.id,
            channel: ch,
            changes: vec![(91, 18), (93, 0), (74, 68), (71, 60)],
            all_sections: false,
        },
    )
    .unwrap();
    let status = 0xb0 | (ch - 1);
    let events: Vec<_> = s.document.style.tracks.iter().flatten().cloned().collect();
    for (cc, val) in [(91u8, 18u8), (93, 0), (74, 68), (71, 60)] {
        assert!(
            events
                .iter()
                .any(|e| { e.tick == main.start && e.data == [status, cc, val] }),
            "missing CC{cc}={val}"
        );
    }
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}

#[test]
fn mixer_eq_points_writes_timed_nrpn_and_clears() {
    use genos_core::edit::ControllerPoint;
    let mut s = session();
    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap()
        .clone();
    let ch = 11u8;
    let before = s.document.style.bytes().unwrap();
    let t0 = main.start;
    let t1 = main.start + 480;
    s.execute(
        0,
        Command::MixerEqPoints {
            section: main.id,
            channel: ch,
            band: EqBand::EqLow,
            points: vec![
                ControllerPoint {
                    tick: t0,
                    value: 80,
                },
                ControllerPoint {
                    tick: t1,
                    value: 40,
                },
            ],
            clear_ticks: vec![],
        },
    )
    .unwrap();
    let events: Vec<_> = s.document.style.tracks.iter().flatten().cloned().collect();
    assert_eq!(
        channel_eq::value_at(&events, ch, t0, EqBand::EqLow),
        Some(80)
    );
    assert_eq!(
        channel_eq::value_at(&events, ch, t1, EqBand::EqLow),
        Some(40)
    );
    s.execute(
        1,
        Command::MixerEqPoints {
            section: main.id,
            channel: ch,
            band: EqBand::EqLow,
            points: vec![],
            clear_ticks: vec![t1],
        },
    )
    .unwrap();
    let events: Vec<_> = s.document.style.tracks.iter().flatten().cloned().collect();
    assert_eq!(
        channel_eq::value_at(&events, ch, t0, EqBand::EqLow),
        Some(80)
    );
    assert_eq!(channel_eq::value_at(&events, ch, t1, EqBand::EqLow), None);
    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}

#[test]
fn pitch_bend_points_roundtrip_and_undo() {
    use genos_core::edit::PitchBendPoint;
    let mut s = session();
    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap()
        .clone();
    let ch = 11u8;
    let before = s.document.style.bytes().unwrap();
    let t0 = main.start;
    let t1 = main.start + 480;
    s.execute(
        0,
        Command::PitchBendPoints {
            section: main.id,
            channel: ch,
            points: vec![
                PitchBendPoint { tick: t0, value: 0 },
                PitchBendPoint {
                    tick: t1,
                    value: 16383,
                },
            ],
            remove: vec![],
        },
    )
    .unwrap();
    let status = 0xe0 | (ch - 1);
    let events: Vec<_> = s.document.style.tracks.iter().flatten().cloned().collect();
    assert!(
        events
            .iter()
            .any(|e| e.tick == t0 && e.data == [status, 0, 0])
    );
    assert!(
        events
            .iter()
            .any(|e| e.tick == t1 && e.data == [status, 127, 127])
    );
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}

#[test]
fn mixer_eq_all_sections_one_undo() {
    let mut s = session();
    let main = s
        .document
        .sections
        .iter()
        .find(|x| x.label == "Main A")
        .unwrap()
        .clone();
    let ch = 11u8;
    s.execute(
        0,
        Command::MixerEq {
            section: main.id,
            channel: ch,
            band: EqBand::EqLow,
            value: 70,
            all_sections: true,
        },
    )
    .unwrap();
    let events: Vec<_> = s.document.style.tracks.iter().flatten().cloned().collect();
    let hits = s
        .document
        .sections
        .iter()
        .filter(|sec| channel_eq::value_at(&events, ch, sec.start, EqBand::EqLow) == Some(70))
        .count();
    assert!(
        hits >= 2,
        "expected EQ on multiple section starts, got {hits}"
    );
    s.undo().unwrap();
}
