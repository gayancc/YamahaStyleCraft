use genos_core::{
    document::Document,
    edit::{Command, Session},
    midi, smf,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
fn fixture() -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
    )
    .unwrap()
}
fn session() -> Session {
    Session::open(&fixture(), "Piano integration".into()).unwrap()
}
fn command(v: Value) -> Command {
    serde_json::from_value(v).unwrap()
}
fn apply(s: &mut Session, v: Value) {
    s.execute(s.revision, command(v)).unwrap();
}
fn scope(s: &Session) -> (u64, u64, u64) {
    let sec = s
        .document
        .sections
        .iter()
        .find(|s| s.label == "Main A")
        .unwrap();
    (sec.id, sec.start, sec.end)
}
fn update(n: &genos_core::document::Note, velocity: u8) -> Value {
    json!({"id":n.id,"start":n.start,"duration":n.duration,"pitch":n.pitch,"velocity":velocity})
}
fn opaque(s: &Session) -> Vec<smf::Chunk> {
    s.document
        .style
        .chunks
        .iter()
        .filter(|c| c.tag != *b"MTrk")
        .cloned()
        .collect()
}

#[test]
fn moving_notes_retimes_owned_cc_and_skips_bank_nrpn() {
    let mut s = session();
    let (section, start, end) = scope(&s);
    let note = s
        .document
        .notes
        .iter()
        .find(|n| n.channel == 11 && n.start >= start && n.start + n.duration < end)
        .unwrap()
        .clone();
    let ch = note.channel;
    let status = 0xb0 | (ch - 1);
    let cc_tick = note.start + note.duration / 2;
    apply(
        &mut s,
        json!({
            "type":"controllerPoints",
            "section":section,
            "channel":ch,
            "controller":11,
            "points":[{"tick":cc_tick,"value":90}]
        }),
    );
    // Bank select at same span must not move.
    apply(
        &mut s,
        json!({
            "type":"controllerPoints",
            "section":section,
            "channel":ch,
            "controller":7,
            "points":[{"tick":cc_tick,"value":100}]
        }),
    );
    let before_expr = s
        .view()
        .events
        .iter()
        .find(|e| e.tick == cc_tick && e.data == [status, 11, 90])
        .unwrap()
        .id;
    let delta = 120i64;
    apply(
        &mut s,
        json!({
            "type":"updateNotes",
            "section":section,
            "ids":[note.id],
            "deltaTick":delta,
            "moveControllers":true
        }),
    );
    let events = s.view().events;
    assert!(
        events
            .iter()
            .any(|e| e.id == before_expr && e.tick == cc_tick + delta as u64 && e.data[2] == 90),
        "expression CC should move with note start"
    );
    assert!(
        events
            .iter()
            .any(|e| e.tick == cc_tick + delta as u64 && e.data == [status, 7, 100]),
        "volume CC should also move"
    );
    s.undo().unwrap();
}

#[test]
fn velocity_curve_is_one_history_entry_preserves_every_unrelated_payload_and_roundtrips() {
    let mut s = session();
    let before = s.document.style.bytes().unwrap();
    let vendor = opaque(&s);
    let (sec, start, end) = scope(&s);
    let ns: Vec<_> = s
        .document
        .notes
        .iter()
        .filter(|n| n.channel == 11 && n.start >= start && n.start + n.duration <= end)
        .take(4)
        .cloned()
        .collect();
    assert_eq!(ns.len(), 4);
    let ids: Vec<_> = ns.iter().map(|n| n.id).collect();
    let unchanged: Vec<_> = s
        .document
        .style
        .tracks
        .iter()
        .flatten()
        .filter(|e| !ids.contains(&e.id))
        .cloned()
        .collect();
    apply(
        &mut s,
        json!({"type":"pianoNotes","section":sec,"channel":11,"updates":ns.iter().enumerate().map(|(i,n)|update(n,20+i as u8*30)).collect::<Vec<_>>()}),
    );
    assert_eq!(s.revision, 1);
    assert_eq!(opaque(&s), vendor);
    for e in unchanged {
        assert_eq!(
            s.document
                .style
                .tracks
                .iter()
                .flatten()
                .find(|n| n.id == e.id),
            Some(&e)
        );
    }
    for (i, n) in ns.iter().enumerate() {
        assert_eq!(
            s.document
                .notes
                .iter()
                .find(|m| m.id == n.id)
                .unwrap()
                .velocity,
            20 + i as u8 * 30
        );
    }
    let edited = s.document.style.bytes().unwrap();
    let reopened = Document::parse(&edited).unwrap();
    assert_eq!(reopened.style.bytes().unwrap(), edited);
    assert_eq!(
        reopened
            .notes
            .iter()
            .map(|n| (n.track, n.channel, n.pitch, n.start, n.duration, n.velocity))
            .collect::<Vec<_>>(),
        s.document
            .notes
            .iter()
            .map(|n| (n.track, n.channel, n.pitch, n.start, n.duration, n.velocity))
            .collect::<Vec<_>>()
    );
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
    assert!(!s.view().can_undo);
    s.redo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), edited);
}

#[test]
fn scoped_note_transactions_reject_foreign_ids_duplicates_bounds_and_invalid_velocity_atomically() {
    let s = session();
    let (sec, start, end) = scope(&s);
    let n = s
        .document
        .notes
        .iter()
        .find(|n| n.channel == 11 && n.start >= start && n.start + n.duration <= end)
        .unwrap()
        .clone();
    let good = update(&n, n.velocity);
    let mut bad = good.clone();
    bad["velocity"] = json!(0);
    let mut outside = good.clone();
    outside["start"] = json!(end);
    let mut cross = good.clone();
    cross["duration"] = json!(end - n.start + 1);
    for v in [
        json!({"updates":[good.clone()],"channel":12}),
        json!({"updates":[good.clone(),good.clone()]}),
        json!({"updates":[bad]}),
        json!({"updates":[outside]}),
        json!({"updates":[cross]}),
        json!({"remove":[n.id],"updates":[good.clone()]}),
        json!({"remove":[n.id],"channel":0}),
    ] {
        let mut s = session();
        let bytes = s.document.style.bytes().unwrap();
        let mut base = json!({"type":"pianoNotes","section":sec,"channel":11});
        base.as_object_mut()
            .unwrap()
            .extend(v.as_object().unwrap().clone());
        assert!(s.execute(0, command(base)).is_err());
        assert_eq!(s.revision, 0);
        assert!(!s.view().can_undo);
        assert_eq!(s.document.style.bytes().unwrap(), bytes);
    }
}

#[test]
fn mixed_chop_transaction_adds_deletes_and_updates_in_one_candidate() {
    let mut s = session();
    let (sec, start, end) = scope(&s);
    let n = s
        .document
        .notes
        .iter()
        .find(|n| {
            n.channel == 11 && n.start >= start && n.duration >= 4 && n.start + n.duration <= end
        })
        .unwrap()
        .clone();
    let before = s.document.style.bytes().unwrap();
    let count = s.document.notes.len();
    let split = n.duration / 2;
    apply(
        &mut s,
        json!({"type":"pianoNotes","section":sec,"channel":11,"remove":[n.id],"add":[{"track":n.track,"channel":11,"pitch":n.pitch,"start":n.start,"duration":split,"velocity":61},{"track":n.track,"channel":11,"pitch":n.pitch,"start":n.start+split,"duration":n.duration-split,"velocity":92}]}),
    );
    assert_eq!(s.document.notes.len(), count + 1);
    assert!(!s.document.notes.iter().any(|m| m.id == n.id));
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), before);
    assert!(!s.view().can_undo);
}

#[test]
fn volume_expression_and_modulation_remain_channel_events_in_playback_and_export() {
    let mut s = session();
    let (sec, start, end) = scope(&s);
    let vendor = opaque(&s);
    let original = s.document.style.bytes().unwrap();
    for ch in 9..=16 {
        for cc in [1, 7, 10, 11, 64, 71, 74, 91, 93] {
            let before: Vec<_> = s
                .document
                .style
                .tracks
                .iter()
                .flatten()
                .filter(|e| {
                    !(e.data.len() == 3
                        && e.data[0] == 0xb0 | (ch - 1)
                        && e.data[1] == cc
                        && [start, start + 1, end - 1].contains(&e.tick))
                })
                .cloned()
                .collect();
            apply(
                &mut s,
                json!({"type":"controllerPoints","section":sec,"channel":ch,"controller":cc,"points":[{"tick":start,"value":0},{"tick":start+1,"value":64},{"tick":end-1,"value":127}]}),
            );
            for e in before {
                assert_eq!(
                    s.document
                        .style
                        .tracks
                        .iter()
                        .flatten()
                        .find(|n| n.id == e.id),
                    Some(&e)
                );
            }
            let plan = midi::plan(&s.document, sec, &[ch]).unwrap();
            for value in [0, 64, 127] {
                assert!(
                    plan.packets
                        .iter()
                        .any(|p| p.data == vec![0xb0 | (ch - 1), cc, value])
                );
            }
        }
    }
    assert_eq!(opaque(&s), vendor);
    assert_eq!(
        s.document.notes.len(),
        Document::parse(&original).unwrap().notes.len()
    );
    let b = s.document.style.bytes().unwrap();
    let reopened = Document::parse(&b).unwrap();
    assert_eq!(reopened.style.bytes().unwrap(), b);
    let payloads = |d: &Document| {
        d.style
            .tracks
            .iter()
            .map(|es| {
                es.iter()
                    .filter(|e| e.meta(0x2f).is_none())
                    .map(|e| (e.tick, e.data.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(payloads(&reopened), payloads(&s.document));
}

#[test]
fn mixer_cc_all_sections_is_one_undo_and_writes_each_section_start() {
    let mut s = session();
    let (sec, _, _) = scope(&s);
    let channel = 11u8;
    let baseline = s.document.style.bytes().unwrap();
    let before = s.revision;
    apply(
        &mut s,
        json!({
            "type":"mixerCc",
            "section":sec,
            "channel":channel,
            "controller":7,
            "value":55,
            "allSections":true
        }),
    );
    assert_eq!(s.revision, before + 1);
    let status = 0xb0 | (channel - 1);
    for section in &s.document.sections {
        let hit = s
            .view()
            .events
            .iter()
            .any(|e| e.tick == section.start && e.data == [status, 7, 55]);
        assert!(hit, "missing CC7=55 at {}", section.label);
    }
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), baseline);
    assert!(!s.view().can_undo);
}

#[test]
fn controller_scope_and_reserved_messages_reject_without_history() {
    let mut s = session();
    let (sec, start, end) = scope(&s);
    let before = s.document.style.bytes().unwrap();
    for (cc, tick, value) in [
        (7, end, 64),
        (7, start, 128),
        (0, start, 64),
        (32, start, 64),
        (120, start, 64),
    ] {
        assert!(s.execute(0,command(json!({"type":"controllerPoints","section":sec,"channel":11,"controller":cc,"points":[{"tick":tick,"value":value}]}))).is_err());
        assert_eq!(s.document.style.bytes().unwrap(), before);
        assert_eq!(s.revision, 0);
    }
    let note_id = s.document.notes[0].id;
    assert!(s.execute(0,command(json!({"type":"controllerPoints","section":sec,"channel":11,"controller":7,"remove":[note_id]}))).is_err());
    assert!(!s.view().can_undo);
}

#[test]
fn effective_tied_controller_updates_delete_and_noop_preserve_history() {
    let mut s = session();
    let (sec, start, _) = scope(&s);
    apply(
        &mut s,
        json!({"type":"controllerPoints","section":sec,"channel":11,"controller":11,"points":[{"tick":start+3,"value":77}]}),
    );
    let b = s.document.style.bytes().unwrap();
    let revision = s.revision;
    apply(
        &mut s,
        json!({"type":"controllerPoints","section":sec,"channel":11,"controller":11,"points":[{"tick":start+3,"value":77}]}),
    );
    assert_eq!(s.revision, revision);
    let id = s
        .view()
        .events
        .iter()
        .find(|e| e.tick == start + 3 && e.data == [0xba, 11, 77])
        .unwrap()
        .id;
    assert!(s.execute(s.revision,command(json!({"type":"controllerPoints","section":sec,"channel":12,"controller":11,"remove":[id]}))).is_err());
    assert_eq!(s.document.style.bytes().unwrap(), b);
    apply(
        &mut s,
        json!({"type":"controllerPoints","section":sec,"channel":11,"controller":11,"remove":[id]}),
    );
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), b);
}

#[test]
fn piano_patch_keeps_fifo_overlap_rejection_and_stale_revision_guard() {
    let b = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/research/experiments/outputs/r02-fifo-same-key-overlap.STY"),
    )
    .unwrap();
    let mut s = Session::open(&b, "FIFO".into()).unwrap();
    let n = s.document.notes[0].clone();
    let sec = s.document.sections[0].id;
    let mut patch = update(&n, n.velocity);
    patch["duration"] = json!(400);
    assert!(
        s.execute(
            0,
            command(
                json!({"type":"pianoNotes","section":sec,"channel":n.channel,"updates":[patch]})
            )
        )
        .is_err()
    );
    assert_eq!(s.document.style.bytes().unwrap(), b);
    assert!(s.execute(99,command(json!({"type":"pianoNotes","section":sec,"channel":n.channel,"updates":[update(&n,62)]}))).is_err());
    assert_eq!(s.revision, 0);
}

#[test]
fn corpus_piano_edits_preserve_casm_and_other_sections_across_all_style_parts() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/research/artifacts/style files");
    let mut files = 0;
    let mut checked_channels = std::collections::BTreeSet::new();
    let mut checked_labels = std::collections::BTreeSet::new();
    for file in fs::read_dir(directory).unwrap() {
        let path = file.unwrap().path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("sty"))
        {
            continue;
        }
        let bytes = fs::read(path).unwrap();
        let mut s = Session::open(&bytes, "Corpus piano".into()).unwrap();
        let vendor = opaque(&s);
        let sections = s.document.sections.clone();
        for channel in 9..=16 {
            // Rotate the source section by part/file so the census covers intros, mains, fills and endings.
            let start = (files + channel as usize) % sections.len();
            let found = (0..sections.len()).find_map(|i| {
                let section = &sections[(start + i) % sections.len()];
                s.document
                    .notes
                    .iter()
                    .find(|n| {
                        n.channel == channel
                            && n.start >= section.start
                            && n.start + n.duration <= section.end
                    })
                    .cloned()
                    .map(|n| (section.clone(), n))
            });
            if let Some((section, n)) = found {
                let unchanged: Vec<_> = s
                    .document
                    .style
                    .tracks
                    .iter()
                    .flatten()
                    .filter(|e| e.id != n.id)
                    .cloned()
                    .collect();
                apply(
                    &mut s,
                    json!({"type":"pianoNotes","section":section.id,"channel":channel,"updates":[update(&n,if n.velocity==127{126}else{n.velocity+1})]}),
                );
                for e in unchanged {
                    assert_eq!(
                        s.document
                            .style
                            .tracks
                            .iter()
                            .flatten()
                            .find(|m| m.id == e.id),
                        Some(&e)
                    );
                }
                checked_channels.insert(channel);
                checked_labels.insert(section.label);
            }
        }
        assert_eq!(opaque(&s), vendor);
        let edited = s.document.style.bytes().unwrap();
        assert_eq!(
            Document::parse(&edited).unwrap().style.bytes().unwrap(),
            edited
        );
        files += 1;
    }
    assert!(files > 80);
    assert_eq!(checked_channels, (9..=16).collect());
    assert!(checked_labels.iter().any(|l| l.starts_with("Intro")));
    assert!(checked_labels.iter().any(|l| l.starts_with("Ending")));
    assert!(checked_labels.iter().any(|l| l.starts_with("Fill")));
    println!(
        "PIANO CORPUS: {files} files, {} style parts, {} section labels",
        checked_channels.len(),
        checked_labels.len()
    );
}

#[test]
fn velocity_pitch_move_and_quantize_do_not_lengthen_normalized_zero_duration_notes() {
    let bytes = smf::pack(&[
        smf::Chunk {
            tag: *b"MThd",
            data: vec![0, 0, 0, 1, 1, 224],
        },
        smf::Chunk {
            tag: *b"MTrk",
            data: vec![
                0, 255, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A', 61, 0x9a, 60, 80, 0, 0x8a, 60,
                55, 0x87, 3, 255, 47, 0,
            ],
        },
        smf::Chunk {
            tag: *b"TEST",
            data: vec![42, 0, 99],
        },
    ])
    .unwrap();
    for mode in ["patch", "legacy", "quantize"] {
        let mut s = Session::open(&bytes, "Zero edge".into()).unwrap();
        let n = s.document.notes[0].clone();
        assert_eq!(n.duration, 1);
        let section = s.document.sections[0].id;
        if mode == "patch" {
            let mut p = update(&n, 100);
            p["pitch"] = json!(62);
            p["start"] = json!(120);
            apply(
                &mut s,
                json!({"type":"pianoNotes","section":section,"channel":11,"updates":[p]}),
            );
        } else if mode == "legacy" {
            apply(
                &mut s,
                json!({"type":"updateNotes","section":section,"ids":[n.id],"deltaTick":59,"deltaPitch":2,"velocity":100}),
            );
        } else {
            apply(
                &mut s,
                json!({"type":"quantize","section":section,"ids":[n.id],"step":120}),
            );
        }
        let off = s
            .view()
            .events
            .into_iter()
            .find(|e| e.id == n.off_id)
            .unwrap();
        assert_eq!(off.tick, 120);
        assert_eq!(off.data[2], 55);
        let reopened = Document::parse(&s.document.style.bytes().unwrap()).unwrap();
        let n = &reopened.notes[0];
        let off = reopened.style.tracks[n.track]
            .iter()
            .find(|e| e.id == n.off_id)
            .unwrap();
        assert_eq!(off.tick, n.start);
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), bytes);
    }
}
