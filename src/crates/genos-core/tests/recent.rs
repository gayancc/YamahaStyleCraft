use genos_core::recent::{self, RecentKind, SCHEMA};
use std::fs;

fn touch(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, b"stub").unwrap();
}

#[test]
fn record_lists_and_dedupes_newest_first_with_cap() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path();
    let mut paths = Vec::new();
    for i in 0..12 {
        let p = app.join(format!("style-{i}.STY"));
        touch(&p);
        paths.push(p);
    }
    for (i, path) in paths.iter().enumerate() {
        recent::record_at(app, path, &format!("2026-10-16T12:{i:02}:00Z")).unwrap();
    }
    let listed = recent::list(app).unwrap();
    assert_eq!(listed.len(), 10);
    assert_eq!(listed[0].entry.name, "style-11.STY");
    assert_eq!(listed[0].entry.kind, RecentKind::Sty);
    assert!(listed[0].available);
    assert_eq!(listed[9].entry.name, "style-2.STY");

    // Re-open an older entry → moves to front, no duplicate.
    recent::record_at(app, &paths[2], "2026-10-16T13:00:00Z").unwrap();
    let listed = recent::list(app).unwrap();
    assert_eq!(listed.len(), 10);
    assert_eq!(listed[0].entry.path, paths[2].to_string_lossy());
    assert_eq!(
        listed
            .iter()
            .filter(|e| e.entry.id == recent::entry_id(&paths[2]))
            .count(),
        1
    );

    let raw = fs::read_to_string(recent::recent_path(app)).unwrap();
    assert!(raw.contains(SCHEMA));
    assert!(raw.len() <= 64 * 1024);
}

#[test]
fn missing_file_marks_unavailable_and_remove_is_safe() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path();
    let present = app.join("ok.gss");
    let missing = app.join("gone.STY");
    touch(&present);
    touch(&missing);
    recent::record_at(app, &present, "2026-10-16T10:00:00Z").unwrap();
    recent::record_at(app, &missing, "2026-10-16T11:00:00Z").unwrap();
    fs::remove_file(&missing).unwrap();

    let listed = recent::list(app).unwrap();
    assert_eq!(listed.len(), 2);
    let gone = listed
        .iter()
        .find(|e| e.entry.name == "gone.STY")
        .expect("missing entry retained");
    assert!(!gone.available);
    assert_eq!(gone.entry.kind, RecentKind::Sty);
    let ok = listed
        .iter()
        .find(|e| e.entry.name == "ok.gss")
        .expect("project entry");
    assert!(ok.available);
    assert_eq!(ok.entry.kind, RecentKind::Project);

    let after = recent::remove(app, &gone.entry.id).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].entry.name, "ok.gss");
    // Removing unknown id does not crash.
    let again = recent::remove(app, "deadbeefdeadbeefdeadbeef").unwrap();
    assert_eq!(again.len(), 1);
}

#[test]
fn corrupt_or_wrong_schema_fails_soft_to_empty() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path();
    let path = recent::recent_path(app);
    fs::create_dir_all(app).unwrap();
    fs::write(&path, b"{not-json").unwrap();
    assert!(recent::list(app).unwrap().is_empty());

    fs::write(
        &path,
        br#"{"schema":"sfm.recent_files.v1","files":[{"id":"x","path":"a.STY","name":"a.STY","kind":"sty","openedAt":"t"}]}"#,
    )
    .unwrap();
    assert!(recent::list(app).unwrap().is_empty());
}

#[test]
fn rejects_unsupported_extension() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("notes.txt");
    touch(&p);
    let err = recent::record(dir.path(), &p).unwrap_err().to_string();
    assert!(err.to_lowercase().contains("recent files"));
}
