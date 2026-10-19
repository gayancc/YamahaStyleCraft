use genos_core::{
    advanced_quantize::*,
    document::{Document, Section},
    smf::{Event, Style},
};

fn fixture(notes: &[(u64, u64, u8)], length: u64, ppq: u16) -> Document {
    let mut events = Vec::new();
    for (i, (start, duration, pitch)) in notes.iter().enumerate() {
        events.push(Event {
            id: (i * 2 + 1) as u64,
            tick: *start,
            order: (i * 2 + 1) as u64,
            data: vec![0x98, *pitch, 90],
        });
        events.push(Event {
            id: (i * 2 + 2) as u64,
            tick: start + duration,
            order: (i * 2 + 2) as u64,
            data: vec![0x88, *pitch, 0],
        });
    }
    events.push(Event {
        id: 9999,
        tick: length,
        order: 9999,
        data: vec![0xff, 0x2f, 0],
    });
    events.sort_by_key(|e| (e.tick, e.order));
    let mut d = Document::from_style(Style {
        chunks: vec![],
        tracks: vec![events],
        track_chunks: vec![],
        ppq,
        format: 0,
    })
    .unwrap();
    d.sections = vec![Section {
        id: 0,
        label: "Main A".into(),
        start: 0,
        end: length,
    }];
    d
}
fn run(d: &Document, o: QuantizeOptions) -> Plan {
    plan(d, 0, 9, &[], &o, None).unwrap()
}
fn timing(p: &Plan, id: u64) -> (u64, u64) {
    let n = p.notes.iter().find(|n| n.id == id).unwrap();
    (n.start, n.duration)
}
fn apply(d: &mut Document, p: &Plan) {
    for u in &p.notes {
        let n = d.notes.iter_mut().find(|n| n.id == u.id).unwrap();
        n.start = u.start;
        n.duration = u.duration;
        for e in d.style.tracks.iter_mut().flatten() {
            if e.id == n.id {
                e.tick = n.start;
            }
            if e.id == n.off_id {
                e.tick = n.start + n.duration;
            }
        }
    }
    for u in &p.events {
        d.style
            .tracks
            .iter_mut()
            .flatten()
            .find(|e| e.id == u.id)
            .unwrap()
            .tick = u.tick;
    }
}
#[test]
fn endpoint_vectors_preserve_pitch_velocity_and_do_not_mutate_document() {
    let d = fixture(&[(50, 100, 60)], 480, 480);
    let before = d.style.tracks.clone();
    for (target, strength, expected) in [
        (Target::Starts, 100., (0, 100)),
        (Target::Ends, 100., (50, 70)),
        (Target::Lengths, 100., (50, 120)),
        (Target::Both, 100., (0, 120)),
        (Target::Starts, 50., (25, 100)),
        (Target::Ends, 50., (50, 85)),
        (Target::Both, 50., (25, 110)),
    ] {
        let p = run(
            &d,
            QuantizeOptions {
                target,
                strength,
                ..Default::default()
            },
        );
        assert_eq!(timing(&p, 1), expected);
        assert_eq!((p.notes[0].pitch, p.notes[0].velocity), (60, 90));
    }
    assert_eq!(d.style.tracks, before);
}
#[test]
fn ordered_assignment_prevents_grid_collapse_and_preserves_duplicate_onsets() {
    let d = fixture(&[(49, 20, 60), (51, 20, 60)], 480, 480);
    let p = run(&d, Default::default());
    assert_eq!(timing(&p, 1), (0, 20));
    assert_eq!(timing(&p, 3), (120, 20));
    let d = fixture(&[(50, 20, 60), (50, 20, 60)], 480, 480);
    let p = run(&d, Default::default());
    assert_eq!(timing(&p, 1), (0, 20));
    assert_eq!(timing(&p, 3), (0, 20));
}
#[test]
fn dense_coarse_grid_uses_observed_one_tick_fallback_and_bounds_work() {
    let d = fixture(&[(0, 70, 51), (120, 72, 51), (240, 62, 51)], 480, 480);
    let p = run(
        &d,
        QuantizeOptions {
            size: Grid::Eighth,
            ..Default::default()
        },
    );
    assert_eq!(p.notes.len(), 1);
    assert_eq!(timing(&p, 3), (1, 72));
    let notes: Vec<_> = (0..1000).map(|i| (i * 2, 1, 60)).collect();
    let d = fixture(&notes, 600_000, 480);
    assert!(
        plan(&d, 0, 9, &[], &Default::default(), None)
            .unwrap_err()
            .to_string()
            .contains("assignment work limit")
    );
}
#[test]
fn group_offset_keeps_groove_and_clamps_whole_group() {
    let d = fixture(&[(50, 40, 60), (190, 40, 62)], 480, 480);
    let p = run(
        &d,
        QuantizeOptions {
            mode: Mode::Relative,
            ..Default::default()
        },
    );
    assert_eq!(timing(&p, 1), (0, 40));
    assert_eq!(timing(&p, 3), (140, 40));
    let p = run(&d, Default::default());
    assert_eq!(timing(&p, 3), (240, 40));
}
#[test]
fn duration_bounds_and_partial_end_fallback() {
    let d = fixture(&[(430, 50, 60)], 480, 480);
    assert_eq!(timing(&run(&d, Default::default()), 1), (360, 50));
    let d = fixture(&[(450, 7, 60)], 470, 480);
    assert_eq!(
        timing(
            &run(
                &d,
                QuantizeOptions {
                    target: Target::Ends,
                    ..Default::default()
                }
            ),
            1
        ),
        (450, 20)
    );
}
#[test]
fn safe_catch_boundaries_and_noop_do_not_create_originals() {
    for (start, expected) in [(30, true), (31, false)] {
        let d = fixture(&[(start, 20, 60)], 480, 480);
        let p = run(
            &d,
            QuantizeOptions {
                catch_enabled: true,
                catch_percent: 50.,
                ..Default::default()
            },
        );
        assert_eq!(!p.is_noop(), expected);
    }
    for (start, expected) in [(10, false), (11, true)] {
        let d = fixture(&[(start, 20, 60)], 480, 480);
        let p = run(
            &d,
            QuantizeOptions {
                safe_range: 10,
                ..Default::default()
            },
        );
        assert_eq!(!p.is_noop(), expected);
    }
    let p = run(&fixture(&[(0, 40, 60)], 480, 480), Default::default());
    assert!(p.is_noop());
    assert!(p.baseline.notes.is_empty());
}
#[test]
fn swing_tuplets_mixed_ties_and_odd_ppq() {
    let d = fixture(&[(150, 20, 60)], 480, 480);
    assert_eq!(
        timing(
            &run(
                &d,
                QuantizeOptions {
                    swing: 100.,
                    ..Default::default()
                }
            ),
            1
        )
        .0,
        160
    );
    let d = fixture(&[(90, 20, 60)], 480, 480);
    assert_eq!(
        timing(
            &run(
                &d,
                QuantizeOptions {
                    tuplet: 5,
                    ..Default::default()
                }
            ),
            1
        )
        .0,
        96
    );
    let d = fixture(&[(200, 20, 60)], 480, 480);
    assert_eq!(
        timing(
            &run(
                &d,
                QuantizeOptions {
                    size: Grid::MixedEighth,
                    mode: Mode::Relative,
                    ..Default::default()
                }
            ),
            1
        )
        .0,
        240
    );
    assert_eq!(
        timing(
            &run(
                &d,
                QuantizeOptions {
                    size: Grid::MixedEighth,
                    ..Default::default()
                }
            ),
            1
        )
        .0,
        160
    );
    let d = fixture(&[(19, 5, 60)], 101, 101);
    assert_eq!(timing(&run(&d, Default::default()), 1).0, 25);
}
#[test]
fn manual_compounds_auto_reuses_earliest_baseline_reset_restores_it() {
    let mut d = fixture(&[(50, 40, 60)], 480, 480);
    let soft = QuantizeOptions {
        strength: 50.,
        ..Default::default()
    };
    let first = run(&d, soft.clone());
    apply(&mut d, &first);
    let second = plan(&d, 0, 9, &[], &soft, Some(&first.baseline)).unwrap();
    assert_eq!(timing(&second, 1).0, 13);
    apply(&mut d, &second);
    let auto = plan(
        &d,
        0,
        9,
        &[],
        &QuantizeOptions {
            from_original: true,
            ..soft
        },
        Some(&second.baseline),
    )
    .unwrap();
    assert_eq!(timing(&auto, 1).0, 25);
    let reset = reset(&d, 0, 9, &[], &second.baseline, false).unwrap();
    assert_eq!(timing(&reset, 1), (50, 40));
    assert!(reset.baseline.notes.is_empty());
    let encoded = serde_json::to_string(&second.baseline).unwrap();
    let decoded: Baseline = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, second.baseline);
}
#[test]
fn controllers_follow_latest_selected_onset_even_in_gaps_and_reset() {
    let mut d = fixture(&[(50, 100, 60), (190, 40, 62)], 480, 480);
    for (id, tick, data) in [
        (100, 0, vec![0xb8, 7, 90]),
        (101, 100, vec![0xb8, 7, 80]),
        (102, 150, vec![0xe8, 0, 64]),
        (103, 230, vec![0xb8, 11, 80]),
        (104, 100, vec![0xc8, 3]),
        (105, 100, vec![0xb8, 0, 5]),
        (106, 100, vec![0xb8, 32, 5]),
        (107, 100, vec![0xff, 0x51, 3, 7, 0xa1, 0x20]),
        (108, 100, vec![0xb8, 123, 0]),
        (109, 100, vec![0xf0, 2, 0x43, 0xf7]),
    ] {
        d.style.tracks[0].push(Event {
            id,
            tick,
            order: id,
            data,
        });
    }
    let p = run(&d, Default::default());
    assert_eq!(
        p.events,
        vec![
            EventMove { id: 101, tick: 50 },
            EventMove { id: 102, tick: 100 }
        ]
    );
    let filtered = run(
        &d,
        QuantizeOptions {
            source_filtered: true,
            ..Default::default()
        },
    );
    assert!(filtered.events.is_empty());
    apply(&mut d, &p);
    let r = reset(&d, 0, 9, &[], &p.baseline, false).unwrap();
    assert_eq!(
        r.events,
        vec![
            EventMove { id: 101, tick: 100 },
            EventMove { id: 102, tick: 150 }
        ]
    );
    let a = plan(
        &d,
        0,
        9,
        &[],
        &QuantizeOptions {
            strength: 50.,
            from_original: true,
            ..Default::default()
        },
        Some(&p.baseline),
    )
    .unwrap();
    assert_eq!(
        a.events,
        vec![
            EventMove { id: 101, tick: 75 },
            EventMove { id: 102, tick: 125 }
        ]
    );
}
#[test]
fn selection_scope_and_baseline_scope_are_independent() {
    let d = fixture(&[(50, 20, 60), (190, 20, 62)], 480, 480);
    let p = plan(&d, 0, 9, &[1], &Default::default(), None).unwrap();
    assert_eq!(p.notes.len(), 1);
    assert_eq!(p.baseline.notes.len(), 1);
    assert!(plan(&d, 0, 9, &[999], &Default::default(), None).is_err());
    assert!(plan(&d, 0, 10, &[1], &Default::default(), None).is_err());
    assert!(plan(&d, 0, 9, &[1, 1], &Default::default(), None).is_err());
}
#[test]
fn rejects_nonfinite_invalid_options_and_bounded_domain_limits() {
    let d = fixture(&[(50, 20, 60)], 480, 480);
    for o in [
        QuantizeOptions {
            strength: f64::NAN,
            ..Default::default()
        },
        QuantizeOptions {
            swing: f64::INFINITY,
            ..Default::default()
        },
        QuantizeOptions {
            catch_percent: 0.,
            ..Default::default()
        },
        QuantizeOptions {
            safe_range: 961,
            ..Default::default()
        },
        QuantizeOptions {
            rough: 121,
            ..Default::default()
        },
        QuantizeOptions {
            tuplet: 6,
            ..Default::default()
        },
        QuantizeOptions {
            mode: Mode::Relative,
            target: Target::Ends,
            ..Default::default()
        },
    ] {
        assert!(plan(&d, 0, 9, &[], &o, None).is_err());
    }
    let d = fixture(&[(1, 1, 60)], 1_000_000_000, 24);
    assert!(
        plan(&d, 0, 9, &[], &Default::default(), None)
            .unwrap_err()
            .to_string()
            .contains("grid-point limit")
    );
    let d = fixture(&[(1, 1, 60), (2, 1, 60), (3, 1, 60)], 100, 480);
    let p = plan(&d, 0, 9, &[], &Default::default(), None).unwrap();
    assert_eq!(timing(&p, 1).0, 0);
    assert_eq!(timing(&p, 3).0, 1);
    assert_eq!(timing(&p, 5).0, 2);
}
#[test]
fn rejects_note_off_identity_swaps_instead_of_changing_fifo_pairing() {
    let d = fixture(&[(50, 150, 60), (210, 10, 60)], 480, 480);
    // Selecting only the first onset can move its end beyond its untouched neighbour's off.
    let result = plan(
        &d,
        0,
        9,
        &[1],
        &QuantizeOptions {
            size: Grid::Eighth,
            mode: Mode::Relative,
            ..Default::default()
        },
        None,
    );
    assert!(result.is_ok());
    let d = fixture(&[(140, 150, 60), (300, 10, 60)], 480, 480);
    assert!(
        plan(
            &d,
            0,
            9,
            &[1],
            &QuantizeOptions {
                size: Grid::Eighth,
                mode: Mode::Relative,
                ..Default::default()
            },
            None
        )
        .is_err()
    );
}
#[test]
fn deterministic_rough_is_bounded_and_strength_zero_does_not_disable_it() {
    let d = fixture(&[(50, 20, 60)], 480, 480);
    let options = QuantizeOptions {
        strength: 0.,
        rough: 5,
        ..Default::default()
    };
    let a = run(&d, options.clone());
    let b = run(&d, options);
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(&b).unwrap()
    );
    let t = a.notes.first().map_or(50, |n| n.start);
    assert!((45..=55).contains(&t));
    assert_ne!(t, 50);
}
#[test]
fn auto_targets_restore_only_the_owned_endpoint_and_keep_current_pitch_velocity() {
    let mut d = fixture(&[(50, 100, 60)], 480, 480);
    let baseline = Baseline {
        notes: vec![NoteOriginal {
            id: 1,
            start: 50,
            duration: 100,
        }],
        events: vec![],
    };
    d.notes[0].start = 80;
    d.notes[0].duration = 120;
    d.notes[0].pitch = 62;
    d.notes[0].velocity = 75;
    // This fixture deliberately updates authoritative events along with the note.
    d.style.tracks[0][0].tick = 80;
    d.style.tracks[0][0].data[1] = 62;
    d.style.tracks[0][1].tick = 200;
    d.style.tracks[0][1].data[1] = 62;
    for (target, expected) in [
        (Target::Starts, (0, 120)),
        (Target::Ends, (80, 40)),
        (Target::Lengths, (80, 120)),
        (Target::Both, (0, 120)),
    ] {
        let p = plan(
            &d,
            0,
            9,
            &[],
            &QuantizeOptions {
                target,
                from_original: true,
                ..Default::default()
            },
            Some(&baseline),
        )
        .unwrap();
        if expected == (80, 120) {
            assert!(p.is_noop());
        } else {
            assert_eq!(timing(&p, 1), expected);
            assert_eq!((p.notes[0].pitch, p.notes[0].velocity), (62, 75));
        }
    }
    let p = reset(&d, 0, 9, &[], &baseline, false).unwrap();
    assert_eq!(timing(&p, 1), (50, 100));
    assert_eq!((p.notes[0].pitch, p.notes[0].velocity), (62, 75));
}
#[test]
fn grids_use_section_origin_and_real_ppq_without_resampling() {
    let mut d = fixture(&[(50, 20, 60)], 480, 480);
    let base = 5760;
    for n in &mut d.notes {
        n.start = n.start * 4 + base;
        n.duration *= 4;
    }
    for e in d.style.tracks.iter_mut().flatten() {
        e.tick = e.tick * 4 + base;
    }
    d.sections[0].start = base;
    d.sections[0].end = base + 1920;
    d.style.ppq = 1920;
    let before = d.style.tracks.clone();
    let p = run(&d, Default::default());
    assert_eq!(timing(&p, 1), (base, 80));
    assert_eq!(d.style.ppq, 1920);
    assert_eq!(d.style.tracks, before);
}
#[test]
fn soft_off_forces_full_strength_and_lengths_ignore_retained_swing() {
    let d = fixture(&[(50, 100, 60)], 480, 480);
    assert_eq!(
        timing(
            &run(
                &d,
                QuantizeOptions {
                    soft: false,
                    strength: 0.,
                    ..Default::default()
                }
            ),
            1
        ),
        (0, 100)
    );
    assert_eq!(
        timing(
            &run(
                &d,
                QuantizeOptions {
                    target: Target::Lengths,
                    swing: 100.,
                    ..Default::default()
                }
            ),
            1
        ),
        (50, 120)
    );
    let d = fixture(&[(110, 20, 60)], 480, 480);
    // A disabled UI Swing field retains its value when Tuplet is selected.
    assert_eq!(
        timing(
            &run(
                &d,
                QuantizeOptions {
                    tuplet: 5,
                    swing: 100.,
                    ..Default::default()
                }
            ),
            1
        )
        .0,
        128
    );
}
#[test]
fn original_zero_tick_pair_retains_its_edge_identity_when_translated() {
    let d = fixture(&[(50, 0, 60)], 480, 480);
    assert_eq!(d.notes[0].duration, 1);
    let p = run(&d, Default::default());
    assert_eq!(timing(&p, 1), (0, 1));
}

#[test]
fn recognized_eq_packets_stay_put_but_interleaved_volume_follows_notes() {
    let mut d = fixture(&[(50, 100, 60)], 480, 480);
    for (i, (cc, value)) in [(99, 1), (98, 48), (7, 90), (6, 70), (101, 127), (100, 127)]
        .into_iter()
        .enumerate()
    {
        d.style.tracks[0].push(Event {
            id: 100 + i as u64,
            tick: 80,
            order: 100 + i as u64,
            data: vec![0xb8, cc, value],
        });
    }
    let p = run(&d, Default::default());
    assert_eq!(p.events, vec![EventMove { id: 102, tick: 30 }]);
    assert_eq!(p.baseline.events.len(), 1);
    // Even an earlier malformed/stale baseline cannot re-time EQ on Reset.
    let baseline = Baseline {
        notes: p.baseline.notes,
        events: (100..106)
            .map(|id| EventOriginal {
                id,
                tick: 60,
                owner_note_id: 1,
            })
            .collect(),
    };
    let restored = reset(&d, 0, 9, &[], &baseline, false).unwrap();
    assert_eq!(restored.events, vec![EventMove { id: 102, tick: 60 }]);
}

#[test]
fn full_dense_eighth_result_matches_saved_live_sfm_output() {
    let case: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/sfm-quantize-dense-eighth-oracle.json"
    ))
    .unwrap();
    let rows = case["notes"].as_array().unwrap();
    let notes: Vec<_> = rows
        .iter()
        .map(|n| {
            (
                n["beforeStart"].as_u64().unwrap(),
                n["beforeDuration"].as_u64().unwrap(),
                n["pitch"].as_u64().unwrap() as u8,
            )
        })
        .collect();
    let d = fixture(
        &notes,
        case["sectionLength"].as_u64().unwrap(),
        case["ppq"].as_u64().unwrap() as u16,
    );
    let p = run(
        &d,
        QuantizeOptions {
            size: Grid::Eighth,
            ..Default::default()
        },
    );
    for (i, row) in rows.iter().enumerate() {
        let id = i as u64 * 2 + 1;
        let current = d.notes.iter().find(|n| n.id == id).unwrap();
        let result = p
            .notes
            .iter()
            .find(|n| n.id == id)
            .map(|n| (n.start, n.duration))
            .unwrap_or((current.start, current.duration));
        assert_eq!(
            result,
            (
                row["afterStart"].as_u64().unwrap(),
                row["afterDuration"].as_u64().unwrap()
            ),
            "SFM note {i}"
        );
    }
}
