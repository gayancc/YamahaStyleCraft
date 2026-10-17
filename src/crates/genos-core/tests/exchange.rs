use genos_core::{
    document::Document,
    edit::{Command, Session},
    exchange::{self, PhraseNote},
    smf::{self, Chunk, Event},
};
fn midi(ppq: u16, tracks: Vec<Vec<(u64, Vec<u8>)>>) -> Vec<u8> {
    let mut chunks = vec![Chunk {
        tag: *b"MThd",
        data: vec![
            0,
            u8::from(tracks.len() > 1),
            0,
            tracks.len() as u8,
            (ppq >> 8) as u8,
            ppq as u8,
        ],
    }];
    for track in tracks {
        let events: Vec<_> = track
            .into_iter()
            .enumerate()
            .map(|(i, (tick, data))| Event {
                id: i as u64 + 1,
                order: i as u64 + 1,
                tick,
                data,
            })
            .collect();
        chunks.push(Chunk {
            tag: *b"MTrk",
            data: smf::encode_track(&events).unwrap(),
        });
    }
    smf::pack(&chunks).unwrap()
}
fn target() -> Session {
    let mut chunks = smf::chunks(&midi(
        480,
        vec![vec![
            (0, vec![0xb8, 0, 121]),
            (0, vec![0xc8, 12]),
            (0, vec![0xff, 0x51, 3, 7, 161, 32]),
            (480, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']),
            (490, vec![0x98, 60, 100]),
            (570, vec![0x88, 60, 12]),
            (500, vec![0x99, 40, 90]),
            (580, vec![0x89, 40, 0]),
            (600, vec![0xb8, 7, 90]),
            (700, vec![0xff, 0x51, 3, 9, 39, 192]),
            (800, vec![0xff, 0x58, 4, 3, 2, 24, 8]),
            (960, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'B']),
            (970, vec![0x98, 65, 80]),
            (1050, vec![0x88, 65, 0]),
            (1440, vec![0xff, 0x2f, 0]),
        ]],
    ))
    .unwrap();
    chunks.push(Chunk {
        tag: *b"CASM",
        data: vec![1, 2, 3, 4],
    }); // Opaque short unknown data retained.
    chunks.push(Chunk {
        tag: *b"OTSc",
        data: vec![99, 42],
    });
    // A valid empty CASM avoids asking this test fixture to interpret its records.
    chunks
        .iter_mut()
        .find(|c| c.tag == *b"CASM")
        .unwrap()
        .data
        .clear();
    Session::open(&smf::pack(&chunks).unwrap(), "Exchange fixture".into()).unwrap()
}
fn incoming(pitch: u8) -> Vec<u8> {
    midi(
        960,
        vec![vec![
            (20, vec![0x90, pitch, 99]),
            (180, vec![0x80, pitch, 0]),
            (960, vec![0xff, 0x2f, 0]),
        ]],
    )
}
fn replace(s: &mut Session, notes: Vec<PhraseNote>) -> genos_core::Result<()> {
    s.execute(
        s.revision,
        Command::ReplacePhrase {
            section: s.document.sections[0].id,
            channel: 9,
            notes,
        },
    )
}

#[test]
fn export_is_standard_midi_with_relative_notes_exact_ppq_and_active_timing_map() {
    let s = target();
    let id = s.document.sections[0].id;
    let bytes = exchange::export(&s.document, id, 9).unwrap();
    let parsed = midly::Smf::parse(&bytes).unwrap();
    assert_eq!(parsed.header.format, midly::Format::SingleTrack);
    let d = Document::parse(&bytes).unwrap();
    assert_eq!(d.style.ppq, 480);
    assert_eq!(d.notes.len(), 1);
    let n = &d.notes[0];
    assert_eq!(
        (n.start, n.duration, n.channel, n.pitch, n.velocity),
        (10, 80, 9, 60, 100)
    );
    assert_eq!(d.style.tracks[0].last().unwrap().tick, 480);
    assert_eq!(d.timing_at(220).0, 100.0);
    assert_eq!(d.timing_at(320).1, 3);
    assert!(d.style.tracks[0].iter().all(|e| e.is_on()
        || e.is_off()
        || e.meta(0x51).is_some()
        || e.meta(0x58).is_some()
        || e.meta(0x2f).is_some()));
}
#[test]
fn exported_return_and_reordered_daw_tracks_are_exact_noops_without_history() {
    let mut s = target();
    let original = s.document.style.bytes().unwrap();
    let id = s.document.sections[0].id;
    let bytes = exchange::export(&s.document, id, 9).unwrap();
    let p = exchange::prepare(&s.document, id, 9, &bytes).unwrap();
    assert!(p.preview.unchanged);
    replace(&mut s, p.notes).unwrap();
    assert_eq!(s.revision, 0);
    assert!(!s.view().can_undo);
    assert_eq!(original, s.document.style.bytes().unwrap());
    let b = midi(
        960,
        vec![
            vec![(0, vec![0xff, 0x51, 3, 1, 2, 3])],
            vec![(20, vec![0x92, 60, 100]), (180, vec![0x82, 60, 0])],
        ],
    );
    let p = exchange::prepare(&s.document, id, 9, &b).unwrap();
    assert!(p.preview.unchanged);
    replace(&mut s, p.notes).unwrap();
    assert_eq!(s.revision, 0);
}
#[test]
fn changed_return_preserves_all_other_notes_events_chunks_and_undo_redo() {
    let mut s = target();
    let original = s.document.style.bytes().unwrap();
    let id = s.document.sections[0].id;
    let old = s.document.clone();
    let p = exchange::prepare(&s.document, id, 9, &incoming(62)).unwrap();
    assert_eq!(
        (
            p.preview.source_channel,
            p.preview.target_channel,
            p.preview.source_ppq,
            p.preview.target_ppq
        ),
        (1, 9, 960, 480)
    );
    assert!(!p.preview.unchanged);
    replace(&mut s, p.notes).unwrap();
    assert_eq!(s.revision, 1);
    let n = s
        .document
        .notes
        .iter()
        .find(|n| n.channel == 9 && n.start < 960)
        .unwrap();
    assert_eq!(
        (n.start, n.duration, n.pitch, n.velocity),
        (490, 80, 62, 99)
    );
    let excluded: Vec<_> = old
        .notes
        .iter()
        .filter(|n| n.channel == 9 && n.start < 960)
        .flat_map(|n| [n.id, n.off_id])
        .collect();
    for e in old
        .style
        .tracks
        .iter()
        .flatten()
        .filter(|e| !excluded.contains(&e.id))
    {
        assert!(s.document.style.tracks.iter().flatten().any(|new| new == e));
    }
    for c in old.style.chunks.iter().filter(|c| c.tag != *b"MTrk") {
        assert!(s.document.style.chunks.contains(c));
    }
    let changed = s.document.style.bytes().unwrap();
    Document::parse(&changed).unwrap();
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), original);
    s.redo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), changed);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exchange.gss");
    let project = genos_core::project::session_bytes(&s).unwrap();
    std::fs::write(&path, project).unwrap();
    let reopened = genos_core::project::open_file(&path).unwrap();
    assert_eq!(reopened.document.style.bytes().unwrap(), changed);
}
#[test]
fn ppq_edges_use_half_even_and_zero_length_after_scaling_rejects() {
    let s = target();
    let id = s.document.sections[0].id;
    let b = midi(
        960,
        vec![vec![(1, vec![0x90, 60, 90]), (5, vec![0x80, 60, 0])]],
    );
    let p = exchange::prepare(&s.document, id, 9, &b).unwrap();
    assert_eq!((p.notes[0].start, p.notes[0].duration), (0, 2));
    let b = midi(
        960,
        vec![vec![(0, vec![0x90, 60, 90]), (1, vec![0x80, 60, 0])]],
    );
    assert!(exchange::prepare(&s.document, id, 9, &b).is_err());
}
#[test]
fn malformed_multichannel_unpaired_nested_and_empty_files_reject() {
    let s = target();
    let id = s.document.sections[0].id;
    let candidates = vec![
        vec![],
        vec![0; 32],
        midi(480, vec![vec![(0, vec![0xff, 0x2f, 0])]]),
        midi(
            480,
            vec![vec![
                (0, vec![0x90, 60, 90]),
                (10, vec![0x80, 60, 0]),
                (20, vec![0x91, 62, 90]),
                (30, vec![0x81, 62, 0]),
            ]],
        ),
        midi(480, vec![vec![(0, vec![0x90, 60, 90])]]),
        midi(
            480,
            vec![vec![
                (0, vec![0x80, 60, 0]),
                (1, vec![0x90, 60, 90]),
                (2, vec![0x80, 60, 0]),
            ]],
        ),
        midi(
            480,
            vec![vec![
                (0, vec![0x90, 60, 90]),
                (5, vec![0x90, 60, 90]),
                (10, vec![0x80, 60, 0]),
                (15, vec![0x80, 60, 0]),
            ]],
        ),
        midi(
            480,
            vec![vec![(0, vec![0x90, 60, 90]), (0, vec![0x80, 60, 0])]],
        ),
        vec![0; exchange::MAX_MIDI_BYTES + 1],
        s.document.style.bytes().unwrap(),
    ];
    for b in candidates {
        assert!(exchange::prepare(&s.document, id, 9, &b).is_err());
    }
}
#[test]
fn out_of_section_and_bad_command_reject_atomically_including_stale_revision() {
    let mut s = target();
    let before = s.document.style.bytes().unwrap();
    for n in [
        PhraseNote {
            start: 470,
            pitch: 60,
            duration: 11,
            velocity: 90,
        },
        PhraseNote {
            start: 0,
            pitch: 128,
            duration: 10,
            velocity: 90,
        },
        PhraseNote {
            start: 0,
            pitch: 60,
            duration: 10,
            velocity: 0,
        },
        PhraseNote {
            start: u64::MAX,
            pitch: 60,
            duration: 10,
            velocity: 90,
        },
    ] {
        assert!(replace(&mut s, vec![n]).is_err());
        assert_eq!(s.document.style.bytes().unwrap(), before);
        assert_eq!(s.revision, 0);
    }
    assert!(replace(&mut s, vec![]).is_err());
    let id = s.document.sections[0].id;
    assert!(
        s.execute(
            99,
            Command::ReplacePhrase {
                section: id,
                channel: 9,
                notes: vec![]
            }
        )
        .is_err()
    );
    assert!(exchange::export(&s.document, id, 0).is_err());
    assert!(exchange::export(&s.document, 999, 9).is_err());
    assert!(exchange::export(&s.document, id, 16).is_err());
    let b = midi(
        480,
        vec![vec![(479, vec![0x90, 60, 90]), (481, vec![0x80, 60, 0])]],
    );
    assert!(exchange::prepare(&s.document, id, 9, &b).is_err());
}
#[test]
fn boundary_crossing_unpaired_and_nested_source_notes_reject() {
    for edges in [
        vec![(470, vec![0x98, 60, 90]), (500, vec![0x88, 60, 0])],
        vec![(500, vec![0x98, 60, 90])],
        vec![(500, vec![0x88, 60, 0])],
        vec![
            (500, vec![0x98, 60, 90]),
            (510, vec![0x98, 60, 90]),
            (520, vec![0x88, 60, 0]),
            (530, vec![0x88, 60, 0]),
        ],
    ] {
        let mut events = vec![(480, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A'])];
        events.extend(edges);
        events.push((960, vec![0xff, 0x2f, 0]));
        let d = Document::parse(&midi(480, vec![events])).unwrap();
        let id = d.sections[0].id;
        assert!(exchange::export(&d, id, 9).is_err());
        assert!(exchange::prepare(&d, id, 9, &incoming(61)).is_err());
    }
}
#[test]
fn adjacent_same_pitch_notes_and_note_off_at_section_end_roundtrip() {
    let mut s = target();
    let id = s.document.sections[0].id;
    let b = midi(
        480,
        vec![vec![
            (0, vec![0x90, 60, 80]),
            (240, vec![0x80, 60, 0]),
            (240, vec![0x90, 60, 90]),
            (480, vec![0x80, 60, 0]),
        ]],
    );
    let p = exchange::prepare(&s.document, id, 9, &b).unwrap();
    replace(&mut s, p.notes).unwrap();
    let b = exchange::export(&s.document, id, 9).unwrap();
    assert!(
        exchange::prepare(&s.document, id, 9, &b)
            .unwrap()
            .preview
            .unchanged
    );
}

#[test]
fn real_corpus_phrase_export_return_preserves_every_original_byte() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/research/artifacts/style files");
    let mut tested = 0;
    for entry in std::fs::read_dir(corpus).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("sty"))
        {
            continue;
        }
        let original = std::fs::read(&path).unwrap();
        let mut session = Session::open(&original, "Corpus".into()).unwrap();
        let mut candidate = None;
        'sections: for section in &session.document.sections {
            for channel in 1..=16 {
                if let Ok(bytes) = exchange::export(&session.document, section.id, channel) {
                    candidate = Some((section.id, channel, bytes));
                    break 'sections;
                }
            }
        }
        let (section, channel, bytes) =
            candidate.unwrap_or_else(|| panic!("no supported phrase: {}", path.display()));
        midly::Smf::parse(&bytes).unwrap();
        let prepared = exchange::prepare(&session.document, section, channel, &bytes).unwrap();
        assert!(prepared.preview.unchanged);
        session
            .execute(
                0,
                Command::ReplacePhrase {
                    section,
                    channel,
                    notes: prepared.notes,
                },
            )
            .unwrap();
        assert_eq!(session.revision, 0);
        assert_eq!(session.document.style.bytes().unwrap(), original);
        tested += 1;
    }
    assert_eq!(tested, 84);
}

#[test]
fn orphan_before_section_cannot_steal_an_imported_note_off() {
    let bytes = midi(
        480,
        vec![vec![
            (0, vec![0x98, 60, 90]),
            (480, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']),
            (960, vec![0xff, 0x2f, 0]),
        ]],
    );
    let mut s = Session::open(&bytes, "Orphan fixture".into()).unwrap();
    let id = s.document.sections[0].id;
    let prepared = exchange::prepare(&s.document, id, 9, &incoming(60)).unwrap();
    assert!(replace(&mut s, prepared.notes).is_err());
    assert_eq!(s.document.style.bytes().unwrap(), bytes);
    assert_eq!(s.revision, 0);
}
