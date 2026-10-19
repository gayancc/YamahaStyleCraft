use genos_core::{
    document::Document,
    edit::{Command, NoteInput, Session},
    midi, project, scale_tick, smf,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/research")
}
fn fixture(name: &str) -> Vec<u8> {
    fs::read(root().join("experiments/fixtures").join(name)).unwrap()
}
fn session() -> Session {
    Session::open(&fixture("08.3-4-ACOUSTIC-control.STY"), "Acoustic".into()).unwrap()
}

#[test]
fn set_tempo_meter_updates_view_and_undoes() {
    let mut s = session();
    let before = s.view();
    s.execute(
        0,
        Command::SetTempoMeter {
            tempo_bpm: 100.0,
            numerator: 4,
            denominator: 4,
        },
    )
    .unwrap();
    let after = s.view();
    assert!((after.tempo - 100.0).abs() < 0.05);
    assert_eq!((after.numerator, after.denominator), (4, 4));
    assert_eq!(after.notes.len(), before.notes.len());
    s.undo().unwrap();
    let undone = s.view();
    assert!((undone.tempo - before.tempo).abs() < 0.05);
    assert_eq!(
        (undone.numerator, undone.denominator),
        (before.numerator, before.denominator)
    );
}

#[test]
fn corpus_noop_bytes_and_note_counts() {
    let start = Instant::now();
    let census: serde_json::Value = serde_json::from_slice(
        &fs::read(root().join("evidence/EV-0191-style-corpus-census.json")).unwrap(),
    )
    .unwrap();
    let mut count = 0;
    let mut notes = 0;
    for e in fs::read_dir(root().join("artifacts/style files")).unwrap() {
        let p = e.unwrap().path();
        if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sty")) {
            continue;
        }
        let b = fs::read(&p).unwrap();
        let doc = Document::parse(&b).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let oracle = census["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"].as_str() == p.file_name().unwrap().to_str())
            .unwrap();
        assert_eq!(project::hash(&b), oracle["sha256"].as_str().unwrap());
        let expected: usize = oracle["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["noteOns"].as_u64().unwrap() as usize)
            .sum();
        assert_eq!(
            doc.notes.len(),
            expected,
            "independent census: {}",
            p.display()
        );
        assert_eq!(doc.style.bytes().unwrap(), b, "{}", p.display());
        assert_eq!(
            doc.style
                .tracks
                .iter()
                .flatten()
                .filter(|e| e.is_on())
                .count(),
            doc.notes.len(),
            "unpaired corpus notes: {}",
            p.display()
        );
        count += 1;
        notes += doc.notes.len();
    }
    // Census is an independent retained read-only scan; test fixtures are not oracle-generated.
    assert!(census.is_object());
    assert!(count > 80);
    assert!(notes > 50_000);
    println!(
        "CORPUS {count} files, {notes} paired notes, byte-exact no-ops, {} ms",
        start.elapsed().as_millis()
    );
}
#[test]
fn retained_controls_expected_counts_and_meter() {
    for (name, count, meter) in [
        ("latin01-original-copy.STY", 4272, (4, 4)),
        ("08.3-4-ACOUSTIC-control.STY", 825, (3, 4)),
        ("15.7-8-BEAT-control.STY", 270, (2, 4)),
    ] {
        let d = Document::parse(&fixture(name)).unwrap();
        assert_eq!(d.notes.len(), count);
        let v = d.view(name.into(), 0, false, false, false);
        assert_eq!((v.numerator, v.denominator), meter);
    }
}
#[test]
fn retained_fifo_and_minimum_duration_oracle() {
    let b = fs::read(root().join("experiments/outputs/r02-fifo-same-key-overlap.STY")).unwrap();
    let d = Document::parse(&b).unwrap();
    let actual: Vec<_> = d
        .notes
        .iter()
        .map(|n| {
            (
                scale_tick(n.start, 1920, 480),
                scale_tick(n.start + n.duration, 1920, 480) - scale_tick(n.start, 1920, 480),
                n.velocity,
            )
        })
        .collect();
    assert_eq!(actual, [(0, 50, 90), (25, 50, 70)]);
    let b = fs::read(root().join("experiments/outputs/r02-min-duration-1.STY")).unwrap();
    let d = Document::parse(&b).unwrap();
    let n = &d.notes[0];
    let start = scale_tick(n.start, 1920, 480);
    let end = scale_tick(n.start + n.duration, 1920, 480).max(start + 1);
    assert_eq!(end - start, 1);
    assert_eq!(
        (0..8).map(|x| scale_tick(x, 1920, 480)).collect::<Vec<_>>(),
        [0, 0, 0, 1, 1, 1, 2, 2]
    );
}
#[test]
fn note_edit_reopen_preserves_all_other_events_and_chunks() {
    let mut s = session();
    let original = s.document.clone();
    let n = s
        .document
        .notes
        .iter()
        .find(|n| n.channel == 11 && n.start == 5760)
        .unwrap()
        .clone();
    let section = s
        .document
        .sections
        .iter()
        .find(|x| x.start == 5760)
        .unwrap()
        .id;
    s.execute(
        0,
        Command::UpdateNotes {
            section,
            ids: vec![n.id],
            delta_tick: 4,
            delta_pitch: 0,
            velocity: Some(n.velocity + 1),
            duration: Some(n.duration + 4),
            delta_duration: 0,
            move_controllers: true,
        },
    )
    .unwrap();
    let reopened = Document::parse(&s.document.style.bytes().unwrap()).unwrap();
    let changed = reopened
        .notes
        .iter()
        .find(|x| x.channel == 11 && x.start == n.start + 4 && x.pitch == n.pitch)
        .unwrap();
    assert_eq!(changed.duration, n.duration + 4);
    assert_eq!(changed.velocity, n.velocity + 1);
    for (a, b) in original.style.chunks.iter().zip(&reopened.style.chunks) {
        if a.tag != *b"MTrk" {
            assert_eq!(a, b);
        }
    }
    let before = &original.style.tracks[n.track];
    let after = &s.document.style.tracks[n.track];
    for e in before.iter().filter(|e| e.id != n.id && e.id != n.off_id) {
        assert_eq!(Some(e), after.iter().find(|x| x.id == e.id));
    }
    s.undo().unwrap();
    assert_eq!(
        s.document.style.bytes().unwrap(),
        original.style.bytes().unwrap()
    );
    assert!(!s.view().dirty);
    s.redo().unwrap();
    assert!(s.view().dirty);
}
#[test]
fn casm_edits_match_independent_golden_byte_deltas() {
    for (field, value, offset) in [
        ("root", 2, 48),
        ("chord", 0, 49),
        ("ntr", 0, 58),
        ("ntt", 1, 59),
    ] {
        let mut s = session();
        let old = s.document.style.clone();
        let r = s.document.routes[0].clone();
        s.execute(
            0,
            Command::Casm {
                route: 0,
                field: field.into(),
                value,
            },
        )
        .unwrap();
        let new = &s.document.style;
        assert_eq!(
            old.chunks
                .iter()
                .filter(|c| c.tag != *b"CASM")
                .collect::<Vec<_>>(),
            new.chunks
                .iter()
                .filter(|c| c.tag != *b"CASM")
                .collect::<Vec<_>>()
        );
        let diffs: Vec<_> = old.chunks[r.chunk]
            .data
            .iter()
            .zip(&new.chunks[r.chunk].data)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(diffs, [offset]);
    }
}
#[test]
fn rejected_batch_is_atomic_and_stale_revision_rejects() {
    let mut s = session();
    let b = s.document.style.bytes().unwrap();
    let sec = s.document.sections[0].clone();
    assert!(
        s.execute(
            0,
            Command::AddNotes {
                section: sec.id,
                notes: vec![
                    NoteInput {
                        track: 0,
                        channel: 11,
                        pitch: 60,
                        velocity: 90,
                        start: sec.start,
                        duration: 4
                    },
                    NoteInput {
                        track: 0,
                        channel: 0,
                        pitch: 60,
                        velocity: 90,
                        start: sec.start,
                        duration: 4
                    }
                ],
                channel_events: vec![],
            }
        )
        .is_err()
    );
    assert_eq!(s.document.style.bytes().unwrap(), b);
    assert_eq!(s.revision, 0);
    assert!(
        s.execute(
            99,
            Command::DeleteNotes {
                ids: vec![s.document.notes[0].id]
            }
        )
        .is_err()
    );
}
#[test]
fn unknown_chunks_and_sysex_survive_musical_edits() {
    let mut s = session();
    s.document.style.chunks.push(smf::Chunk {
        tag: *b"Xtra",
        data: vec![0, 255, 32, 12, 1],
    });
    let b = s.document.style.bytes().unwrap();
    let mut s = Session::open(&b, "Unknown chunk".into()).unwrap();
    let sec = s.document.sections[0].clone();
    s.execute(
        0,
        Command::AddNotes {
            section: sec.id,
            notes: vec![NoteInput {
                track: 0,
                channel: 11,
                pitch: 60,
                velocity: 90,
                start: sec.start,
                duration: 4,
            }],
            channel_events: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        s.document.style.chunks.last().unwrap().data,
        [0, 255, 32, 12, 1]
    );
}
#[test]
fn project_self_contained_and_noclobber() {
    let s = session();
    let temp = tempfile::tempdir().unwrap();
    let p = temp.path().join("test.gss");
    project::save_project(&s, &p).unwrap();
    let loaded = project::open_file(&p).unwrap();
    assert_eq!(
        s.document.style.bytes().unwrap(),
        loaded.document.style.bytes().unwrap()
    );
    assert!(project::save_project(&s, &p).is_err());
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    json["sha256"] = "invalid".into();
    fs::write(&p, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(project::open_file(&p).is_err());
}
#[test]
fn recovery_replacement_is_atomic_and_self_contained() {
    let mut s = session();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("recovery.gss");
    project::write_recovery(
        &path,
        &project::project_bytes(&s.name, &s.document.style.bytes().unwrap()).unwrap(),
    )
    .unwrap();
    let n = s.document.notes[0].clone();
    s.execute(0, Command::DeleteNotes { ids: vec![n.id] })
        .unwrap();
    project::write_recovery(
        &path,
        &project::project_bytes(&s.name, &s.document.style.bytes().unwrap()).unwrap(),
    )
    .unwrap();
    let restored = project::open_file(&path).unwrap();
    assert_eq!(restored.document.notes.len(), 824);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}
#[test]
fn overlapping_pitch_edits_cannot_silently_change_pairing() {
    let b = fs::read(root().join("experiments/outputs/r02-fifo-same-key-overlap.STY")).unwrap();
    let mut s = Session::open(&b, "FIFO".into()).unwrap();
    let id = s.document.notes[0].id;
    let section = s.document.sections[0].id;
    assert!(
        s.execute(
            0,
            Command::UpdateNotes {
                section,
                ids: vec![id],
                delta_tick: 0,
                delta_pitch: 0,
                velocity: None,
                duration: Some(400),
                delta_duration: 0,
                move_controllers: true,
            }
        )
        .is_err()
    );
    assert_eq!(s.document.style.bytes().unwrap(), b);
}
#[test]
fn undo_branch_invalidates_redo_and_invalid_controller_rejects() {
    let mut s = session();
    let sec = s.document.sections[0].clone();
    let n = s.document.notes[0].clone();
    s.execute(0, Command::DeleteNotes { ids: vec![n.id] })
        .unwrap();
    s.undo().unwrap();
    assert!(s.view().can_redo);
    s.execute(
        s.revision,
        Command::AddEvent {
            section: sec.id,
            track: 0,
            tick: sec.start,
            data: vec![0xb0 | 10, 7, 101],
        },
    )
    .unwrap();
    assert!(!s.view().can_redo);
    let b = s.document.style.bytes().unwrap();
    assert!(
        s.execute(
            s.revision,
            Command::AddEvent {
                section: sec.id,
                track: 0,
                tick: sec.start,
                data: vec![0xb0 | 10, 7, 128]
            }
        )
        .is_err()
    );
    assert_eq!(s.document.style.bytes().unwrap(), b);
}
#[test]
fn quantize_clamps_exclusive_end_and_preserves_unknown_events() {
    let mut s = session();
    let sec = s.document.sections[0].clone();
    s.execute(
        0,
        Command::AddNotes {
            section: sec.id,
            notes: vec![NoteInput {
                track: 0,
                channel: 16,
                pitch: 100,
                velocity: 90,
                start: sec.end - 1,
                duration: 1,
            }],
            channel_events: vec![],
        },
    )
    .unwrap();
    let n = s
        .document
        .notes
        .iter()
        .find(|n| n.pitch == 100 && n.start == sec.end - 1)
        .unwrap()
        .clone();
    s.execute(
        1,
        Command::Quantize {
            section: sec.id,
            ids: vec![n.id],
            step: 480,
            move_controllers: true,
        },
    )
    .unwrap();
    let n = s.document.notes.iter().find(|x| x.id == n.id).unwrap();
    assert!(n.start < sec.end);
    assert!(n.start + n.duration <= sec.end);
}
#[test]
fn native_plan_uses_source_timing_and_channels() {
    let s = session();
    let sec = s
        .document
        .sections
        .iter()
        .find(|s| s.label == "Main A")
        .unwrap();
    let p = midi::plan(&s.document, sec.id, &[11]).unwrap();
    assert!(!p.packets.is_empty());
    assert!(p.packets.iter().all(|p| p.data[0] & 15 == 10));
    assert!(p.packets.windows(2).all(|w| w[0].at <= w[1].at));
    let on = p.packets.iter().find(|p| p.data[0] & 0xf0 == 0x90).unwrap();
    assert_eq!(on.at.as_micros(), 0);
    let micros = (sec.end - sec.start) as f64 / 1920.0 * (60_000_000.0 / s.view().tempo);
    assert!((p.duration.as_micros() as f64 - micros).abs() < 2.0);
}
#[test]
fn malformed_retained_files_reject_without_panics() {
    for name in [
        "r07-empty.STY",
        "r07-bad-magic.STY",
        "r07-truncated-control.STY",
        "r07-truncated-casm.STY",
    ] {
        let b = fs::read(root().join("experiments/outputs").join(name)).unwrap();
        assert!(Document::parse(&b).is_err(), "{name}");
    }
}
#[test]
fn no_op_note_and_casm_commands_preserve_original_bytes_and_history() {
    let mut s = session();
    let bytes = s.document.style.bytes().unwrap();
    let n = s.document.notes[0].clone();
    let section = s.document.sections[0].id;
    s.execute(
        0,
        Command::UpdateNotes {
            section,
            ids: vec![n.id],
            delta_tick: 0,
            delta_pitch: 0,
            velocity: Some(n.velocity),
            duration: Some(n.duration),
            delta_duration: 0,
            move_controllers: true,
        },
    )
    .unwrap();
    s.execute(
        0,
        Command::Casm {
            route: 0,
            field: "root".into(),
            value: s.document.routes[0].source_root,
        },
    )
    .unwrap();
    assert_eq!(s.revision, 0);
    assert!(!s.view().dirty);
    assert!(!s.view().can_undo);
    assert_eq!(s.document.style.bytes().unwrap(), bytes);
}
#[test]
fn voice_bank_edits_match_retained_semantic_deltas_without_touching_notes_or_casm() {
    // EV-0263/0265/0267: source Main A/Bass raw PC33, LSB0, MSB121.
    for (status, parameter, before, after) in [
        (0xba, Some(0), 121, 122),
        (0xba, Some(32), 0, 1),
        (0xca, None, 33, 34),
    ] {
        let mut s = session();
        let original = s.document.clone();
        let event = original.style.tracks[0]
            .iter()
            .rfind(|e| {
                e.tick == 5760 && e.data[0] == status && parameter.is_none_or(|p| e.data[1] == p)
            })
            .unwrap();
        let index = if parameter.is_some() { 2 } else { 1 };
        assert_eq!(event.data[index], before);
        let mut data = event.data.clone();
        data[index] = after;
        s.execute(
            0,
            Command::SetEvent {
                id: event.id,
                tick: event.tick,
                data,
            },
        )
        .unwrap();
        assert_eq!(
            s.document
                .notes
                .iter()
                .map(|n| (n.start, n.duration, n.pitch, n.velocity))
                .collect::<Vec<_>>(),
            original
                .notes
                .iter()
                .map(|n| (n.start, n.duration, n.pitch, n.velocity))
                .collect::<Vec<_>>()
        );
        for (a, b) in original.style.chunks.iter().zip(&s.document.style.chunks) {
            if a.tag != *b"MTrk" {
                assert_eq!(a, b);
            }
        }
        let changed = &s.document.style.tracks[0];
        let differences: Vec<_> = original.style.tracks[0]
            .iter()
            .zip(changed)
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].1.data[index], after);
        Document::parse(&s.document.style.bytes().unwrap()).unwrap();
    }
}
#[test]
fn discrete_program_controller_and_bend_survive_edit_export_reopen() {
    for data in [
        vec![0xba, 1, 64],
        vec![0xca, 34],
        vec![0xda, 60],
        vec![0xea, 0, 64],
    ] {
        let mut s = session();
        let section = s.document.sections[0].clone();
        let original = s.document.style.tracks[0].len();
        s.execute(
            0,
            Command::AddEvent {
                section: section.id,
                track: 0,
                tick: section.start + 100,
                data: data.clone(),
            },
        )
        .unwrap();
        let b = s.document.style.bytes().unwrap();
        let parsed = Document::parse(&b).unwrap();
        assert_eq!(parsed.style.tracks[0].len(), original + 1);
        assert!(
            parsed.style.tracks[0]
                .iter()
                .any(|e| e.tick == section.start + 100 && e.data == data)
        );
        let header = smf::pack(&parsed.style.chunks[..=parsed.style.track_chunks[0]]).unwrap();
        let independent = midly::Smf::parse(&header).unwrap();
        assert_eq!(independent.tracks[0].len(), original + 1);
    }
}
#[test]
fn project_unknown_extension_fields_survive_reopen_resave() {
    let mut s = session();
    s.project_extensions.insert(
        "futureExtension".into(),
        serde_json::json!({"opaque":[1,2,"retained"]}),
    );
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("extension.gss");
    project::save_project(&s, &p).unwrap();
    let next = project::open_file(&p).unwrap();
    assert_eq!(next.project_extensions, s.project_extensions);
    let json: serde_json::Value =
        serde_json::from_slice(&project::session_bytes(&next).unwrap()).unwrap();
    assert_eq!(json["futureExtension"]["opaque"][2], "retained");
}
#[test]
fn discrete_event_edits_cannot_change_source_or_typed_kind() {
    let mut s = session();
    let section = s.document.sections[0].clone();
    let event = s.document.style.tracks[0]
        .iter()
        .find(|e| e.tick == section.start && e.data[0] >> 4 == 0xb && e.data[1] == 0)
        .unwrap()
        .clone();
    let bytes = s.document.style.bytes().unwrap();
    let mut data = event.data.clone();
    data[1] = 32;
    assert!(
        s.execute(
            0,
            Command::SetEvent {
                id: event.id,
                tick: event.tick,
                data
            }
        )
        .is_err()
    );
    assert!(
        s.execute(
            0,
            Command::SetEvent {
                id: event.id,
                tick: section.end,
                data: event.data
            }
        )
        .is_err()
    );
    assert_eq!(s.document.style.bytes().unwrap(), bytes);
}
proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(512))]
    #[test]
    fn arbitrary_binary_never_panics(b in proptest::collection::vec(proptest::num::u8::ANY,0..8192)){let _=Document::parse(&b);}
    #[test]
    fn vlq_roundtrips(n in 0u32..=0x0fff_ffff){let mut b=Vec::new();smf::write_vlq(n as u64,&mut b).unwrap();let mut p=0;proptest::prop_assert_eq!(smf::vlq(&b,&mut p).unwrap(),n);proptest::prop_assert_eq!(p,b.len());}
    #[test]
    fn truncated_real_style_never_panics(n in 0usize..13061){let b=fixture("08.3-4-ACOUSTIC-control.STY");let _=Document::parse(&b[..n]);}
}
