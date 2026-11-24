//! Read-only Ctb2-guided MIDI preview, not a Yamaha arranger emulator.
//! EV-0108 preview_casm/chord_preview/drum_setup contracts; no raw Style DSP replay.
use crate::{
    Error, Result,
    casm::supported_ctb2_size,
    chord_preview::{self, PitchRule},
    document::{Document, Route},
    firmware_ntt,
    genos_system_effect::xg_parameter_change,
    midi::{Packet, Plan},
    native_drums, style_effects, voice_catalog,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    time::Duration,
};

fn bad(text: impl Into<String>) -> Error {
    Error::Unsupported(text.into())
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chord {
    pub root: u8,
    pub quality: String,
    pub bars: u8,
    #[serde(default)]
    pub bass: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bass_quality: Option<String>,
}
impl Chord {
    fn alternate(&self) -> (u8, &str) {
        (
            self.bass.unwrap_or(self.root),
            self.bass_quality.as_deref().unwrap_or(&self.quality),
        )
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub section: u64,
    pub sources: Vec<u8>,
    pub chords: Vec<Chord>,
    pub effects: bool,
    pub drums: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub label: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub notes: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteInfo {
    pub source: u8,
    pub destination: u8,
    pub notes: usize,
    pub drum: bool,
    pub voice: voice_catalog::Address,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub area: String,
    pub status: String,
    pub detail: String,
    pub count: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub steps: Vec<Step>,
    pub routes: Vec<RouteInfo>,
    pub coverage: Vec<Coverage>,
    pub warnings: Vec<String>,
    pub duration_ms: u64,
    pub setup_packets: usize,
    pub timed_packets: usize,
    pub note_ons: usize,
    pub changed_pitches: usize,
    pub suppressed_notes: usize,
    pub boundary_rearticulations: usize,
    pub coalesced_note_ons: usize,
}
pub struct Compiled {
    pub plan: Plan,
    pub report: Report,
    pub channels: Vec<u8>,
}
fn quality_bit(q: &str) -> Option<u8> {
    Some(match q {
        "Major" => 0,
        "Maj7" => 2,
        "aug" => 7,
        "Minor" => 8,
        "m7" => 10,
        "m7b5" => 11,
        "dim" => 17,
        "7" => 19,
        "sus4" => 32,
        "sus2" => 33,
        _ => return None,
    })
}
fn source_quality(q: u8) -> Option<&'static str> {
    Some(match q {
        0 => "MajorTriad",
        2 => "Maj7",
        8 => "Minor",
        10 => "m7",
        14 => "m7(11)",
        19 => "7",
        _ => return None,
    })
}
pub(crate) fn target_type(quality: &str) -> Option<u8> {
    quality_bit(quality).or_else(|| {
        let id = quality.strip_prefix("Genos:")?.parse::<u8>().ok()?;
        firmware_ntt::configured().ok().flatten()?;
        (id < firmware_ntt::CHORD_TYPE_COUNT).then_some(id)
    })
}

pub fn supports_target_quality(quality: &str) -> bool {
    target_type(quality).is_some()
}

fn enabled(r: &Route, root: u8, quality: &str) -> bool {
    if root > 11 {
        return false;
    }
    let root_enabled = r.bytes[12 - usize::from(root / 8)] & (1 << (root % 8)) != 0;
    root_enabled
        && target_type(quality).is_some_and(|id| {
            let mask = if let Some(tables) = firmware_ntt::configured().ok().flatten() {
                tables.chord_mask(id)
            } else {
                Some((17 - usize::from(id / 8), 1 << (id % 8)))
            };
            mask.is_some_and(|(byte, bit)| r.bytes[byte] & bit != 0)
        })
}
fn rule<'a>(
    r: &'a Route,
    note: u8,
    chord: &'a Chord,
    drum: bool,
) -> Result<(PitchRule<'a>, u8, u8)> {
    if !supported_ctb2_size(r.bytes.len())
        || r.bytes[0] >= 16
        || r.bytes[9] >= 16
        || r.bytes[0] + 1 != r.source
        || r.bytes[9] + 1 != r.destination
        || r.bytes[18] > 11
        || r.bytes[19] >= firmware_ntt::CHORD_TYPE_COUNT
    {
        return Err(bad("Invalid Ctb2 route binding or source chord"));
    }
    // The firmware loads the imported zone bytes as signed bounds and tests
    // the lower bound first, including when the stored bounds are inverted.
    let zone_note = i16::from(note);
    let offset = if zone_note < i16::from(r.bytes[20] as i8) {
        22
    } else if zone_note > i16::from(r.bytes[21] as i8) {
        34
    } else {
        28
    };
    let b = &r.bytes[offset..offset + 6];
    let ntr = match b[0] {
        0 => "root_trans",
        1 => "root_fixed",
        2 if !drum && firmware_ntt::configured()?.is_some() && (b[1] & 127) <= 2 => "guitar",
        _ => {
            return Err(bad(format!(
                "Source {}: guitar/unknown NTR {} is not available in routed preview",
                r.source, b[0]
            )));
        }
    };
    let ntt = match b[1] & 127 {
        0 => "bypass",
        1 => "melody",
        2 => "chord",
        3 => "melodic_minor",
        4 => "melodic_minor_5th",
        5 => "harmonic_minor",
        6 => "harmonic_minor_5th",
        7 => "natural_minor",
        8 => "natural_minor_5th",
        9 => "dorian",
        10 => "dorian_5th",
        n => {
            return Err(bad(format!(
                "Source {}: NTT {n} is not supported",
                r.source
            )));
        }
    };
    let flagged = (b[3] | b[4]) & 128 != 0;
    let firmware =
        ntr == "guitar" || (!drum && ntt != "bypass" && firmware_ntt::configured()?.is_some());
    let inverted = (b[3] & 127) > (b[4] & 127);
    if ((b[2] > 11 || flagged || inverted) && (drum || (ntt != "bypass" && !firmware)))
        || b[5] > 5
        || (b[5] == 5 && (drum || firmware_ntt::configured()?.is_none()))
    {
        return Err(bad(
            "Invalid Ctb2 output range, high key or retrigger value",
        ));
    }
    let (target_root, target_quality) = if b[1] & 128 != 0 {
        chord.alternate()
    } else {
        (chord.root, chord.quality.as_str())
    };
    Ok((
        PitchRule {
            source_root: r.bytes[18],
            source_quality: if ntt == "bypass" || firmware {
                // Bypass does not use chord quality. Root Transposition still
                // applies its root shift, high key and limits; Root Fixed is
                // literal. The raw source-chord byte was validated above.
                "Major"
            } else {
                source_quality(r.bytes[19])
                    .ok_or_else(|| bad(format!("Unsupported source chord {}", r.bytes[19])))?
            },
            target_root,
            target_quality,
            ntr,
            ntt,
            high_key: Some(b[2]),
            note_limit_low: b[3],
            note_limit_high: b[4],
            kind: "pitched",
            bass: -1,
            expansion_layout: false,
        },
        b[5],
        b[1],
    ))
}
fn apply_bypass_limits(mut pitch: i16, low: u8, high: u8) -> Option<u8> {
    // Genos applies the lower and upper limits in order. A flagged bound
    // rejects a violation, while an ordinary bound folds by octaves.
    let lower = i16::from(low & 127);
    if low & 128 != 0 && pitch < lower {
        return None;
    }
    while pitch < lower {
        pitch += 12;
    }
    let upper = i16::from(high & 127);
    if high & 128 != 0 && pitch > upper {
        return None;
    }
    while pitch > upper {
        pitch -= 12;
    }
    (0..=127).contains(&pitch).then_some(pitch as u8)
}

fn converted(
    note: u8,
    rule: &PitchRule<'_>,
    table_ids: Option<(u8, u8, &Chord)>,
    to_root: bool,
) -> Result<Option<u8>> {
    if rule.ntr == "guitar"
        && let Some((ntt, _, chord)) = table_ids
        && let Some(tables) = firmware_ntt::configured()?
    {
        let note = if to_root && (48..=95).contains(&note) {
            note - note % 12
        } else {
            note
        };
        let ordinary = (chord.root, chord.quality.as_str());
        let alternate = (rule.target_root, rule.target_quality);
        let (pitch, (root, quality)) = if ntt & 128 != 0 && note % 12 <= 1 && ordinary != alternate
        {
            (note - note % 12, alternate)
        } else {
            (note, ordinary)
        };
        return Ok(
            target_type(quality).and_then(|id| tables.guitar_pitch(pitch, ntt & 127, root, id))
        );
    }
    if to_root {
        let note = i16::from(note);
        let mut shift = (note - i16::from(rule.target_root)).rem_euclid(12);
        if rule.ntr == "root_trans" {
            if rule.target_root > rule.source_root {
                shift -= 12;
            }
            if rule.exceeds_firmware_high_key() {
                shift += 12;
            }
        } else if shift > 5 {
            shift -= 12;
        }
        return Ok(apply_bypass_limits(
            note - shift,
            rule.note_limit_low,
            rule.note_limit_high,
        ));
    }
    if rule.ntt == "bypass" {
        let mut pitch = i16::from(note);
        if rule.ntr == "root_trans" {
            pitch += i16::from(rule.target_root) - i16::from(rule.source_root);
            if rule.exceeds_firmware_high_key() {
                pitch -= 12;
            }
        }
        return Ok(apply_bypass_limits(
            pitch,
            rule.note_limit_low,
            rule.note_limit_high,
        ));
    }
    if let Some((ntt, source_type, _)) = table_ids
        && let Some(tables) = firmware_ntt::configured()?
        && let Some(target_type) = target_type(rule.target_quality)
    {
        let pitch = tables.pitch(note, rule, ntt & 127, source_type, target_type);
        return Ok(apply_bypass_limits(
            pitch,
            rule.note_limit_low,
            rule.note_limit_high,
        ));
    }

    // The retained study helper has an identity shortcut; enforce CASM limits even there.
    let pitch = chord_preview::preview_pitch(note, rule)?;
    let Some(pitch) = pitch else {
        return Ok(None);
    };
    Ok((rule.note_limit_low..=rule.note_limit_high)
        .filter(|candidate| i16::from(*candidate).rem_euclid(12) == i16::from(pitch).rem_euclid(12))
        .min_by_key(|candidate| i16::from(*candidate).abs_diff(i16::from(pitch))))
}
fn coverage(area: &str, status: &str, detail: impl Into<String>, count: usize) -> Coverage {
    Coverage {
        area: area.into(),
        status: status.into(),
        detail: detail.into(),
        count,
    }
}

fn keep_packet(
    packet: &Packet,
    active: &mut BTreeMap<(u8, u8), usize>,
    coalesced: &mut usize,
) -> bool {
    let status = packet.data[0] & 0xf0;
    if !matches!(status, 0x80 | 0x90) {
        return true;
    }
    let entry = active
        .entry((packet.data[0] & 15, packet.data[1]))
        .or_default();
    if status == 0x90 {
        let emit = *entry == 0;
        *entry += 1;
        *coalesced += usize::from(!emit);
        emit
    } else {
        *entry = entry.saturating_sub(1);
        *entry == 0
    }
}

/// Pitch a source note sounds at while the style follows `chord`, using the section's Ctb2
/// route for that source channel. Without a usable route the note simply transposes by the
/// chord root. `None` is a note the route mutes for this chord.
pub fn follow_pitch(
    doc: &Document,
    section: u64,
    source: u8,
    note: u8,
    chord: &crate::midi::LiveChord,
) -> Option<u8> {
    follow_pitch_in_window(doc, section, source, note, chord, false)
}

/// Map a live note at its scheduled time since the most recent root change.
pub fn follow_pitch_at(
    doc: &Document,
    section: u64,
    source: u8,
    note: u8,
    chord: &crate::midi::LiveChord,
    since_root_change: Option<Duration>,
) -> Option<u8> {
    let active = since_root_change.is_some_and(|elapsed| {
        doc.sections
            .iter()
            .find(|s| s.id == section)
            .is_some_and(|section| {
                let (bpm, _, _) = doc.timing_at(section.start);
                Duration::try_from_secs_f64(30.0 / bpm).is_ok_and(|window| elapsed < window)
            })
    });
    follow_pitch_in_window(doc, section, source, note, chord, active)
}

/// Map a live note with the transport's tempo-aware root-change window state.
pub fn follow_pitch_in_window(
    doc: &Document,
    section: u64,
    source: u8,
    note: u8,
    chord: &crate::midi::LiveChord,
    in_window: bool,
) -> Option<u8> {
    follow_routed_pitch(doc, section, source, note, chord, in_window).pitch
}

/// Map a live note and report whether its route allows a held-note slide.
pub fn follow_routed_pitch(
    doc: &Document,
    section: u64,
    source: u8,
    note: u8,
    chord: &crate::midi::LiveChord,
    in_window: bool,
) -> crate::midi::LivePitch {
    if chord.root > 11 || chord.bass.is_some_and(|root| root > 11) {
        return None.into();
    }
    let Some(section) = doc.sections.iter().find(|s| s.id == section) else {
        return None.into();
    };
    let chord = Chord {
        root: chord.root,
        quality: chord.quality.clone(),
        bars: 1,
        bass: chord.bass,
        bass_quality: chord.bass_quality.clone(),
    };
    match playback_route(doc, &section.label, source) {
        Some(route) => match rule(route, note, &chord, false) {
            Ok((rule, rtr, ntt)) if !enabled(route, rule.target_root, rule.target_quality) => {
                let bass_region = ntt & 128 != 0;
                let held_mask_portamento =
                    matches!(route.destination, 9 | 10) && matches!(rtr, 1 | 2) && {
                        let (other_root, other_quality) = if bass_region {
                            (chord.root, chord.quality.as_str())
                        } else {
                            chord.alternate()
                        };
                        let uses_other_chord = [23, 29, 35]
                            .into_iter()
                            .any(|offset| (route.bytes[offset] & 128 != 0) != bass_region);
                        !uses_other_chord || !enabled(route, other_root, other_quality)
                    };
                crate::midi::LivePitch {
                    bass_region,
                    held_mask_portamento,
                    ..None.into()
                }
            }
            Ok((rule, rtr, ntt)) => {
                let firmware = firmware_ntt::configured().is_ok_and(|tables| tables.is_some());
                let to_root = firmware && in_window && matches!(rtr, 2 | 4);
                match converted(note, &rule, Some((ntt, route.bytes[19], &chord)), to_root) {
                    Ok(pitch) => crate::midi::LivePitch {
                        pitch,
                        portamento: firmware && matches!(rtr, 1..=4),
                        stop_on_change: firmware && matches!(rtr, 0 | 5),
                        bass_region: ntt & 128 != 0,
                        held_mask_portamento: false,
                    },
                    Err(_) => Some(note).into(),
                }
            }
            Err(_) => Some(note).into(),
        },
        _ => {
            let root = i16::from(chord.root);
            let shift = if root > 6 { root - 12 } else { root };
            u8::try_from(i16::from(note) + shift)
                .ok()
                .filter(|n| *n <= 127)
                .into()
        }
    }
}

fn playback_route<'a>(doc: &'a Document, section: &str, source: u8) -> Option<&'a Route> {
    doc.firmware_cseg_route(section, source).unwrap_or_else(|| {
        let mut routes = doc.routes.iter().filter(|route| {
            route.source == source && route.sections.iter().any(|label| label == section)
        });
        let first = routes.next();
        if routes.next().is_none() { first } else { None }
    })
}

pub fn compile(doc: &Document, config: &Config) -> Result<Compiled> {
    if config.sources.is_empty()
        || config.sources.len() > 16
        || config.sources.iter().any(|c| !(1..=16).contains(c))
        || config.sources.iter().collect::<HashSet<_>>().len() != config.sources.len()
    {
        return Err(bad("Select distinct source channels 1..16"));
    }
    if config.chords.is_empty()
        || config.chords.len() > 32
        || config.chords.iter().any(|c| {
            c.root > 11
                || c.bass.is_some_and(|bass| bass > 11)
                || !supports_target_quality(&c.quality)
                || c.bass_quality
                    .as_deref()
                    .is_some_and(|quality| !supports_target_quality(quality))
                || !(1..=16).contains(&c.bars)
        })
    {
        return Err(bad("Choose 1..32 supported chords, 1..16 bars each"));
    }
    if doc
        .warnings
        .iter()
        .any(|w| w.contains("Ctb2") || w.contains("Ctab"))
    {
        return Err(bad(
            "Opaque or malformed CASM records prevent complete route coverage",
        ));
    }
    let section = doc
        .sections
        .iter()
        .find(|s| s.id == config.section)
        .ok_or_else(|| bad("Section not found"))?;
    let length = section
        .end
        .checked_sub(section.start)
        .filter(|n| *n > 0)
        .ok_or_else(|| bad("Empty section"))?;
    let (bpm, num, den) = doc.timing_at(section.start);
    let per_bar = u64::from(doc.style.ppq) * 4 * u64::from(num) / u64::from(den);
    if per_bar == 0 || length % per_bar != 0 {
        return Err(bad(
            "Live progression currently requires a section with whole bars",
        ));
    }
    let mut events = doc.style.tracks.iter().flatten().collect::<Vec<_>>();
    events.sort_by_key(|e| (e.tick, e.order, e.id));
    if events.iter().any(|e| {
        e.tick > section.start
            && e.tick < section.end
            && (e.meta(0x51).is_some() || e.meta(0x58).is_some())
    }) {
        return Err(bad(
            "Tempo/meter changes inside the section require a variable-timing preview",
        ));
    }
    let seconds_per_tick = 60.0 / bpm / f64::from(doc.style.ppq);
    let total_ticks = config
        .chords
        .iter()
        .map(|c| u64::from(c.bars) * per_bar)
        .sum::<u64>();
    if total_ticks as f64 * seconds_per_tick > 600.0 {
        return Err(bad("Limit the progression to ten minutes"));
    }
    let at = |tick: u64| Duration::from_secs_f64(tick as f64 * seconds_per_tick);
    let routes = config
        .sources
        .iter()
        .map(|source| {
            playback_route(doc, &section.label, *source).ok_or_else(|| {
                bad(format!(
                    "Source {source} needs exactly one Ctb2 route in {}",
                    section.label
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    // Source controllers/voices cannot be merged reliably onto one external part.
    if routes
        .iter()
        .map(|r| r.destination)
        .collect::<HashSet<_>>()
        .len()
        != routes.len()
    {
        return Err(bad(
            "Several selected sources share a destination; select one source per destination for this preview",
        ));
    }
    let paired = doc.notes.iter().map(|n| n.id).collect::<HashSet<_>>();
    if events.iter().any(|e| {
        e.is_on()
            && e.tick < section.end
            && config.sources.contains(&e.channel().unwrap())
            && !paired.contains(&e.id)
    }) {
        return Err(bad(
            "Unclosed notes on a selected source prevent a complete paired preview",
        ));
    }
    let mut infos = Vec::new();
    let mut warnings = doc.warnings.iter().cloned().collect::<BTreeSet<_>>();
    for r in &routes {
        let voice = voice_catalog::address_at(doc, section.start, r.source).ok_or_else(|| {
            bad(format!(
                "Source {} has no explicit bank/program at section start",
                r.source
            ))
        })?;
        if voice.msb == 8 {
            return Err(bad(format!(
                "Source {} uses a MegaVoice requiring articulation-aware routed playback",
                r.source
            )));
        }
        if events.iter().any(|e| {
            e.tick > section.start
                && e.tick < section.end
                && e.channel() == Some(r.source)
                && (e.data[0] & 0xf0 == 0xc0
                    || e.data[0] & 0xf0 == 0xb0 && matches!(e.data[1], 0 | 32 | 120..=127))
        }) {
            return Err(bad(format!(
                "Source {} changes bank/program or channel mode inside the section; routed preview requires a stable part",
                r.source
            )));
        }
        if !matches!(voice.msb, 126 | 127)
            && events.iter().any(|e| {
                e.channel() == Some(r.source) && e.tick < section.end && e.data[0] & 0xf0 == 0xa0
            })
        {
            return Err(bad(format!(
                "Source {} has polyphonic aftertouch; per-note pressure conversion is not implemented",
                r.source
            )));
        }
        let drum = matches!(voice.msb, 126 | 127);
        if drum && ![9, 10].contains(&r.destination) {
            return Err(bad(
                "Factory drum routes require destination channel 9 or 10",
            ));
        }
        let notes = doc
            .notes
            .iter()
            .filter(|n| {
                n.channel == r.source
                    && n.start < section.end
                    && n.start + n.duration > section.start
            })
            .collect::<Vec<_>>();
        for n in &notes {
            for c in &config.chords {
                let _ = rule(r, n.pitch, c, drum)?;
            }
        }
        if drum
            && notes.iter().any(|n| {
                rule(r, n.pitch, &config.chords[0], drum)
                    .is_ok_and(|(r, _, _)| r.ntr != "root_fixed" || r.ntt != "bypass")
            })
        {
            return Err(bad("Drum preview needs fixed/bypass pitch rules"));
        }
        infos.push(RouteInfo {
            source: r.source,
            destination: r.destination,
            notes: notes.len(),
            drum,
            voice,
        });
    }
    let destinations = infos.iter().map(|r| r.destination).collect::<Vec<_>>();
    let mut setup = Vec::new();
    // Set normal mode for pitched parts. Drum assignment follows the kit PC below.
    for info in infos.iter().filter(|r| !r.drum) {
        setup.push(xg_parameter_change([8, info.destination - 1, 7], &[0])?);
    }
    // Stable defaults prevent a previous preview's missing CCs leaking into this one.
    for info in &infos {
        for (cc, value) in [
            (1, 0),
            (7, 100),
            (10, 64),
            (11, 127),
            (64, 0),
            (71, 64),
            (72, 64),
            (74, 64),
            (91, 40),
            (93, 0),
        ] {
            setup.push(vec![0xb0 | (info.destination - 1), cc, value]);
        }
    }
    let mut header_controls = 0;
    let mut loop_controls = setup
        .iter()
        .filter(|p| p[0] & 0xf0 == 0xb0)
        .cloned()
        .collect::<Vec<_>>();
    for e in events.iter().filter(|e| e.tick <= section.start) {
        if !e.is_on()
            && !e.is_off()
            && let Some(r) = routes.iter().find(|r| e.channel() == Some(r.source))
        {
            let mut data = e.data.clone();
            data[0] = (data[0] & 0xf0) | (r.destination - 1);
            // Chase the source-start controller state at each section wrap. PCs stay
            // startup-only so a kit change cannot erase live per-key setup.
            if !matches!(data[0] & 0xf0, 0xc0)
                && !(data[0] & 0xf0 == 0xb0 && matches!(data[1], 0 | 32))
            {
                loop_controls.push(data.clone());
            }
            setup.push(data);
            header_controls += 1;
        }
    }
    let mut drum_fields = 0;
    let parameters = if config.drums {
        native_drums::parameters(doc)
    } else {
        Ok(Vec::new())
    };
    for info in infos.iter().filter(|r| r.drum) {
        let slot = if info.destination == 9 { 1 } else { 0 };
        setup.push(xg_parameter_change(
            [8, info.destination - 1, 7],
            &[slot + 2],
        )?);
        if config.drums {
            let parameters = parameters
                .as_ref()
                .map_err(|e| bad(format!("Drum setup validation: {e}")))?;
            if !parameters.is_empty() {
                if info.source != info.destination {
                    return Err(bad(
                        "Per-key setup requires the original source and destination channel to agree; Style/Extra conversion is not inferred",
                    ));
                }
                // Reuse the preservation editor's strict header/reset/kit/unique-owner
                // validation rather than guessing ownership from the last mode packet.
                let binding = native_drums::inspect(doc, info.source)?;
                if binding.address != info.voice {
                    return Err(bad("Drum kit differs from the initial setup binding"));
                }
                for p in parameters.iter().filter(|p| p.setup == binding.setup) {
                    setup.push(xg_parameter_change(
                        [0x30 + slot, p.note, p.parameter],
                        &p.data,
                    )?);
                    drum_fields += 1;
                }
            } else {
                warnings.insert(format!(
                    "Source {} has no explicit per-key setup fields; factory kit defaults are used",
                    info.source
                ));
            }
        }
    }
    let mut effect_packets = 0;
    if config.effects {
        match style_effects::preview_setup(doc) {
            Ok(s) => {
                effect_packets = s.packets.len();
                setup.extend(s.packets);
                if s.header.reverb.late_changes || s.header.chorus.late_changes {
                    warnings.insert("Later effect messages are not replayed".into());
                }
            }
            Err(e) => {
                warnings.insert(format!("Initial effects not applied: {e}"));
            }
        }
    }
    if setup.len() > 100_000 {
        return Err(bad("Preview exceeds the 100,000 setup-message limit"));
    }
    let raw_sysex = events
        .iter()
        .filter(|e| e.sysex_payload().is_some())
        .count();
    if raw_sysex > 0 {
        warnings.insert("Raw Style DSP, insertion, master/part EQ and proprietary SysEx are not replayed; only listed typed setup is sent".into());
    }
    warnings.insert("This is a Ctb2-guided conversion study, not Yamaha-equivalent arranger playback; unmodeled CASM bytes are retained in the file".into());
    let mut packets: Vec<Packet> = Vec::new();
    // Origin/cycle ownership keeps unchanged parts sounding through a slash-only
    // change. A source wrap remains a new pattern cycle.
    let mut previous_notes: HashMap<(u64, u64), (u8, String, u64, usize)> = HashMap::new();
    let mut slides = HashMap::new();
    let mut steps = Vec::new();
    let mut position = 0;
    let mut changed = 0;
    let mut suppressed = 0;
    let mut rearticulations = 0;
    // A step boundary is not a harmonic change when its effective chord is
    // identical. Combine such spans so RTR never cuts an unchanged held note.
    let mut spans: Vec<(&Chord, u64)> = Vec::new();
    let canonical_bass = |c: &Chord| c.bass.filter(|b| *b != c.root);
    for chord in &config.chords {
        if let Some((previous, bars)) = spans.last_mut()
            && previous.root == chord.root
            && previous.quality == chord.quality
            && previous.alternate() == chord.alternate()
        {
            *bars += u64::from(chord.bars);
        } else {
            spans.push((chord, u64::from(chord.bars)));
        }
    }
    if spans.len() < config.chords.len() {
        warnings
            .insert("Adjacent identical harmonic steps are combined to preserve held notes".into());
    }
    if config
        .chords
        .iter()
        .any(|c| c.alternate() != (c.root, c.quality.as_str()))
    {
        warnings.insert(
            "Only Ctb2 regions with NTT Bass enabled use the alternate chord root or quality"
                .into(),
        );
    }
    let firmware = firmware_ntt::configured()?.is_some();
    // The firmware reader carries the fractional remainder between events.
    // Scale cumulative ticks before taking a difference at a chord change.
    let firmware_tick = |tick| u128::from(tick) * 1920 / u128::from(doc.style.ppq);
    let mut previous_roots = None;
    let mut to_root_until = 0;
    for (chord, bars) in spans {
        let roots = (chord.root, chord.alternate().0);
        if firmware && previous_roots.is_some_and(|previous| previous != roots) {
            to_root_until = firmware_tick(position) + 960;
        }
        previous_roots = Some(roots);
        let end = position + bars * per_bar;
        let before = packets.len();
        let mut cursor = position;
        while cursor < end {
            let phase = cursor % length;
            let slice_end = (cursor + (length - phase)).min(end);
            if phase == 0 {
                packets.extend(loop_controls.iter().map(|data| Packet {
                    at: at(cursor),
                    data: data.clone(),
                }));
            }
            let from = section.start + phase;
            let to = from + (slice_end - cursor);
            for (route, info) in routes.iter().zip(&infos) {
                for n in doc.notes.iter().filter(|n| {
                    n.channel == route.source && n.start < to && n.start + n.duration > from
                }) {
                    let (rule, rtr, ntt) = rule(route, n.pitch, chord, info.drum)?;
                    if !enabled(route, rule.target_root, rule.target_quality) {
                        suppressed += usize::from(n.start >= from);
                        continue;
                    }
                    let stop_on_change = rtr == 0 || (firmware && rtr == 5);
                    let fixed = info.drum
                        || (rule.ntr == "root_fixed"
                            && rule.ntt == "bypass"
                            && !(firmware && stop_on_change));
                    let (effective_root, effective_quality) = if fixed {
                        (0, "")
                    } else if rule.ntr == "guitar" && n.pitch % 12 > 1 {
                        (chord.root, chord.quality.as_str())
                    } else {
                        (rule.target_root, rule.target_quality)
                    };
                    let carried = n.start < from;
                    let start = cursor + (n.start.max(from) - from);
                    let to_root = matches!(rtr, 2 | 4) && firmware_tick(start) < to_root_until;
                    let finish = cursor + ((n.start + n.duration).min(to) - from);
                    let origin = (n.id, cursor / length);
                    if carried
                        && let Some((root, quality, last_end, off)) =
                            previous_notes.get_mut(&origin)
                        && *last_end == cursor
                        && *root == effective_root
                        && *quality == effective_quality
                        && !to_root
                    {
                        packets[*off].at = at(finish);
                        *last_end = finish;
                        continue;
                    }
                    if carried && cursor > 0 && stop_on_change {
                        suppressed += 1;
                        continue;
                    }
                    let mut pitch = if info.drum {
                        (rule.note_limit_low..=rule.note_limit_high)
                            .contains(&n.pitch)
                            .then_some(n.pitch)
                    } else {
                        converted(n.pitch, &rule, Some((ntt, route.bytes[19], chord)), to_root)?
                    };
                    if carried && cursor > 0 && !info.drum {
                        rearticulations += 1;
                        if !firmware && matches!(rtr, 1 | 2) {
                            warnings.insert("Held-note pitch-shift RTR is rearticulated at chord boundaries; smooth Yamaha pitch-shift is not reproduced".into());
                        }
                        if !firmware && matches!(rtr, 2 | 4) && rule.ntr != "guitar" {
                            let mut root_rule = PitchRule {
                                source_root: n.pitch % 12,
                                source_quality: "Major",
                                target_quality: "Major",
                                ntr: "root_fixed",
                                // Explicit to-root RTR is a conversion, unlike
                                // the literal Root Fixed + Bypass NTT pair.
                                ntt: "chord",
                                ..rule
                            };
                            let low = root_rule.note_limit_low;
                            let high = root_rule.note_limit_high;
                            if (low | high) & 128 != 0 {
                                root_rule.note_limit_low = 0;
                                root_rule.note_limit_high = 127;
                                pitch = converted(n.pitch, &root_rule, None, false)?.and_then(
                                    |pitch| apply_bypass_limits(i16::from(pitch), low, high),
                                );
                            } else {
                                pitch = converted(n.pitch, &root_rule, None, false)?;
                            }
                        }
                    }
                    let Some(pitch) = pitch else {
                        suppressed += 1;
                        continue;
                    };
                    if firmware
                        && carried
                        && !info.drum
                        && matches!(rtr, 1..=4)
                        && let Some((root, quality, last_end, off)) =
                            previous_notes.get_mut(&origin)
                        && *last_end == cursor
                    {
                        if packets[*off].data[1] == pitch {
                            packets[*off].at = at(finish);
                            *last_end = finish;
                            *root = effective_root;
                            *quality = effective_quality.to_owned();
                            rearticulations -= 1;
                            continue;
                        }
                        slides.insert(packets.len(), *off);
                    }
                    changed += usize::from(pitch != n.pitch);
                    packets.push(Packet {
                        at: at(start),
                        data: vec![0x90 | (route.destination - 1), pitch, n.velocity],
                    });
                    packets.push(Packet {
                        at: at(finish),
                        data: vec![0x80 | (route.destination - 1), pitch, 0],
                    });
                    previous_notes.insert(
                        origin,
                        (
                            effective_root,
                            effective_quality.to_owned(),
                            finish,
                            packets.len() - 1,
                        ),
                    );
                }
                // Chord masks gate notes, not the source controller timeline.
                for e in events.iter().filter(|e| {
                    e.tick >= from
                        && e.tick < to
                        && e.channel() == Some(route.source)
                        && !e.is_on()
                        && !e.is_off()
                }) {
                    // Primed startup messages, including kit PCs, must not reset per-key edits each loop.
                    if e.tick == section.start {
                        continue;
                    }
                    let mut data = e.data.clone();
                    data[0] = (data[0] & 0xf0) | (route.destination - 1);
                    packets.push(Packet {
                        at: at(cursor + e.tick - from),
                        data,
                    });
                }
            }
            cursor = slice_end;
        }
        let count = packets[before..]
            .iter()
            .filter(|p| p.data[0] & 0xf0 == 0x90)
            .count();
        steps.push(Step {
            label: format!(
                "{} {}{}",
                chord_preview::ROOTS[chord.root as usize],
                chord.quality,
                canonical_bass(chord)
                    .map(|b| format!("/{}", chord_preview::ROOTS[b as usize]))
                    .unwrap_or_default()
            ),
            start_ms: at(position).as_millis() as u64,
            end_ms: at(end).as_millis() as u64,
            notes: count,
        });
        position = end;
        if packets.len() > 500_000 {
            return Err(bad("Preview exceeds the 500,000-message limit"));
        }
    }
    let slide_offs: HashSet<_> = slides.values().copied().collect();
    let slide_sources: HashMap<_, _> = slides
        .into_iter()
        .map(|(on, off)| (on, packets[off].clone()))
        .collect();
    let mut ordered: Vec<_> = packets
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !slide_offs.contains(index))
        .collect();
    ordered.sort_by_key(|(_, p)| {
        (
            p.at,
            match p.data[0] & 0xf0 {
                0x80 => 0,
                0x90 => 2,
                _ => 1,
            },
        )
    });
    let mut active = BTreeMap::<(u8, u8), usize>::new();
    let mut coalesced = 0;
    let mut portamento = 0;
    let mut packets = Vec::with_capacity(ordered.len() + slide_sources.len() * 2);
    for (index, packet) in ordered {
        if let Some(source) = slide_sources.get(&index) {
            let channel = packet.data[0] & 15;
            let old = source.data[1];
            let new = packet.data[1];
            if active.get(&(channel, old)) == Some(&1)
                && active.get(&(channel, new)).copied().unwrap_or(0) == 0
            {
                portamento += 1;
                packets.push(Packet {
                    at: packet.at,
                    data: vec![0xb0 | channel, 84, old],
                });
                keep_packet(&packet, &mut active, &mut coalesced);
                packets.push(packet);
                keep_packet(source, &mut active, &mut coalesced);
                packets.push(source.clone());
                continue;
            }
            if keep_packet(source, &mut active, &mut coalesced) {
                packets.push(source.clone());
            }
        }
        if keep_packet(&packet, &mut active, &mut coalesced) {
            packets.push(packet);
        }
    }
    if packets.len() > 500_000 {
        return Err(bad("Preview exceeds the 500,000-message limit"));
    }
    if coalesced > 0 {
        warnings.insert(format!(
            "{coalesced} overlapping converted note-ons coalesced to prevent early note-offs"
        ));
    }
    if portamento > 0 {
        warnings.insert(format!(
            "{portamento} held-note changes use MIDI portamento control on the destination device"
        ));
    }
    if active.values().any(|v| *v != 0) {
        return Err(bad("Unbalanced converted notes"));
    }
    let ons = packets.iter().filter(|p| p.data[0] & 0xf0 == 0x90).count();
    if ons == 0 {
        return Err(bad("No enabled notes for this progression"));
    }
    let controls = packets
        .iter()
        .filter(|p| !matches!(p.data[0] & 0xf0, 0x80 | 0x90))
        .count();
    let report = Report {
        steps,
        routes: infos,
        coverage: vec![
            coverage(
                "Notes and routing",
                "applied",
                "Source notes routed to Ctb2 destinations with paired converted offs",
                ons,
            ),
            coverage(
                "CASM pitch/masks",
                "limited",
                if firmware {
                    "Firmware pitch tables, source zones, chord masks and Bass pairs. Eligible held notes use portamento control. Auto Start is outside explicit progression."
                } else {
                    "Built-in pitched conversion, source zones, chord masks and Bass pairs. Held pitch changes rearticulate. Auto Start is outside explicit progression."
                },
                changed,
            ),
            coverage(
                "Voices and controllers",
                "applied",
                "Ordered bank/program/controller baseline and in-section channel events; zero-start kit PCs prime once",
                header_controls + controls,
            ),
            coverage(
                "Drum parameters",
                "limited",
                "Factory Song-part modes plus validated explicit original per-key fields; no guessed defaults or expansion conversion",
                drum_fields,
            ),
            coverage(
                "System effects",
                "limited",
                "Known initial reverb/chorus type and Return only",
                effect_packets,
            ),
            coverage(
                "Source SysEx examined",
                "limited",
                "Source packets never replayed raw; only typed fields listed above rebuilt. DSP/insertion/EQ/proprietary/reset metadata not dispatched",
                raw_sysex,
            ),
        ],
        warnings: warnings.into_iter().collect(),
        duration_ms: at(total_ticks).as_millis() as u64,
        setup_packets: setup.len(),
        timed_packets: packets.len(),
        note_ons: ons,
        changed_pitches: changed,
        suppressed_notes: suppressed,
        boundary_rearticulations: rearticulations,
        coalesced_note_ons: coalesced,
    };
    Ok(Compiled {
        plan: Plan {
            setup,
            packets,
            duration: at(total_ticks),
            bars: vec![at(total_ticks)],
            beats: vec![at(total_ticks)],
            section: None,
            tempo: None,
            // Packets already carry their destination channels.
            routes: crate::midi::ChannelRoutes::default(),
        },
        report,
        channels: destinations,
    })
}
