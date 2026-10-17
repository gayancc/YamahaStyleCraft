//! Blank Style authoring from a required Yamaha reference scaffold.
//!
//! Evidence: EV-0141/EV-0142/EV-0151 (Composer defaults), EV-0171 (export needs
//! scaffold). Synthetic 15-section STY bytes without a reference remain blocked.
use crate::{
    Error, Result,
    document::Document,
    edit::Session,
    smf::{self, Event, Style},
};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlankOptions {
    pub name: String,
    pub tempo_bpm: f64,
    pub numerator: u8,
    pub denominator: u16,
}

fn reject(s: &str) -> Error {
    Error::Edit(s.into())
}

fn validate_options(o: &BlankOptions) -> Result<()> {
    let name = o.name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(reject("name must be 1..120 characters"));
    }
    if !o.tempo_bpm.is_finite() || !(20.0..=400.0).contains(&o.tempo_bpm) {
        return Err(reject("tempo must be a finite BPM between 20 and 400"));
    }
    if !(1..=16).contains(&o.numerator) {
        return Err(reject("numerator must be 1..16"));
    }
    if !matches!(o.denominator, 1 | 2 | 4 | 8 | 16) {
        return Err(reject("denominator must be 1, 2, 4, 8, or 16"));
    }
    Ok(())
}

fn has_main_section(style: &Style) -> bool {
    style.tracks.iter().flatten().any(|e| {
        e.meta(6).is_some_and(|b| {
            String::from_utf8_lossy(b)
                .trim_matches(char::from(0))
                .trim()
                .to_ascii_uppercase()
                .starts_with("MAIN ")
        })
    })
}

pub fn tempo_bytes(bpm: f64) -> Result<[u8; 3]> {
    let us = (60_000_000.0 / bpm).round();
    if !(1.0..=16_777_215.0).contains(&us) {
        return Err(reject("tempo meta out of MIDI range"));
    }
    let n = us as u32;
    Ok([
        ((n >> 16) & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        (n & 0xff) as u8,
    ])
}

pub fn meter_bytes(numerator: u8, denominator: u16) -> [u8; 4] {
    let exp = denominator.trailing_zeros() as u8;
    // cc/bb follow common SMF defaults used by viewed Styles (24 clocks/click, 8 32nds/quarter).
    [numerator, exp, 24, 8]
}

pub fn set_or_insert_meta(
    track: &mut Vec<Event>,
    kind: u8,
    payload: &[u8],
    next_id: &mut u64,
) -> Result<()> {
    let mut data = vec![0xff, kind];
    smf::write_vlq(payload.len() as u64, &mut data)?;
    data.extend(payload);
    if let Some(e) = track
        .iter_mut()
        .find(|e| e.tick == 0 && e.meta(kind).is_some())
    {
        e.data = data;
        return Ok(());
    }
    *next_id += 1;
    track.insert(
        0,
        Event {
            id: *next_id,
            tick: 0,
            order: *next_id,
            data,
        },
    );
    Ok(())
}

pub fn apply_tempo_meter(
    style: &mut Style,
    tempo_bpm: f64,
    numerator: u8,
    denominator: u16,
) -> Result<()> {
    let tempo = tempo_bytes(tempo_bpm)?;
    let meter = meter_bytes(numerator, denominator);
    let mut next = style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap_or(0);
    // All header copies must agree, including metadata in another MIDI track.
    for event in style.tracks.iter_mut().flatten().filter(|e| e.tick == 0) {
        if event.meta(0x51).is_some() {
            event.data = vec![0xff, 0x51, 3, tempo[0], tempo[1], tempo[2]];
        }
        if event.meta(0x58).is_some() {
            event.data = vec![0xff, 0x58, 4, meter[0], meter[1], meter[2], meter[3]];
        }
    }
    let track = style
        .tracks
        .first_mut()
        .ok_or_else(|| reject("reference Style has no tracks"))?;
    set_or_insert_meta(track, 0x51, &tempo, &mut next)?;
    set_or_insert_meta(track, 0x58, &meter, &mut next)?;
    Ok(())
}

/// Clear note events from a reference Style and apply Composer tempo/meter.
/// Preserves markers, CASM, OTSc, controllers, and programs for export fidelity.
pub fn create_blank_from_reference(bytes: &[u8], options: BlankOptions) -> Result<Session> {
    validate_options(&options)?;
    let name = options.name.trim().to_string();
    let mut style = Style::parse(bytes)?;
    if !has_main_section(&style) {
        return Err(reject("reference Style must contain a MAIN section marker"));
    }
    if !style
        .tracks
        .iter()
        .flatten()
        .any(|e| e.meta(6) == Some(b"SFF2"))
        || style
            .tracks
            .iter()
            .flatten()
            .any(|e| e.meta(6) == Some(b"SFF1"))
    {
        return Err(Error::Unsupported("Blank authoring requires an SFF2 reference; SFF1 and MIDI-only scaffolds are not qualified".into()));
    }
    let before_notes = style.tracks.iter().flatten().filter(|e| e.is_on()).count();
    for track in &mut style.tracks {
        track.retain(|e| !(e.is_on() || e.is_off()));
    }
    apply_tempo_meter(
        &mut style,
        options.tempo_bpm,
        options.numerator,
        options.denominator,
    )?;
    for (i, track) in style.tracks.iter().enumerate() {
        style.chunks[style.track_chunks[i]].data = smf::encode_track(track)?;
    }
    let out = style.bytes()?;
    let document = Document::parse(&out)?;
    if !document
        .sections
        .iter()
        .any(|s| s.label.to_ascii_uppercase().starts_with("MAIN "))
    {
        return Err(reject(
            "blank result lost MAIN section markers; choose another reference",
        ));
    }
    if !document.notes.is_empty() {
        return Err(reject("blank result still contains paired notes"));
    }
    let mut session = Session::open(&out, name)?;
    session.mark_unsaved();
    session.project_extensions.insert(
        "authoring".into(),
        serde_json::json!({
            "mode": "blank_from_reference",
            "evidence": ["EV-0141", "EV-0142", "EV-0151", "EV-0171"],
            "clearedNotes": before_notes,
            "limitations": [
                "Section layout and CASM come from the reference Style.",
                "Synthetic 15-section blank STY without a scaffold remains unavailable.",
                "SFF1 blank authoring is not enabled."
            ]
        }),
    );
    Ok(session)
}
