use genos_core::chord_preview::{PitchRule, preview_pitch};

#[test]
fn yamaha_documented_minor_tables_change_only_named_degrees_in_both_directions() {
    // Genos Reference Manual H0 p29. Explicit chromatic truth tables,
    // including non-scale notes, independent of the retained Python helper.
    let cases = [
        (
            "melodic_minor",
            [0, 1, 2, 3, 3, 5, 6, 7, 8, 9, 10, 11],
            [0, 1, 2, 4, 4, 5, 6, 7, 8, 9, 10, 11],
        ),
        (
            "harmonic_minor",
            [0, 1, 2, 3, 3, 5, 6, 7, 8, 8, 10, 11],
            [0, 1, 2, 4, 4, 5, 6, 7, 9, 9, 10, 11],
        ),
        (
            "natural_minor",
            [0, 1, 2, 3, 3, 5, 6, 7, 8, 8, 10, 10],
            [0, 1, 2, 4, 4, 5, 6, 7, 9, 9, 11, 11],
        ),
        (
            "dorian",
            [0, 1, 2, 3, 3, 5, 6, 7, 8, 9, 10, 10],
            [0, 1, 2, 4, 4, 5, 6, 7, 8, 9, 11, 11],
        ),
    ];
    for (name, forward, reverse) in cases {
        for ntt in [name.to_string(), format!("{name}_5th")] {
            for (source_quality, target_quality, expected) in
                [("Major", "Minor", forward), ("Minor", "Major", reverse)]
            {
                for root in 0..12 {
                    for (pc, mapped) in expected.iter().enumerate() {
                        let rule = PitchRule {
                            source_root: root,
                            target_root: root,
                            source_quality,
                            target_quality,
                            ntt: &ntt,
                            high_key: None,
                            ..PitchRule::default()
                        };
                        let note = 48 + root + pc as u8;
                        assert_eq!(
                            preview_pitch(note, &rule).unwrap(),
                            Some(48 + root + mapped),
                            "{ntt} {source_quality}->{target_quality}, root {root}, degree {pc}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn fifth_variants_modify_the_perfect_fifth_for_augmented_and_diminished_targets() {
    for family in ["melodic_minor", "harmonic_minor", "natural_minor", "dorian"] {
        for (target_quality, fifth) in [("aug", 68), ("dim", 66), ("m7b5", 66)] {
            let ntt = format!("{family}_5th");
            let rule = PitchRule {
                ntt: &ntt,
                target_quality,
                ..PitchRule::default()
            };
            assert_eq!(preview_pitch(67, &rule).unwrap(), Some(fifth));
            let ordinary = PitchRule {
                ntt: family,
                ..rule.clone()
            };
            assert_eq!(preview_pitch(67, &ordinary).unwrap(), Some(67));
            // Seventh is not globally quantized to a chord tone.
            if family == "melodic_minor" || family == "harmonic_minor" {
                assert_eq!(preview_pitch(71, &rule).unwrap(), Some(71));
            }
        }
    }
}

#[test]
fn extended_ntt_keeps_root_transposition_high_key_limits_and_fixed_octave_handling() {
    let mut rule = PitchRule {
        ntt: "natural_minor",
        target_root: 9,
        target_quality: "Minor",
        ..PitchRule::default()
    };
    assert_eq!(preview_pitch(64, &rule).unwrap(), Some(72));
    rule.high_key = Some(7);
    assert_eq!(preview_pitch(64, &rule).unwrap(), Some(60));
    rule.note_limit_low = 72;
    rule.note_limit_high = 72;
    assert_eq!(preview_pitch(64, &rule).unwrap(), Some(72));
    rule.note_limit_high = 71;
    assert!(preview_pitch(64, &rule).is_err());
    rule.note_limit_low = 61;
    rule.note_limit_high = 71;
    assert_eq!(preview_pitch(64, &rule).unwrap(), None);
    rule.note_limit_low = 0;
    rule.note_limit_high = 127;
    rule.ntr = "root_fixed";
    assert_eq!(preview_pitch(64, &rule).unwrap(), Some(60));
}

#[test]
fn pitched_preview_matches_independent_python_contract_including_aliases_and_ties() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/development/cursor-followup/oracle-chords.json");
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(rows.len(), 5632);
    for row in rows {
        let a = row["args"].as_array().unwrap();
        let n = |i: usize| a[i].as_u64().unwrap() as u8;
        let s = |i: usize| a[i].as_str().unwrap();
        let rule = PitchRule {
            source_root: n(1),
            source_quality: s(2),
            target_root: n(3),
            target_quality: s(4),
            ntr: s(5),
            ntt: s(6),
            high_key: Some(n(7)),
            note_limit_low: n(8),
            note_limit_high: n(9),
            ..PitchRule::default()
        };
        let expected = row["expected"].as_u64().map(|n| n as u8);
        assert_eq!(
            preview_pitch(n(0), &rule).unwrap(),
            expected,
            "{}",
            row["args"]
        );
    }
}
