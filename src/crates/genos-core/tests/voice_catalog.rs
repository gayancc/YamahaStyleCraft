use genos_core::{
    edit::{Command, Session},
    smf::{self, Chunk, Event},
    voice_catalog::{self, Address, Catalog, Voice},
};
use std::{path::PathBuf, sync::OnceLock};
fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docs/development/voice-catalog")
}
fn catalog() -> Catalog {
    static C: OnceLock<Catalog> = OnceLock::new();
    C.get_or_init(|| {
        voice_catalog::import_pdf(
            &std::fs::read(fixtures().join("genos_en_dl_f0.pdf")).unwrap(),
            "genos_en_dl_f0.pdf",
        )
        .unwrap()
    })
    .clone()
}
fn session() -> Session {
    let raw = vec![
        (0, vec![0xb8, 0, 0]),
        (0, vec![0xb8, 32, 112]),
        (0, vec![0xc8, 0]),
        (480, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A']),
        (480, vec![0xb8, 7, 90]),
        (480, vec![0x98, 60, 100]),
        (600, vec![0x88, 60, 0]),
        (960, vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'B']),
        (960, vec![0xb8, 32, 113]),
        (960, vec![0xc8, 3]),
        (960, vec![0x98, 64, 90]),
        (1080, vec![0x88, 64, 0]),
        (1440, vec![0xff, 0x2f, 0]),
    ];
    let events: Vec<_> = raw
        .into_iter()
        .enumerate()
        .map(|(i, (tick, data))| Event {
            id: i as u64 + 1,
            order: i as u64 + 1,
            tick,
            data,
        })
        .collect();
    Session::open(
        &smf::pack(&[
            Chunk {
                tag: *b"MThd",
                data: vec![0, 0, 0, 1, 1, 224],
            },
            Chunk {
                tag: *b"MTrk",
                data: smf::encode_track(&events).unwrap(),
            },
            Chunk {
                tag: *b"CASM",
                data: vec![],
            },
            Chunk {
                tag: *b"OTSc",
                data: vec![1, 2, 3],
            },
        ])
        .unwrap(),
        "Voice fixture".into(),
    )
    .unwrap()
}
fn apply(s: &mut Session, address: Address) -> genos_core::Result<()> {
    s.execute(
        s.revision,
        Command::SetVoice {
            section: s.document.sections[0].id,
            channel: 9,
            address,
        },
    )
}

#[test]
fn official_pdf_all_1711_rows_match_independent_pdfplumber_cell_extraction() {
    let c = catalog();
    let expected: Vec<Voice> =
        serde_json::from_slice(&std::fs::read(fixtures().join("oracle-voices.json")).unwrap())
            .unwrap();
    assert_eq!(c.voices, expected);
    assert_eq!(c.voices.len(), 1711);
    let voice = c
        .find(Address {
            msb: 104,
            lsb: 21,
            program: 0,
        })
        .unwrap();
    assert_eq!(voice.name, "CFX ConcertGrand");
    assert_eq!(voice.page, 3);
    let voice = c
        .find(Address {
            msb: 8,
            lsb: 2,
            program: 18,
        })
        .unwrap();
    assert_eq!(voice.name, "ActiveBassPickHmrOn");
}

#[test]
fn embedded_factory_matches_every_verified_pdf_entry_and_searches_offline() {
    let embedded = voice_catalog::factory();
    assert_eq!(
        embedded
            .voices
            .iter()
            .filter(|v| v.page > 0)
            .cloned()
            .collect::<Vec<_>>(),
        catalog().voices
    );
    assert_eq!(embedded.summary().count, 2456);
    assert_eq!(
        embedded.search("CFX ConcertGrand").unwrap().voices[0].name,
        "CFX ConcertGrand"
    );
    let kits = embedded.search_drum_kits("").unwrap();
    assert!(!kits.voices.is_empty());
    assert_eq!(kits.total, 58);
    assert!(
        kits.voices
            .iter()
            .all(|v| v.category == "DrumKit" || v.msb == 120)
    );
}

#[test]
fn complete_catalog_matches_every_real_stylemagic_genos_address_and_omits_separators() {
    use std::collections::{BTreeMap, BTreeSet};
    let text: String = include_bytes!("fixtures/genos-stylemagic.idl")
        .iter()
        .map(|b| char::from(*b))
        .collect();
    let mut sections: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut section = "";
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = name;
        } else {
            sections.entry(section).or_default().push(line);
        }
    }
    let mut expected = BTreeMap::new();
    let mut placeholders = BTreeSet::new();
    for kind in [
        "Normal Voice",
        "Organ Flutes",
        "SFX Voice",
        "GM2 Normal",
        "GM2 Rhythm",
        "SFX Kit",
        "Drum Kit",
    ] {
        for line in &sections[kind] {
            let Some((bank, name)) = line.strip_prefix("Patch[").and_then(|s| s.split_once("]="))
            else {
                continue;
            };
            let bank: u16 = bank.parse().unwrap();
            for row in &sections[name] {
                let Some((program, label)) = row.split_once('=') else {
                    continue;
                };
                let Ok(program) = program.parse::<u8>() else {
                    continue;
                };
                let address = Address {
                    msb: (bank >> 7) as u8,
                    lsb: (bank & 127) as u8,
                    program,
                };
                if label.starts_with("<---") {
                    placeholders.insert(address.id());
                    continue;
                }
                assert!(expected.insert(address.id(), label).is_none());
            }
        }
    }
    let factory = voice_catalog::factory();
    let actual: BTreeSet<_> = factory.voices.iter().map(|v| v.address().id()).collect();
    assert_eq!(actual, expected.keys().copied().collect());
    assert_eq!(actual.len(), 2456);
    assert_eq!(placeholders.len(), 10);
    assert!(actual.is_disjoint(&placeholders));
    for voice in factory.voices.iter().filter(|v| v.page == 0) {
        assert_eq!(voice.name, expected[&voice.address().id()]);
    }
}

#[test]
fn browser_pages_reach_every_voice_once_and_filters_preserve_exact_addresses() {
    use std::collections::BTreeSet;
    let catalog = voice_catalog::factory();
    let mut addresses = BTreeSet::new();
    for offset in (0..catalog.voices.len()).step_by(40) {
        let page = catalog.browse("", "all", "", offset, 40).unwrap();
        assert_eq!(page.total, 2456);
        assert_eq!(page.offset, offset);
        assert_eq!(page.has_more, offset + 40 < 2456);
        for voice in page.voices {
            assert!(addresses.insert(voice.address().id()));
        }
    }
    assert_eq!(addresses.len(), 2456);
    assert_eq!(catalog.browse("", "panel", "", 0, 40).unwrap().total, 1711);
    assert_eq!(catalog.browse("", "xg", "", 0, 40).unwrap().total, 480);
    assert_eq!(catalog.browse("", "gm2", "", 0, 40).unwrap().total, 265);
    assert_eq!(catalog.browse("", "drums", "", 0, 100).unwrap().total, 58);
    assert_eq!(catalog.browse("", "voices", "", 0, 40).unwrap().total, 2398);
    assert!(
        catalog
            .search("104:21:1")
            .unwrap()
            .voices
            .iter()
            .any(|v| v.name == "CFX ConcertGrand")
    );
    let piano = catalog.browse("", "xg", "Piano", 0, 100).unwrap();
    assert!(piano.total > 0);
    assert!(
        piano
            .voices
            .iter()
            .all(|v| v.page == 0 && v.category == "Piano")
    );
    assert!(catalog.browse("", "Genos2", "", 0, 40).is_err());
    assert!(catalog.browse("", "all", "", 0, 0).is_err());
    assert_eq!(
        catalog
            .browse("", "all", "", usize::MAX, 40)
            .unwrap()
            .voices
            .len(),
        0
    );
}

#[test]
fn stock_voices_work_without_import_and_with_a_damaged_or_unreadable_cache() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voice-catalog.json");
    assert_eq!(
        voice_catalog::available(&path).voices,
        voice_catalog::factory().voices
    );
    assert!(
        !path.exists(),
        "default lookup does not write or require a cache"
    );
    for bytes in [b"{broken".to_vec(), vec![b'x'; 2 * 1024 * 1024 + 1]] {
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            voice_catalog::available(&path).voices,
            voice_catalog::factory().voices
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "preserve damaged local data"
        );
    }
    assert_eq!(
        voice_catalog::available(dir.path()).voices,
        voice_catalog::factory().voices
    );
    let mut imported = catalog();
    imported.source_name = "Previously imported Data List.pdf".into();
    voice_catalog::save(&path, &imported).unwrap();
    assert_eq!(
        voice_catalog::available(&path).source_name,
        imported.source_name
    );
}
#[test]
fn search_uses_category_name_and_type_terms_and_returns_bounded_results() {
    let c = catalog();
    let result = c.search("piano CFX").unwrap();
    assert!(result.total > 5);
    assert!(
        result
            .voices
            .iter()
            .all(|v| v.category == "Piano" && v.name.contains("CFX"))
    );
    let all = c.search("").unwrap();
    assert_eq!(all.total, 1711);
    assert_eq!(all.voices.len(), 100);
    assert_eq!(c.search("this-voice-does-not-exist").unwrap().total, 0);
    assert!(c.search(&"x".repeat(121)).is_err());
}
#[test]
fn invalid_pdf_editions_and_oversize_reject_and_cache_roundtrips_with_integrity() {
    let c = catalog();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.json");
    voice_catalog::save(&path, &c).unwrap();
    let before = std::fs::read(&path).unwrap();
    for bad in [
        vec![],
        b"%PDF-1.7 fake other edition".to_vec(),
        vec![0; voice_catalog::MAX_PDF_BYTES + 1],
    ] {
        assert!(voice_catalog::import_pdf(&bad, "bad.pdf").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    assert_eq!(
        voice_catalog::load(&path).unwrap().unwrap().voices,
        c.voices
    );
    let mut bad = c.clone();
    bad.voices[0].msb = 128;
    assert!(voice_catalog::save(&path, &bad).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut corrupt: serde_json::Value = serde_json::from_slice(&before).unwrap();
    corrupt["voices"][0]["name"] = "Changed".into();
    std::fs::write(&path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
    assert!(voice_catalog::load(&path).is_err());
    std::fs::write(&path, b"{bad").unwrap();
    assert!(voice_catalog::load(&path).is_err());
    assert!(
        voice_catalog::load(&dir.path().join("absent"))
            .unwrap()
            .is_none()
    );
}
#[test]
fn named_voice_is_one_transaction_with_ordered_bank_program_before_notes() {
    let mut s = session();
    let before = s.document.clone();
    let bytes = before.style.bytes().unwrap();
    let address = Address {
        msb: 104,
        lsb: 21,
        program: 0,
    };
    apply(&mut s, address).unwrap();
    assert_eq!(s.revision, 1);
    let start = s.document.sections[0].start;
    assert_eq!(
        voice_catalog::address_at(&s.document, start, 9),
        Some(address)
    );
    let at_start: Vec<_> = s.document.style.tracks[0]
        .iter()
        .filter(|e| e.tick == start)
        .map(|e| e.data.clone())
        .collect();
    assert_eq!(
        at_start[..3],
        [vec![0xb8, 0, 104], vec![0xb8, 32, 21], vec![0xc8, 0]]
    );
    for e in before.style.tracks.iter().flatten() {
        assert!(s.document.style.tracks.iter().flatten().any(|n| n == e));
    }
    for c in before.style.chunks.iter().filter(|c| c.tag != *b"MTrk") {
        assert!(s.document.style.chunks.contains(c));
    }
    let changed = s.document.style.bytes().unwrap();
    let reparsed = genos_core::document::Document::parse(&changed).unwrap();
    assert_eq!(
        voice_catalog::address_at(&reparsed, start, 9),
        Some(address)
    );
    apply(&mut s, address).unwrap();
    assert_eq!(s.revision, 1);
    assert_eq!(s.document.style.bytes().unwrap(), changed);
    s.undo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), bytes);
    s.redo().unwrap();
    assert_eq!(s.document.style.bytes().unwrap(), changed);
}
#[test]
fn invalid_stale_voice_edits_are_atomic_and_later_events_remain_explicit() {
    let mut s = session();
    let before = s.document.style.bytes().unwrap();
    let id = s.document.sections[0].id;
    for address in [
        Address {
            msb: 128,
            lsb: 0,
            program: 0,
        },
        Address {
            msb: 0,
            lsb: 128,
            program: 0,
        },
        Address {
            msb: 0,
            lsb: 0,
            program: 128,
        },
    ] {
        assert!(apply(&mut s, address).is_err());
        assert_eq!(s.document.style.bytes().unwrap(), before);
    }
    assert!(
        s.execute(
            99,
            Command::SetVoice {
                section: id,
                channel: 9,
                address: Address {
                    msb: 0,
                    lsb: 0,
                    program: 0
                }
            }
        )
        .is_err()
    );
    assert!(
        s.execute(
            0,
            Command::SetVoice {
                section: 999,
                channel: 9,
                address: Address {
                    msb: 0,
                    lsb: 0,
                    program: 0
                }
            }
        )
        .is_err()
    );
    assert!(
        s.execute(
            0,
            Command::SetVoice {
                section: id,
                channel: 0,
                address: Address {
                    msb: 0,
                    lsb: 0,
                    program: 0
                }
            }
        )
        .is_err()
    );
    apply(
        &mut s,
        Address {
            msb: 104,
            lsb: 21,
            program: 0,
        },
    )
    .unwrap();
    // Existing subsequent bank/program events survive and retain normal MIDI inheritance.
    assert_eq!(
        voice_catalog::address_at(&s.document, 960, 9),
        Some(Address {
            msb: 104,
            lsb: 113,
            program: 3
        })
    );
}
#[test]
fn resolution_latches_banks_at_program_change_instead_of_pending_ccs() {
    let mut s = session();
    let id = s.document.sections[0].id;
    s.execute(
        0,
        Command::AddEvent {
            section: id,
            track: 0,
            tick: 700,
            data: vec![0xb8, 0, 104],
        },
    )
    .unwrap();
    assert_eq!(
        voice_catalog::address_at(&s.document, 700, 9),
        Some(Address {
            msb: 0,
            lsb: 112,
            program: 0
        })
    );
    let unknown = session();
    assert!(voice_catalog::address_at(&unknown.document, 0, 16).is_none());
}

#[test]
fn an_already_matching_but_late_program_is_moved_before_boundary_notes() {
    let mut s = session();
    let id = s.document.sections[0].id;
    let address = Address {
        msb: 104,
        lsb: 21,
        program: 0,
    };
    for data in [vec![0xb8, 0, 104], vec![0xb8, 32, 21], vec![0xc8, 0]] {
        s.execute(
            s.revision,
            Command::AddEvent {
                section: id,
                track: 0,
                tick: 480,
                data,
            },
        )
        .unwrap();
    }
    let old_revision = s.revision;
    apply(&mut s, address).unwrap();
    assert_eq!(s.revision, old_revision + 1);
    let row: Vec<_> = s.document.style.tracks[0]
        .iter()
        .filter(|e| e.tick == 480)
        .collect();
    let on = row.iter().position(|e| e.is_on()).unwrap();
    let program = row.iter().position(|e| e.data[0] >> 4 == 0xc).unwrap();
    assert!(program < on);
}

#[test]
fn named_voice_edit_preserves_notes_and_vendor_chunks_across_all_84_styles() {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
        let bytes = std::fs::read(&path).unwrap();
        let mut s = Session::open(&bytes, "Corpus voice".into()).unwrap();
        let section = s
            .document
            .sections
            .iter()
            .find(|sec| {
                s.document
                    .notes
                    .iter()
                    .any(|n| n.start >= sec.start && n.start < sec.end)
            })
            .unwrap();
        let channel = s
            .document
            .notes
            .iter()
            .find(|n| n.start >= section.start && n.start < section.end)
            .unwrap()
            .channel;
        let original = s.document.clone();
        let notes = serde_json::to_value(&original.notes).unwrap();
        s.execute(
            0,
            Command::SetVoice {
                section: section.id,
                channel,
                address: Address {
                    msb: 104,
                    lsb: 21,
                    program: 0,
                },
            },
        )
        .unwrap();
        assert_eq!(serde_json::to_value(&s.document.notes).unwrap(), notes);
        for c in original.style.chunks.iter().filter(|c| c.tag != *b"MTrk") {
            assert!(s.document.style.chunks.contains(c));
        }
        let output = s.document.style.bytes().unwrap();
        genos_core::document::Document::parse(&output).unwrap();
        s.undo().unwrap();
        assert_eq!(s.document.style.bytes().unwrap(), bytes);
        tested += 1;
    }
    assert_eq!(tested, 84);
}

#[test]
fn drum_kit_category_rows_are_complete_and_searchable() {
    let c = catalog();
    let kits = c.drum_kits();
    assert_eq!(kits.len(), 49);
    assert!(kits.iter().all(|v| v.category == "DrumKit"));
    assert!(
        kits.windows(2)
            .all(|w| { w[0].name.to_lowercase() <= w[1].name.to_lowercase() })
    );
    let rock = kits.iter().find(|v| v.name == "RockDrumKit").unwrap();
    assert_eq!(
        (rock.msb, rock.lsb, rock.program, rock.voice_type.as_str()),
        (127, 8, 17, "Revo!Drums")
    );
    let found = c.search_drum_kits("RockDrum").unwrap();
    assert_eq!(found.total, 1);
    assert_eq!(found.voices[0].name, "RockDrumKit");
    assert_eq!(c.search_drum_kits("CFX").unwrap().total, 0);
    let all = c.search_drum_kits("").unwrap();
    assert_eq!(all.total, 49);
    assert_eq!(all.voices.len(), 49);
}

#[test]
fn drum_channel_status_resolves_section_start_kits_and_late_programs() {
    let c = catalog();
    let mut s = session();
    let section = s.document.sections[0].id;
    // session fixture already has ch9 Standard-like banks at tick 0; apply a known DrumKit.
    apply(
        &mut s,
        Address {
            msb: 127,
            lsb: 0,
            program: 0,
        },
    )
    .unwrap();
    let status = voice_catalog::drum_channel_status(&s.document, &c, section).unwrap();
    assert_eq!(status.len(), 2);
    assert_eq!(status[0].channel, 9);
    assert_eq!(status[1].channel, 10);
    let kit = status[0].voice.as_ref().unwrap();
    assert_eq!(kit.category, "DrumKit");
    assert!(status[0].kit_category);
    assert!(!status[0].late_program);
    // Insert a late Program Change on channel 9 inside the section.
    let start = s.document.sections[0].start;
    let end = s.document.sections[0].end;
    let mid = start + (end - start) / 2;
    s.document.style.tracks[0].push(Event {
        id: 9001,
        order: 9001,
        tick: mid,
        data: vec![0xc8, 1],
    });
    let late = voice_catalog::drum_channel_status(&s.document, &c, section).unwrap();
    assert!(late[0].late_program);
}
