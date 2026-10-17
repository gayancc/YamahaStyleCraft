use genos_core::chord_preview::{PitchRule, preview_pitch};

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
