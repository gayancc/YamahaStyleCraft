//! Non-Yamaha basic chord preview helpers recovered from EV-0108
//! `yamaha_device_probe/chord_preview.py` bytecode.
//!
//! ProbeService docstring (module const 0): this is **not** an emulator of
//! Yamaha's arranger. Supported subset maps chord tones / scale degrees;
//! unsupported rules stay silent in converted preview only. Source mode is
//! literal. No project/export mutation.

use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Chromatic root names (bytecode `ROOTS`).
pub const ROOTS: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// Supported preview qualities (bytecode `SCALES` / `TONES` keys).
pub const QUALITIES: [&str; 10] = [
    "Major", "Minor", "7", "Maj7", "m7", "dim", "aug", "sus4", "sus2", "m7b5",
];

fn scale_degrees(quality: &str) -> Option<&'static [u8]> {
    Some(match quality {
        "Major" | "Maj7" => &[0, 2, 4, 5, 7, 9, 11],
        "Minor" | "m7" => &[0, 2, 3, 5, 7, 8, 10],
        "7" => &[0, 2, 4, 5, 7, 9, 10],
        "dim" => &[0, 2, 3, 5, 6, 8, 9],
        "aug" => &[0, 2, 4, 6, 8, 9, 11],
        "sus4" => &[0, 2, 5, 5, 7, 9, 10],
        "sus2" => &[0, 2, 2, 5, 7, 9, 10],
        "m7b5" => &[0, 1, 3, 5, 6, 8, 10],
        _ => return None,
    })
}

fn tone_degrees(quality: &str) -> Option<&'static [u8]> {
    Some(match quality {
        "Major" => &[0, 4, 7],
        "Minor" => &[0, 3, 7],
        "7" => &[0, 4, 7, 10],
        "Maj7" => &[0, 4, 7, 11],
        "m7" => &[0, 3, 7, 10],
        "dim" => &[0, 3, 6],
        "aug" => &[0, 4, 8],
        "sus4" => &[0, 5, 7],
        "sus2" => &[0, 2, 7],
        "m7b5" => &[0, 3, 6, 10],
        _ => return None,
    })
}

/// Display aliases (bytecode `ALIASES`).
pub fn alias(quality: &str) -> Option<&'static str> {
    match quality {
        "Maj7" => Some("Major 7"),
        "m7" => Some("Minor 7"),
        "7" => Some("Dominant 7"),
        _ => None,
    }
}

pub fn root_number(name: &str) -> Option<u8> {
    ROOTS.iter().position(|r| *r == name).map(|i| i as u8)
}

pub fn root_name(n: u8) -> Option<&'static str> {
    ROOTS.get((n % 12) as usize).copied()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewConfig {
    pub mode: String,
    pub quality: String,
    pub root: u8,
    pub bass: i8,
    pub channel: u8,
    pub split: u8,
}

/// `validate_config` — modes `source`|`manual`|`midi`; quality must be in SCALES.
pub fn validate_config(
    mode: Option<&str>,
    quality: Option<&str>,
    root: Option<u8>,
    bass: Option<i8>,
    channel: Option<u8>,
    split: Option<u8>,
) -> Result<PreviewConfig> {
    let mode = mode.unwrap_or("source");
    if !matches!(mode, "source" | "manual" | "midi") {
        return Err(Error::Unsupported("Invalid chord preview mode".into()));
    }
    let quality = quality.unwrap_or("Major");
    if scale_degrees(quality).is_none() {
        return Err(Error::Unsupported("Unsupported preview chord".into()));
    }
    let root = clamp_field("root", root.unwrap_or(0), 0, 11)?;
    let bass = clamp_signed("bass", bass.unwrap_or(-1), -1, 11)?;
    let channel = clamp_field("channel", channel.unwrap_or(1), 1, 16)?;
    let split = clamp_field("split", split.unwrap_or(59), 0, 127)?;
    Ok(PreviewConfig {
        mode: mode.into(),
        quality: quality.into(),
        root,
        bass,
        channel,
        split,
    })
}

fn clamp_field(label: &str, value: u8, low: u8, high: u8) -> Result<u8> {
    if value < low || value > high {
        return Err(Error::Unsupported(format!("Invalid chord preview {label}")));
    }
    Ok(value)
}

fn clamp_signed(label: &str, value: i8, low: i8, high: i8) -> Result<i8> {
    if value < low || value > high {
        return Err(Error::Unsupported(format!("Invalid chord preview {label}")));
    }
    Ok(value)
}

/// `transpose_degree(pc, source_quality, target_quality, chordal)`.
/// Returns `12` when the tone degree has no target slot (silent in preview).
pub fn transpose_degree(
    pc: u8,
    mut source_quality: &str,
    target_quality: &str,
    chordal: bool,
) -> Result<u8> {
    let pc = pc % 12;
    if source_quality == "MajorTriad" {
        source_quality = "Major";
    }
    if source_quality == "m7(11)" {
        if chordal && pc == 5 {
            let tones = tone_degrees(target_quality)
                .ok_or_else(|| Error::Unsupported("Unsupported preview chord".into()))?;
            return Ok(*tones.get(2).unwrap_or(&12));
        }
        source_quality = "m7";
    }
    if source_quality == target_quality {
        return Ok(pc);
    }
    let old_tones = tone_degrees(source_quality)
        .ok_or_else(|| Error::Unsupported("Unsupported preview chord".into()))?;
    let new_tones = tone_degrees(target_quality)
        .ok_or_else(|| Error::Unsupported("Unsupported preview chord".into()))?;
    if let Some(degree) = old_tones.iter().position(|&t| t == pc) {
        return Ok(if degree < new_tones.len() {
            new_tones[degree]
        } else {
            12
        });
    }
    let old = scale_degrees(source_quality)
        .ok_or_else(|| Error::Unsupported("Unsupported preview chord".into()))?;
    let new = scale_degrees(target_quality)
        .ok_or_else(|| Error::Unsupported("Unsupported preview chord".into()))?;
    if let Some(degree) = old.iter().position(|&t| t == pc) {
        return Ok(new[degree]);
    }
    Ok(pc)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizedChord {
    pub root: u8,
    pub quality: String,
}

/// `recognize(notes)` — match pitch-class set against `TONES` under each root.
pub fn recognize(notes: &[u8], previous: Option<&RecognizedChord>) -> Option<RecognizedChord> {
    if notes.is_empty() {
        return None;
    }
    let pcs: BTreeSet<u8> = notes.iter().map(|n| n % 12).collect();
    let bass = notes.iter().copied().min().unwrap() % 12;
    let mut candidates = Vec::new();
    for root in 0u8..12 {
        for quality in QUALITIES {
            let tones = tone_degrees(quality)?;
            let set: BTreeSet<u8> = tones.iter().map(|t| (t + root) % 12).collect();
            if set == pcs {
                candidates.push(RecognizedChord {
                    root,
                    quality: quality.into(),
                });
            }
        }
    }
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by_key(|c| {
        let bass_penalty = if c.root == bass { 0u8 } else { 1 };
        let prev_penalty = match previous {
            Some(p) if p.root == c.root && p.quality == c.quality => 0u8,
            Some(_) => 1,
            None => 0,
        };
        (bass_penalty, prev_penalty, c.root, c.quality.clone())
    });
    candidates.into_iter().next()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChordInspect {
    pub root: u8,
    pub root_name: String,
    pub quality: String,
    pub alias: Option<String>,
    pub tones: Vec<u8>,
    pub scale: Vec<u8>,
    pub absolute_tones: Vec<u8>,
    pub disclaimer: String,
}

pub fn inspect(root: u8, quality: &str) -> Result<ChordInspect> {
    let root = root % 12;
    let tones = tone_degrees(quality)
        .ok_or_else(|| Error::Unsupported("Unsupported preview chord".into()))?
        .to_vec();
    let scale = scale_degrees(quality)
        .ok_or_else(|| Error::Unsupported("Unsupported preview chord".into()))?
        .to_vec();
    Ok(ChordInspect {
        root,
        root_name: root_name(root).unwrap_or("?").into(),
        quality: quality.into(),
        alias: alias(quality).map(str::to_string),
        absolute_tones: tones.iter().map(|t| (t + root) % 12).collect(),
        tones,
        scale,
        disclaimer: "Non-destructive basic chord preview — not a Yamaha arranger emulator (EV-0108 chord_preview.py)."
            .into(),
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemappedNote {
    pub source: u8,
    pub preview: Option<u8>,
    pub silent: bool,
}

/// CASM-guided pitch inputs recovered from `ChordPreview.pitch`.
#[derive(Clone, Debug)]
pub struct PitchRule<'a> {
    pub source_root: u8,
    pub source_quality: &'a str,
    pub target_root: u8,
    pub target_quality: &'a str,
    /// `root_trans` (default) or `root_fixed`.
    pub ntr: &'a str,
    /// `bypass` | `chord` | `melody` | `melodic_minor` | …
    pub ntt: &'a str,
    /// Optional high-key pitch-class cap (root_trans wrap); `None` skips.
    pub high_key: Option<u8>,
    pub note_limit_low: u8,
    pub note_limit_high: u8,
    /// `pitched` (default) or `guitar` (EV-0108 guitar_preview).
    pub kind: &'a str,
    /// Optional bass pitch-class for guitar audition (`-1` = use root).
    pub bass: i8,
    /// When set, apply expansion layout: 0..=95 guitar, 96..=127 fixed.
    pub expansion_layout: bool,
}

impl Default for PitchRule<'static> {
    fn default() -> Self {
        Self {
            source_root: 0,
            source_quality: "Major",
            target_root: 0,
            target_quality: "Major",
            ntr: "root_trans",
            ntt: "chord",
            high_key: Some(11), // 'B'
            note_limit_low: 0,
            note_limit_high: 127,
            kind: "pitched",
            bass: -1,
            expansion_layout: false,
        }
    }
}

/// Manual-mode pitch remap (NTT chord, NTR root_trans).
pub fn remap_notes(
    notes: &[u8],
    source_root: u8,
    source_quality: &str,
    target_root: u8,
    target_quality: &str,
) -> Result<Vec<RemappedNote>> {
    remap_notes_with_rule(
        notes,
        &PitchRule {
            source_root,
            source_quality,
            target_root,
            target_quality,
            ntr: "root_trans",
            ntt: "chord",
            ..PitchRule::default()
        },
    )
}

/// NTT-only helper (root_trans). Prefer `remap_notes_with_rule` for NTR.
pub fn remap_notes_with_ntt(
    notes: &[u8],
    source_root: u8,
    source_quality: &str,
    target_root: u8,
    target_quality: &str,
    ntt: &str,
) -> Result<Vec<RemappedNote>> {
    remap_notes_with_rule(
        notes,
        &PitchRule {
            source_root,
            source_quality,
            target_root,
            target_quality,
            ntr: "root_trans",
            ntt,
            ..PitchRule::default()
        },
    )
}

pub fn remap_notes_with_rule(notes: &[u8], rule: &PitchRule<'_>) -> Result<Vec<RemappedNote>> {
    validate_rule(rule)?;
    if notes.len() > 128 {
        return Err(Error::Unsupported(
            "Preview is limited to 128 pitches".into(),
        ));
    }
    let mut out = Vec::with_capacity(notes.len());
    for &note in notes {
        match preview_pitch(note, rule)? {
            Some(preview) => out.push(RemappedNote {
                source: note,
                preview: Some(preview),
                silent: false,
            }),
            None => out.push(RemappedNote {
                source: note,
                preview: None,
                silent: true,
            }),
        }
    }
    Ok(out)
}

/// `ChordPreview.pitch` dispatcher — pitched, guitar, or expansion layout.
/// Returns `None` when converted preview is silent / out of range.
pub fn preview_pitch(note: u8, rule: &PitchRule<'_>) -> Result<Option<u8>> {
    validate_rule(rule)?;
    if note > 127 {
        return Err(Error::Unsupported("Preview pitch must be 0..127".into()));
    }
    if rule.expansion_layout {
        return preview_expansion_layout(note, rule);
    }
    if rule.kind == "guitar" {
        return Ok(preview_guitar(note, rule));
    }
    preview_pitched(note, rule)
}

fn validate_rule(rule: &PitchRule<'_>) -> Result<()> {
    let source = match rule.source_quality {
        "MajorTriad" => "Major",
        "m7(11)" => "m7",
        q => q,
    };
    if rule.source_root > 11
        || rule.target_root > 11
        || !(-1..=11).contains(&rule.bass)
        || rule.high_key.is_some_and(|k| k > 11)
        || rule.note_limit_low > rule.note_limit_high
        || rule.note_limit_high > 127
        || !matches!(rule.kind, "pitched" | "guitar")
        || !matches!(rule.ntr, "root_trans" | "root_fixed")
        || !matches!(rule.ntt, "bypass" | "chord" | "melody" | "melodic_minor")
        || tone_degrees(source).is_none()
        || tone_degrees(rule.target_quality).is_none()
    {
        return Err(Error::Unsupported(
            "Unsupported or invalid preview rule".into(),
        ));
    }
    Ok(())
}

fn preview_guitar(note: u8, rule: &PitchRule<'_>) -> Option<u8> {
    let tones = tone_degrees(rule.target_quality)?;
    crate::guitar_preview::pitch(
        note,
        rule.target_root % 12,
        tones,
        rule.note_limit_low,
        rule.note_limit_high,
        rule.bass,
    )
}

fn preview_expansion_layout(note: u8, rule: &PitchRule<'_>) -> Result<Option<u8>> {
    match crate::expansion_guitar::expansion_note_band(note) {
        crate::expansion_guitar::ExpansionNoteBand::Fixed => Ok(Some(note)),
        crate::expansion_guitar::ExpansionNoteBand::Pitched => {
            let mut guitar_rule = rule.clone();
            guitar_rule.kind = "guitar";
            guitar_rule.expansion_layout = false;
            guitar_rule.note_limit_high = guitar_rule
                .note_limit_high
                .min(crate::expansion_guitar::CASM_GUIDED_NOTE_HIGH);
            Ok(preview_guitar(note, &guitar_rule))
        }
    }
}

/// `ChordPreview.pitch` for pitched (non-drum/fixed/guitar) notes — EV-0108.
fn preview_pitched(note: u8, rule: &PitchRule<'_>) -> Result<Option<u8>> {
    let source = (rule.source_root % 12) as i16;
    let target = (rule.target_root % 12) as i16;
    let source_quality = if rule.source_quality == "MajorTriad" {
        "Major"
    } else {
        rule.source_quality
    };
    let target_quality = rule.target_quality;
    tone_degrees(if source_quality == "m7(11)" {
        "m7"
    } else {
        source_quality
    })
    .ok_or_else(|| Error::Unsupported("Unsupported preview chord".into()))?;
    tone_degrees(target_quality)
        .ok_or_else(|| Error::Unsupported("Unsupported preview chord".into()))?;

    // Identity shortcut from pitch(): same root and (same quality or bypass).
    if target == source && (source_quality == target_quality || rule.ntt == "bypass") {
        return Ok(Some(note));
    }

    let diff = note as i16 - source;
    let octave = diff.div_euclid(12);
    let pc = diff.rem_euclid(12) as u8;
    let mut mapped = pc;

    if rule.ntt == "melodic_minor" {
        let minor_sources = ["Minor", "m7", "m7(11)"];
        let minor_targets = ["Minor", "m7", "dim", "m7b5"];
        let major_targets = ["Major", "Maj7", "7", "aug"];
        if !minor_sources.contains(&source_quality)
            && minor_targets.contains(&target_quality)
            && pc == 4
        {
            mapped = 3;
        } else if minor_sources.contains(&source_quality)
            && major_targets.contains(&target_quality)
            && pc == 3
        {
            mapped = 4;
        }
        // else leave mapped=pc (no transpose_degree on melodic_minor path)
    } else if rule.ntt != "bypass" {
        mapped = transpose_degree(pc, source_quality, target_quality, rule.ntt == "chord")?;
    }
    // bypass with different roots: mapped stays pc (root shift only)

    if mapped >= 12 {
        return Ok(None);
    }

    let mut result = octave * 12 + target + mapped as i16;

    if rule.ntr == "root_fixed" {
        // result += round((note - result) / 12) * 12
        let adjust = ((note as f64 - result as f64) / 12.0).round_ties_even() as i16;
        result += adjust * 12;
    } else if let Some(high_key) = rule.high_key {
        // root_trans: if target > highKey, drop one octave
        if target > high_key as i16 {
            result -= 12;
        }
    }

    let low = i16::from(rule.note_limit_low).max(0);
    let high = i16::from(rule.note_limit_high).min(127);

    let mut candidates = Vec::new();
    for i in -12i16..=12 {
        let cand = result + i * 12;
        if cand >= low && cand <= high {
            candidates.push(cand as u8);
        }
    }
    if candidates.is_empty() {
        return Ok(None);
    }
    // min(candidates, key=abs(c - result))
    let best = *candidates
        .iter()
        .min_by_key(|&&c| (c as i16 - result).unsigned_abs())
        .unwrap();
    Ok(Some(best))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vocabulary {
    pub roots: Vec<String>,
    pub qualities: Vec<String>,
    pub tones: BTreeMap<String, Vec<u8>>,
    pub disclaimer: String,
}

pub fn vocabulary() -> Vocabulary {
    let mut tones = BTreeMap::new();
    for q in QUALITIES {
        if let Some(t) = tone_degrees(q) {
            tones.insert(q.into(), t.to_vec());
        }
    }
    Vocabulary {
        roots: ROOTS.iter().map(|s| (*s).into()).collect(),
        qualities: QUALITIES.iter().map(|s| (*s).into()).collect(),
        tones,
        disclaimer:
            "EV-0108 chord_preview.py SCALES/TONES — not Yamaha proprietary fingering tables."
                .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_fixed_uses_python_even_ties_in_both_directions() {
        let rule = PitchRule {
            target_root: 6,
            ntr: "root_fixed",
            ntt: "bypass",
            ..PitchRule::default()
        };
        assert_eq!(preview_pitch(60, &rule).unwrap(), Some(66));
        let reverse = PitchRule {
            source_root: 6,
            target_root: 0,
            ..rule
        };
        assert_eq!(preview_pitch(66, &reverse).unwrap(), Some(60));
    }

    #[test]
    fn m7_eleven_source_maps_special_fourth_without_rejecting_alias() {
        let rule = PitchRule {
            source_quality: "m7(11)",
            target_quality: "Major",
            ..PitchRule::default()
        };
        assert_eq!(preview_pitch(65, &rule).unwrap(), Some(67));
    }

    #[test]
    fn invalid_rules_reject_even_on_identity_empty_and_fixed_paths() {
        let bad = PitchRule {
            ntt: "unknown",
            ..PitchRule::default()
        };
        assert!(remap_notes_with_rule(&[], &bad).is_err());
        assert!(preview_pitch(60, &bad).is_err());
        let fixed = PitchRule {
            expansion_layout: true,
            ..PitchRule::default()
        };
        assert!(preview_pitch(200, &fixed).is_err());
        assert!(remap_notes_with_rule(&[60; 129], &fixed).is_err());
        assert!(
            preview_pitch(
                60,
                &PitchRule {
                    target_root: 12,
                    ..fixed
                }
            )
            .is_err()
        );
        assert!(
            preview_pitch(
                60,
                &PitchRule {
                    note_limit_low: 70,
                    note_limit_high: 60,
                    ..PitchRule::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn major_to_minor_third_maps_by_tone_degree() {
        // E relative to C (pc 4) is Major tone index 1 → Minor tone 3
        assert_eq!(transpose_degree(4, "Major", "Minor", true).unwrap(), 3);
    }

    #[test]
    fn equal_quality_is_identity() {
        assert_eq!(transpose_degree(7, "Major", "Major", true).unwrap(), 7);
    }

    #[test]
    fn recognize_c_major_triad() {
        let hit = recognize(&[60, 64, 67], None).unwrap();
        assert_eq!(hit.root, 0);
        assert_eq!(hit.quality, "Major");
    }

    #[test]
    fn remap_c_major_notes_to_d_minor() {
        let notes = remap_notes(&[60, 64, 67], 0, "Major", 2, "Minor").unwrap();
        assert_eq!(
            notes.iter().map(|n| n.preview.unwrap()).collect::<Vec<_>>(),
            vec![62, 65, 69]
        );
    }

    #[test]
    fn validate_rejects_unknown_quality() {
        assert!(validate_config(None, Some("Phrygian"), None, None, None, None).is_err());
    }

    #[test]
    fn bypass_same_root_is_literal() {
        let notes = remap_notes_with_ntt(&[60, 64], 0, "Major", 0, "Minor", "bypass").unwrap();
        assert_eq!(notes[0].preview, Some(60));
        assert_eq!(notes[1].preview, Some(64));
    }

    #[test]
    fn bypass_different_root_shifts_only() {
        // pc preserved; C4 → D4 under bypass C→D
        let notes = remap_notes_with_ntt(&[60], 0, "Major", 2, "Major", "bypass").unwrap();
        assert_eq!(notes[0].preview, Some(62));
    }

    #[test]
    fn root_fixed_keeps_source_octave_register() {
        let rule = PitchRule {
            source_root: 0,
            source_quality: "Major",
            target_root: 7, // G
            target_quality: "Major",
            ntr: "root_fixed",
            ntt: "bypass",
            high_key: Some(11),
            note_limit_low: 0,
            note_limit_high: 127,
            ..PitchRule::default()
        };
        // C4 (60) → root shift would be 67 before fixed; fixed snaps near source register
        let out = preview_pitch(60, &rule).unwrap().unwrap();
        assert!((out as i16 - 60).unsigned_abs() <= 6, "got {out}");
    }

    #[test]
    fn high_key_wrap_drops_octave_when_target_above_cap() {
        let rule = PitchRule {
            source_root: 0,
            source_quality: "Major",
            target_root: 11, // B
            target_quality: "Major",
            ntr: "root_trans",
            ntt: "bypass",
            high_key: Some(10), // Bb — target B > high → −12
            note_limit_low: 0,
            note_limit_high: 127,
            ..PitchRule::default()
        };
        // Without wrap: 60→71; with wrap: 59
        assert_eq!(preview_pitch(60, &rule).unwrap(), Some(59));
    }

    #[test]
    fn guitar_kind_uses_lane_voicing() {
        let rule = PitchRule {
            target_root: 0,
            target_quality: "Major",
            kind: "guitar",
            note_limit_low: 0,
            note_limit_high: 95,
            ..PitchRule::default()
        };
        let out = preview_pitch(60, &rule).unwrap().unwrap();
        assert!((0..=95).contains(&out));
    }

    #[test]
    fn expansion_layout_keeps_fixed_band_literal() {
        let rule = PitchRule {
            target_root: 0,
            target_quality: "Major",
            expansion_layout: true,
            ..PitchRule::default()
        };
        assert_eq!(preview_pitch(100, &rule).unwrap(), Some(100));
    }
}
