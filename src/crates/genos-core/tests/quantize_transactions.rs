use genos_core::{
    edit::{Command, QUANTIZE_STATE, Session},
    project,
    smf::{self, Chunk, Event},
};
use serde_json::{Value, json};

fn session(start: u64) -> Session {
    let messages = [
        (0, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']),
        (0, vec![0xb8, 7, 100]),
        (start, vec![0x98, 60, 90]),
        (80, vec![0xb8, 7, 80]),
        (start + 100, vec![0x88, 60, 0]),
        (480, vec![0xff, 0x2f, 0]),
    ];
    let events: Vec<_> = messages
        .into_iter()
        .enumerate()
        .map(|(i, (tick, data))| Event {
            id: i as u64 + 1,
            order: i as u64 + 1,
            tick,
            data,
        })
        .collect();
    let bytes = smf::pack(&[
        Chunk {
            tag: *b"MThd",
            data: vec![0, 0, 0, 1, 1, 224],
        },
        Chunk {
            tag: *b"MTrk",
            data: smf::encode_track(&events).unwrap(),
        },
        Chunk {
            tag: *b"TEST",
            data: vec![1, 2, 3],
        },
    ])
    .unwrap();
    Session::open(&bytes, "Timing transaction".into()).unwrap()
}
fn apply(s: &mut Session, options: Value, reset: bool) {
    let section = s.document.sections[0].id;
    let cmd: Command = serde_json::from_value(json!({"type":"advancedQuantize",
        "section":section,"channel":9,"options":options,"reset":reset}))
    .unwrap();
    s.execute(s.revision, cmd).unwrap();
}
fn timing(s: &Session) -> (u64, u64, u64) {
    let n = &s.document.notes[0];
    let cc = s
        .document
        .style
        .tracks
        .iter()
        .flatten()
        .find(|e| e.data == [0xb8, 7, 80])
        .unwrap();
    (n.start, n.duration, cc.tick)
}

#[test]
fn manual_auto_reset_and_history_commit_notes_controllers_and_originals_together() {
    let mut s = session(50);
    s.project_extensions
        .insert("custom".into(), json!({"keep":true}));
    let before = s.document.style.bytes().unwrap();
    apply(&mut s, json!({"strength":50}), false);
    assert_eq!(timing(&s), (25, 100, 55));
    assert!(s.project_extensions.contains_key(QUANTIZE_STATE));
    apply(&mut s, json!({"fromOriginal":true}), false);
    assert_eq!(timing(&s), (0, 100, 30));
    apply(&mut s, json!({}), true);
    assert_eq!(timing(&s), (50, 100, 80));
    assert!(!s.project_extensions.contains_key(QUANTIZE_STATE));
    for expected in [(0, 100, 30), (25, 100, 55), (50, 100, 80)] {
        s.undo().unwrap();
        assert_eq!(timing(&s), expected);
    }
    assert_eq!(s.document.style.bytes().unwrap(), before);
    assert!(!s.view().dirty);
    assert!(!s.project_extensions.contains_key(QUANTIZE_STATE));
    for expected in [(25, 100, 55), (0, 100, 30), (50, 100, 80)] {
        s.redo().unwrap();
        assert_eq!(timing(&s), expected);
    }
    assert_eq!(s.project_extensions["custom"], json!({"keep":true}));
    assert_eq!(s.document.style.chunks.last().unwrap().data, [1, 2, 3]);
}

#[test]
fn serialized_project_restores_originals_after_event_order_changes() {
    let mut s = session(50);
    let before_id = s.document.notes[0].id;
    apply(&mut s, json!({}), false);
    assert_eq!(timing(&s), (0, 100, 30));
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("quantized.gss");
    project::save_project(&s, &file).unwrap();
    let mut reopened = project::open_file(&file).unwrap();
    assert_eq!(reopened.document.notes[0].id, before_id);
    assert!(!reopened.view().dirty);
    assert!(reopened.view().quantize_original.is_some());
    apply(&mut reopened, json!({}), true);
    assert_eq!(timing(&reopened), (50, 100, 80));
    assert!(reopened.view().dirty);
    reopened.undo().unwrap();
    assert_eq!(timing(&reopened), (0, 100, 30));
    assert!(!reopened.view().dirty);
    // A project ID map is untrusted data; duplicate bindings must be rejected.
    let mut payload: Value = serde_json::from_slice(&project::session_bytes(&s).unwrap()).unwrap();
    payload["genosEventIdentityV1"][0][1][0] = payload["genosEventIdentityV1"][0][0][0].clone();
    let bad = folder.path().join("duplicate.gss");
    std::fs::write(&bad, serde_json::to_vec(&payload).unwrap()).unwrap();
    assert!(project::open_file(&bad).is_err());
}

#[test]
fn no_op_preserves_redo_and_does_not_create_a_baseline_or_revision() {
    let mut s = session(0);
    apply(&mut s, json!({}), false);
    assert_eq!(s.revision, 0);
    assert!(!s.view().can_undo);
    assert!(!s.view().dirty);
    assert!(s.view().quantize_original.is_none());
    apply(&mut s, json!({"target":"lengths"}), false);
    s.undo().unwrap();
    assert!(s.view().can_redo);
    let revision = s.revision;
    apply(&mut s, json!({}), false);
    assert_eq!(s.revision, revision);
    assert!(s.view().can_redo);
}

#[test]
fn malformed_or_foreign_project_originals_are_rejected_before_exposing_a_view() {
    let mut s = session(50);
    apply(&mut s, json!({}), false);
    let payload: Value = serde_json::from_slice(&project::session_bytes(&s).unwrap()).unwrap();
    let key = format!("{}:9", s.document.sections[0].id);
    let folder = tempfile::tempdir().unwrap();
    let mut cases = Vec::new();
    let mut bad = payload.clone();
    bad[QUANTIZE_STATE][&key] = json!({"notes":null,"events":[]});
    cases.push(bad);
    let mut bad = payload.clone();
    bad[QUANTIZE_STATE][&key]["notes"][0]["id"] = json!(999999);
    cases.push(bad);
    let mut bad = payload.clone();
    bad[QUANTIZE_STATE][&key]["events"][0]["ownerNoteId"] = json!(999999);
    cases.push(bad);
    let mut bad = payload.clone();
    let original = bad[QUANTIZE_STATE][&key].take();
    bad[QUANTIZE_STATE] = json!({"0:1":original});
    cases.push(bad);
    let mut bad = payload.clone();
    bad.as_object_mut().unwrap().remove("genosEventIdentityV1");
    cases.push(bad);
    for (i, bad) in cases.into_iter().enumerate() {
        let file = folder.path().join(format!("bad-{i}.gss"));
        std::fs::write(&file, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(project::open_file(&file).is_err(), "case {i}");
    }
}

#[test]
fn deleting_a_quantized_note_prunes_its_originals_and_undo_restores_them() {
    let mut s = session(50);
    apply(&mut s, json!({}), false);
    let originals = s.view().quantize_original;
    let id = s.document.notes[0].id;
    s.execute(s.revision, Command::DeleteNotes { ids: vec![id] })
        .unwrap();
    assert!(s.view().quantize_original.is_none());
    s.undo().unwrap();
    assert_eq!(s.view().quantize_original, originals);
    apply(&mut s, json!({}), true);
    assert_eq!(timing(&s), (50, 100, 80));
}
