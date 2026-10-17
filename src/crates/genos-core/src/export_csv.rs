//! Lightweight CSV dump of paired notes. Not SFM profile/DAW interchange.
use crate::{Error, Result, document::Document};
use std::fmt::Write as _;

pub fn section_notes_csv(doc: &Document, section_id: u64) -> Result<String> {
    let section = doc
        .sections
        .iter()
        .find(|s| s.id == section_id)
        .ok_or_else(|| Error::Edit("section not found".into()))?;
    let mut out = String::from("section,channel,pitch,velocity,start,duration,noteId,offId\n");
    // Include notes that overlap the section window (same spirit as playback plan).
    for n in doc
        .notes
        .iter()
        .filter(|n| n.start < section.end && n.start.saturating_add(n.duration) > section.start)
    {
        writeln!(
            out,
            "{},{},{},{},{},{},{},{}",
            csv_escape(&section.label),
            n.channel,
            n.pitch,
            n.velocity,
            n.start,
            n.duration,
            n.id,
            n.off_id
        )
        .map_err(|e| Error::Format(e.to_string()))?;
    }
    Ok(out)
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smf::{self, Event};

    #[test]
    fn csv_lists_section_notes_only() {
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
            Event {
                id: 5,
                tick: 1920,
                order: 5,
                data: vec![0x90, 64, 80],
            },
            Event {
                id: 6,
                tick: 2040,
                order: 6,
                data: vec![0x80, 64, 0],
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
        let csv = section_notes_csv(&doc, main_a.id).unwrap();
        assert!(csv.contains("MAIN A,1,60,90,0,120,2,3"));
        assert!(!csv.contains(",64,"));
    }
}
