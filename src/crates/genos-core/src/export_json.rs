//! Lightweight JSON dump of section notes/events. Not SFM profile interchange.
use crate::{Error, Result, document::Document};
use serde_json::{Value, json};

pub fn section_snapshot_pretty(doc: &Document, section_id: u64) -> Result<String> {
    Ok(serde_json::to_string_pretty(&section_snapshot(
        doc, section_id,
    )?)?)
}

pub fn section_snapshot(doc: &Document, section_id: u64) -> Result<Value> {
    let section = doc
        .sections
        .iter()
        .find(|s| s.id == section_id)
        .ok_or_else(|| Error::Edit("section not found".into()))?;
    let events: Vec<_> = doc.style.tracks.iter().flatten().cloned().collect();
    let (tempo, numerator, denominator) = doc.timing_at(section.start);
    let notes: Vec<_> = doc
        .notes
        .iter()
        .filter(|n| n.start < section.end && n.start.saturating_add(n.duration) > section.start)
        .map(|n| {
            json!({
                "id": n.id,
                "offId": n.off_id,
                "channel": n.channel,
                "pitch": n.pitch,
                "velocity": n.velocity,
                "start": n.start,
                "duration": n.duration,
                "track": n.track,
            })
        })
        .collect();
    let section_events: Vec<_> = events
        .iter()
        .filter(|e| e.tick >= section.start && e.tick < section.end)
        .map(|e| {
            json!({
                "id": e.id,
                "tick": e.tick,
                "order": e.order,
                "data": e.data,
            })
        })
        .collect();
    Ok(json!({
        "schema": "genos.section_snapshot.v1",
        "ppq": doc.style.ppq,
        "tempo": tempo,
        "numerator": numerator,
        "denominator": denominator,
        "section": {
            "id": section.id,
            "label": section.label,
            "start": section.start,
            "end": section.end,
        },
        "notes": notes,
        "events": section_events,
        "limitations": [
            "Independent JSON snapshot for inspection/export — not SFM profile or DAW exchange.",
            "CASM chord transformations are not applied."
        ]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smf::{self, Event};

    #[test]
    fn snapshot_lists_overlapping_section_notes() {
        let events = vec![
            Event {
                id: 1,
                tick: 0,
                order: 1,
                data: vec![0xff, 6, 6, b'M', b'A', b'I', b'N', b' ', b'A'],
            },
            Event {
                id: 2,
                tick: 0,
                order: 2,
                data: vec![0x90, 60, 90],
            },
            Event {
                id: 3,
                tick: 120,
                order: 3,
                data: vec![0x80, 60, 0],
            },
            Event {
                id: 4,
                tick: 1920,
                order: 4,
                data: vec![0xff, 6, 6, b'M', b'A', b'I', b'N', b' ', b'B'],
            },
        ];
        let chunks = [
            smf::Chunk {
                tag: *b"MThd",
                data: vec![0, 0, 0, 1, 1, 224],
            },
            smf::Chunk {
                tag: *b"MTrk",
                data: smf::encode_track(&events).unwrap(),
            },
        ];
        let doc = Document::parse(&smf::pack(&chunks).unwrap()).unwrap();
        let main_a = doc.sections.iter().find(|s| s.label == "MAIN A").unwrap();
        let v = section_snapshot(&doc, main_a.id).unwrap();
        assert_eq!(v["schema"], "genos.section_snapshot.v1");
        assert_eq!(v["notes"].as_array().unwrap().len(), 1);
        assert_eq!(v["notes"][0]["pitch"], 60);
    }
    #[test]
    fn snapshot_uses_timing_active_at_selected_section_not_the_first_tempo() {
        let es = vec![
            Event {
                id: 1,
                tick: 0,
                order: 1,
                data: vec![0xff, 0x51, 3, 7, 161, 32],
            },
            Event {
                id: 2,
                tick: 0,
                order: 2,
                data: vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A'],
            },
            Event {
                id: 3,
                tick: 480,
                order: 3,
                data: vec![0xff, 0x51, 3, 9, 39, 192],
            },
            Event {
                id: 4,
                tick: 480,
                order: 4,
                data: vec![0xff, 0x58, 4, 3, 2, 24, 8],
            },
            Event {
                id: 5,
                tick: 480,
                order: 5,
                data: vec![0xff, 6, 6, b'M', b'a', b'i', b'n', b' ', b'B'],
            },
            Event {
                id: 6,
                tick: 960,
                order: 6,
                data: vec![0xff, 0x2f, 0],
            },
        ];
        let bytes = smf::pack(&[
            smf::Chunk {
                tag: *b"MThd",
                data: vec![0, 0, 0, 1, 1, 224],
            },
            smf::Chunk {
                tag: *b"MTrk",
                data: smf::encode_track(&es).unwrap(),
            },
        ])
        .unwrap();
        let doc = Document::parse(&bytes).unwrap();
        let v = section_snapshot(&doc, doc.sections[1].id).unwrap();
        assert_eq!(v["tempo"], 100.0);
        assert_eq!(v["numerator"], 3);
        assert_eq!(v["denominator"], 4);
    }
}
