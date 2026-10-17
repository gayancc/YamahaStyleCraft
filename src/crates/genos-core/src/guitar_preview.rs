//! Bounded harmonic audition for imported Guitar patterns — EV-0108
//! `yamaha_device_probe/guitar_preview.py`.
//!
//! Ordered C/F/G/A/B reference lanes (not a literal CM7 source chord).
//! Approximate harmonic audition — not recovered Yamaha string/fret tables.
//! All-purpose, stroke, and arpeggio are not emulated.

use std::collections::{BTreeMap, BTreeSet};

/// Reference pitch-classes relative to octave base (bytecode `LANES`).
pub const LANES: [u8; 5] = [0, 5, 7, 9, 11];

/// Hard audition ceiling used by ProbeService guitar preview.
pub const GUITAR_HIGH_CAP: u8 = 95;

fn clamp_span(low: u8, high: u8) -> (u8, u8) {
    let lo = low;
    let hi = high.min(GUITAR_HIGH_CAP);
    (lo, hi)
}

fn allowed_pcs(root: u8, tones: &[u8]) -> BTreeSet<u8> {
    tones
        .iter()
        .map(|t| (u16::from(*t) + u16::from(root % 12)) as u8 % 12)
        .collect()
}

fn nearest_option(note: u8, options: &[u8]) -> Option<u8> {
    options
        .iter()
        .copied()
        .min_by_key(|n| ((i16::from(*n) - i16::from(note)).unsigned_abs(), *n))
}

/// DP voicing over the five reference lanes — bytecode `voicing`.
///
/// Returns an ordered 5-note target voicing, or `None` when no path exists.
pub fn voicing(
    octave: i16,
    root: u8,
    tones: &[u8],
    low: u8,
    high: u8,
    bass: i8,
) -> Option<Vec<u8>> {
    let (lo, hi) = clamp_span(low, high);
    if lo > hi {
        return None;
    }
    let references: Vec<i16> = LANES
        .iter()
        .map(|lane| octave * 12 + i16::from(*lane))
        .collect();
    let allowed = allowed_pcs(root, tones);
    let candidates: Vec<u8> = (u16::from(lo)..=u16::from(hi))
        .map(|n| n as u8)
        .filter(|n| allowed.contains(&(n % 12)))
        .collect();
    let bass_pc = if bass >= 0 {
        (bass as u8) % 12
    } else {
        root % 12
    };
    let starts: Vec<u8> = (u16::from(lo)..=u16::from(hi))
        .map(|n| n as u8)
        .filter(|n| n % 12 == bass_pc)
        .collect();

    // states[last] = (cost, notes)
    let mut states: BTreeMap<u8, (i64, Vec<u8>)> = BTreeMap::new();
    let ref0 = references[0];
    for n in starts {
        let cost = (i64::from(n) - i64::from(ref0)).pow(2);
        states.insert(n, (cost, vec![n]));
    }

    for &reference in &references[1..] {
        let mut next_states: BTreeMap<u8, (i64, Vec<u8>)> = BTreeMap::new();
        for &n in &candidates {
            let mut prior: Vec<(i64, Vec<u8>)> = states
                .iter()
                .filter(|(last, _)| **last < n)
                .map(|(_, v)| v.clone())
                .collect();
            if prior.is_empty() {
                continue;
            }
            prior.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            let (cost, notes) = prior.into_iter().next().unwrap();
            let mut new_notes = notes;
            new_notes.push(n);
            let new_cost = cost + (i64::from(n) - i64::from(reference)).pow(2);
            next_states.insert(n, (new_cost, new_notes));
        }
        states = next_states;
    }

    if states.is_empty() {
        return None;
    }
    let mut best: Option<(i64, Vec<u8>)> = None;
    for (cost, notes) in states.into_values() {
        match &best {
            None => best = Some((cost, notes)),
            Some((bc, bn)) if cost < *bc || (cost == *bc && notes < *bn) => {
                best = Some((cost, notes));
            }
            _ => {}
        }
    }
    best.map(|(_, notes)| notes)
}

/// `guitar_preview.pitch` — map one source note into the audition voicing.
pub fn pitch(note: u8, root: u8, tones: &[u8], low: u8, high: u8, bass: i8) -> Option<u8> {
    let octave = i16::from(note) / 12;
    let pc = note % 12;
    let (lo, hi) = clamp_span(low, high);

    if !LANES.contains(&pc) {
        let allowed = allowed_pcs(root, tones);
        let options: Vec<u8> = (u16::from(lo)..=u16::from(hi))
            .map(|n| n as u8)
            .filter(|n| allowed.contains(&(n % 12)))
            .collect();
        return nearest_option(note, &options);
    }

    let mapped = voicing(octave, root, tones, low, high, -1)?;
    if mapped.is_empty() {
        return None;
    }

    // Bass lane (C / LANES[0]): optional bass substitution below second voice.
    if pc == LANES[0] && bass >= 0 && mapped.len() >= 2 {
        let upper = mapped[1].saturating_sub(1);
        let bass_hi = upper.min(hi).min(GUITAR_HIGH_CAP);
        let bass_pc = (bass as u8) % 12;
        let choices: Vec<u8> = (u16::from(lo)..=u16::from(bass_hi))
            .map(|n| n as u8)
            .filter(|n| n % 12 == bass_pc)
            .collect();
        if let Some(choice) = nearest_option(mapped[0], &choices) {
            return Some(choice);
        }
        return Some(mapped[0]);
    }

    let lane_index = LANES.iter().position(|&l| l == pc)?;
    if let Some(&out) = mapped.get(lane_index) {
        return Some(out);
    }

    let allowed = allowed_pcs(root, tones);
    let options: Vec<u8> = (u16::from(lo)..=u16::from(hi))
        .map(|n| n as u8)
        .filter(|n| allowed.contains(&(n % 12)))
        .collect();
    nearest_option(note, &options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_are_c_f_g_a_b() {
        assert_eq!(LANES, [0, 5, 7, 9, 11]);
    }

    #[test]
    fn voicing_major_c_returns_five_ordered_notes() {
        let v = voicing(5, 0, &[0, 4, 7], 0, 95, -1).unwrap();
        assert_eq!(v.len(), 5);
        for w in v.windows(2) {
            assert!(w[0] < w[1]);
        }
        // All pcs in C major triad
        for n in &v {
            assert!([0u8, 4, 7].contains(&(n % 12)));
        }
    }

    #[test]
    fn pitch_lane_note_maps_into_voicing() {
        // C5 (60) is lane 0 in octave 5
        let out = pitch(60, 0, &[0, 4, 7], 0, 95, -1).unwrap();
        assert!((0..=95).contains(&out));
        assert_eq!(out % 12, 0); // stays on C lane without bass override
    }

    #[test]
    fn pitch_non_lane_snaps_to_nearest_allowed() {
        // D4 (62) pc=2 not in LANES → nearest C/E/G in range
        let out = pitch(62, 0, &[0, 4, 7], 0, 95, -1).unwrap();
        assert!([0u8, 4, 7].contains(&(out % 12)));
        assert!((i16::from(out) - 62).unsigned_abs() <= 2);
    }

    #[test]
    fn pitch_with_bass_on_root_lane_can_substitute() {
        // bass = F (5): when mapping C-lane note, may place F below second voice
        let out = pitch(60, 0, &[0, 4, 7], 0, 95, 5).unwrap();
        assert!((0..=95).contains(&out));
        let pc = out % 12;
        assert!(pc == 5 || pc == 0);
    }
}
