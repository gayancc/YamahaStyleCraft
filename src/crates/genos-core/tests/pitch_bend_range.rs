//! SFM-PITCH-RANGE: Genos/Genos2 RPN 0,0 fingerprint (not proprietary SysEx).
use genos_core::edit::{Command, Session};
use std::{fs, path::Path};

fn session() -> Session {
    let b = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
    )
    .unwrap();
    Session::open(&b, "pbr".into()).unwrap()
}

#[test]
fn pitch_bend_range_writes_rpn_packet_and_undoes() {
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
        .unwrap_or(11);
    let before = s.document.style.bytes().unwrap();
    s.execute(
        0,
        Command::PitchBendRange {
            section: main.id,
            channel: ch,
            semitones: 12,
            all_sections: false,
        },
    )
    .unwrap();
    let status = 0xb0 | (ch - 1);
    let at: Vec<_> = s
        .document
        .style
        .tracks
        .iter()
        .flatten()
        .filter(|e| e.tick == main.start && e.data.len() == 3 && e.data[0] == status)
        .collect();
    let expected = [(101u8, 0u8), (100, 0), (6, 12), (101, 127), (100, 127)];
    for &(cc, val) in &expected {
        assert!(
            at.iter().any(|e| e.data[1] == cc && e.data[2] == val),
            "missing CC{cc}={val} in {:?}",
            at.iter()
                .map(|e| (e.data[1], e.data[2]))
                .collect::<Vec<_>>()
        );
    }
    // Ordered packet: first 101/0 before data entry before null RPN.
    let packet: Vec<_> = at
        .iter()
        .filter(|e| matches!(e.data[1], 6 | 100 | 101))
        .collect();
    assert!(packet.len() >= 5);
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
}

#[test]
fn pitch_bend_range_rejects_voice_channels_and_bad_semitones() {
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
            Command::PitchBendRange {
                section: main.id,
                channel: 1,
                semitones: 2,
                all_sections: false,
            },
        )
        .is_err()
    );
    assert!(
        s.execute(
            0,
            Command::PitchBendRange {
                section: main.id,
                channel: 11,
                semitones: 25,
                all_sections: false,
            },
        )
        .is_err()
    );
}
