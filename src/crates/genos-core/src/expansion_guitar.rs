//! Expansion Guitar preview profiles — EV-0108 `chord_preview.py` constants
//! and `validate_config` / layout helpers (not the full voice-compatibility
//! prepare path).

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Bytecode `EXPANSION_GUITAR_LAYOUT`.
pub const EXPANSION_GUITAR_LAYOUT: &str = "pitched-0-95-fixed-96-127";

/// Bytecode `EXPANSION_GUITAR_REASON`.
pub const EXPANSION_GUITAR_REASON: &str = "User-selected expansion Guitar preview (approximate)";

/// Note-limit high cap when CASM guided / open-bass (non-mega) path applies.
pub const CASM_GUIDED_NOTE_HIGH: u8 = 95;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceAddress {
    pub msb: u8,
    pub lsb: u8,
    pub program: u8, // 1..=128
}

impl VoiceAddress {
    pub fn as_tuple(self) -> (u8, u8, u8) {
        (self.msb, self.lsb, self.program)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpansionGuitarProfile {
    pub address: [u8; 3],
    pub layout: String,
}

/// MEGA voice → (noteLimitLow, noteLimitHigh) — bytecode `MEGA_PROFILES`.
pub fn mega_profiles() -> BTreeMap<(u8, u8, u8), (u8, u8)> {
    let mut map = BTreeMap::new();
    for &(lsb, pc) in &[
        (0u8, 1u8),
        (0, 2),
        (0, 4),
        (1, 4),
        (2, 4),
        (3, 4),
        (4, 4),
        (0, 7),
    ] {
        map.insert((8, lsb, pc), (61, 75));
    }
    for &(lsb, pc) in &[
        (0u8, 17u8),
        (0, 18),
        (1, 18),
        (2, 18),
        (0, 19),
        (1, 19),
        (0, 20),
    ] {
        map.insert((8, lsb, pc), (81, 120));
    }
    map.insert((8, 8, 4), (61, 75));
    map.insert((8, 1, 2), (61, 75));
    map.insert((8, 11, 2), (61, 75));
    map
}

pub fn pitched_bank8() -> BTreeSet<(u8, u8, u8)> {
    [
        (8, 37, 2),
        (8, 42, 4),
        (8, 32, 18),
        (8, 33, 18),
        (8, 53, 4),
        (8, 35, 50),
        (8, 44, 21),
        (8, 32, 114),
    ]
    .into_iter()
    .collect()
}

pub fn casm_open_bass() -> BTreeSet<(u8, u8, u8)> {
    [(8, 3, 19), (8, 4, 19)].into_iter().collect()
}

pub fn casm_guided_voices() -> BTreeSet<(u8, u8, u8)> {
    [(8, 1, 57), (8, 0, 21), (8, 5, 50), (8, 5, 18)]
        .into_iter()
        .collect()
}

/// Cap note-limit high for CASM guided / open-bass when not a mega profile.
pub fn apply_casm_guided_high_cap(address: (u8, u8, u8), note_limit_high: u8) -> u8 {
    let mega = mega_profiles();
    if mega.contains_key(&address) {
        return note_limit_high;
    }
    let open = casm_open_bass();
    let guided = casm_guided_voices();
    if open.contains(&address) || guided.contains(&address) {
        note_limit_high.min(CASM_GUIDED_NOTE_HIGH)
    } else {
        note_limit_high
    }
}

/// Validate `expansionGuitarProfiles` from configureChordPreview.
pub fn validate_expansion_guitar_profiles(
    profiles: &[ExpansionGuitarProfile],
) -> Result<Vec<ExpansionGuitarProfile>> {
    if profiles.len() > 16 {
        return Err(Error::Unsupported(
            "Invalid expansion Guitar preview profiles".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    for profile in profiles {
        if profile.layout != EXPANSION_GUITAR_LAYOUT {
            return Err(Error::Unsupported(
                "Invalid expansion Guitar preview profile".into(),
            ));
        }
        let address = profile.address;
        // all ints; first two 0..=127; program 1..=128; unique
        if address[0] > 127 || address[1] > 127 || !(1..=128).contains(&address[2]) {
            return Err(Error::Unsupported(
                "Invalid expansion Guitar preview address".into(),
            ));
        }
        let key = (address[0], address[1], address[2]);
        if !seen.insert(key) {
            return Err(Error::Unsupported(
                "Invalid expansion Guitar preview address".into(),
            ));
        }
        selected.push(ExpansionGuitarProfile {
            address,
            layout: EXPANSION_GUITAR_LAYOUT.into(),
        });
    }
    Ok(selected)
}

/// Layout split: pitched band 0..=95 vs fixed/literal 96..=127.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpansionNoteBand {
    Pitched,
    Fixed,
}

pub fn expansion_note_band(note: u8) -> ExpansionNoteBand {
    if note <= 95 {
        ExpansionNoteBand::Pitched
    } else {
        ExpansionNoteBand::Fixed
    }
}

/// Parse voice address from (msb, lsb, program 1..=128) — bytecode `voice_address` shape.
pub fn parse_voice_address(msb: u8, lsb: u8, program: u8) -> Option<VoiceAddress> {
    if msb > 127 || lsb > 127 || !(1..=128).contains(&program) {
        return None;
    }
    Some(VoiceAddress { msb, lsb, program })
}

/// Expansion candidate rejects factory bank 8 / GM drums banks (bytecode gate).
pub fn expansion_msb_allowed(msb: u8) -> bool {
    !matches!(msb, 8 | 126 | 127)
}

/// Control-change row used by tick-zero / future-stable helpers.
#[derive(Clone, Debug)]
pub struct PartControlChange {
    pub tick: i32,
    pub controller: u8,
    pub value: u8,
}

/// Program-change row used by tick-zero / future-stable helpers.
#[derive(Clone, Debug)]
pub struct PartProgramChange {
    pub tick: i32,
    pub program: u8, // 1..=128
}

/// Bytecode `_tick_zero_voice_converges` (CC0/32 + PC at tick 0 only).
pub fn tick_zero_voice_converges(
    initial: VoiceAddress,
    controls: &[PartControlChange],
    programs: &[PartProgramChange],
) -> bool {
    let mut pending = [initial.msb, initial.lsb, initial.program];
    let mut sounding = pending;
    for event in controls {
        if event.tick != 0 || !matches!(event.controller, 0 | 32) {
            continue;
        }
        if event.value > 127 {
            return false;
        }
        let idx = if event.controller == 0 { 0 } else { 1 };
        pending[idx] = event.value;
    }
    for event in programs {
        if event.tick != 0 {
            continue;
        }
        if !(1..=128).contains(&event.program) {
            return false;
        }
        pending[2] = event.program;
        sounding = pending;
    }
    let initial_t = [initial.msb, initial.lsb, initial.program];
    pending == initial_t && sounding == initial_t
}

/// Bytecode `_expansion_guitar_future_voice_stable`.
pub fn expansion_guitar_future_voice_stable(
    initial: VoiceAddress,
    controls: &[PartControlChange],
    programs: &[PartProgramChange],
) -> bool {
    for event in controls {
        if event.tick <= 0 || !matches!(event.controller, 0 | 32) {
            continue;
        }
        let expected = if event.controller == 0 {
            initial.msb
        } else {
            initial.lsb
        };
        if event.value != expected {
            return false;
        }
    }
    for event in programs {
        if event.tick <= 0 {
            continue;
        }
        if event.program != initial.program {
            return false;
        }
    }
    true
}

/// Phase-1 expansion candidate gates that do not need CASM routes / catalogs.
///
/// Caller supplies resolved `factory_known`, `shared_source`, and `changing_voice`
/// (full `expansion_guitar_candidate` still needs `validated_routes` / `rule_for_note`).
#[derive(Clone, Debug)]
pub struct ExpansionCandidateGates {
    pub address: VoiceAddress,
    pub is_drum_kit: bool,
    pub factory_known: bool,
    pub imported_from_casm: bool,
    pub casm_edited: bool,
    pub shared_source_voice: bool,
    pub changing_voice: bool,
    pub tick_zero_converges: bool,
    pub future_voice_stable: bool,
}

/// Returns a profile when phase-1 gates pass; `None` otherwise.
pub fn expansion_guitar_candidate_phase1(
    gates: &ExpansionCandidateGates,
) -> Option<ExpansionGuitarProfile> {
    if !expansion_msb_allowed(gates.address.msb) {
        return None;
    }
    if gates.is_drum_kit || gates.factory_known {
        return None;
    }
    if !gates.imported_from_casm || gates.casm_edited {
        return None;
    }
    if !gates.shared_source_voice || gates.changing_voice {
        return None;
    }
    if !gates.tick_zero_converges || !gates.future_voice_stable {
        return None;
    }
    Some(ExpansionGuitarProfile {
        address: [gates.address.msb, gates.address.lsb, gates.address.program],
        layout: EXPANSION_GUITAR_LAYOUT.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mega_profiles_cover_bytecode_keys() {
        let m = mega_profiles();
        assert_eq!(m.get(&(8, 0, 1)), Some(&(61, 75)));
        assert_eq!(m.get(&(8, 0, 17)), Some(&(81, 120)));
        assert_eq!(m.get(&(8, 8, 4)), Some(&(61, 75)));
    }

    #[test]
    fn validate_accepts_unique_layout_profiles() {
        let ok = validate_expansion_guitar_profiles(&[ExpansionGuitarProfile {
            address: [8, 1, 2],
            layout: EXPANSION_GUITAR_LAYOUT.into(),
        }])
        .unwrap();
        assert_eq!(ok.len(), 1);
    }

    #[test]
    fn validate_rejects_bad_layout_and_dupes() {
        assert!(
            validate_expansion_guitar_profiles(&[ExpansionGuitarProfile {
                address: [8, 1, 2],
                layout: "other".into(),
            }])
            .is_err()
        );
        assert!(
            validate_expansion_guitar_profiles(&[
                ExpansionGuitarProfile {
                    address: [8, 1, 2],
                    layout: EXPANSION_GUITAR_LAYOUT.into(),
                },
                ExpansionGuitarProfile {
                    address: [8, 1, 2],
                    layout: EXPANSION_GUITAR_LAYOUT.into(),
                },
            ])
            .is_err()
        );
    }

    #[test]
    fn casm_guided_caps_high_at_95() {
        assert_eq!(apply_casm_guided_high_cap((8, 1, 57), 127), 95);
        assert_eq!(apply_casm_guided_high_cap((8, 0, 1), 75), 75); // mega unchanged
    }

    #[test]
    fn layout_band_split() {
        assert_eq!(expansion_note_band(95), ExpansionNoteBand::Pitched);
        assert_eq!(expansion_note_band(96), ExpansionNoteBand::Fixed);
    }

    #[test]
    fn msb_bank8_rejected_for_expansion_candidate() {
        assert!(!expansion_msb_allowed(8));
        assert!(expansion_msb_allowed(0));
    }

    #[test]
    fn tick_zero_converges_when_no_tick0_mutations() {
        let addr = parse_voice_address(0, 1, 2).unwrap();
        assert!(tick_zero_voice_converges(addr, &[], &[]));
    }

    #[test]
    fn future_stable_rejects_later_bank_change() {
        let addr = parse_voice_address(0, 1, 2).unwrap();
        let controls = [PartControlChange {
            tick: 10,
            controller: 0,
            value: 5,
        }];
        assert!(!expansion_guitar_future_voice_stable(addr, &controls, &[]));
    }

    #[test]
    fn phase1_candidate_returns_layout_when_gates_pass() {
        let addr = parse_voice_address(0, 12, 40).unwrap();
        let profile = expansion_guitar_candidate_phase1(&ExpansionCandidateGates {
            address: addr,
            is_drum_kit: false,
            factory_known: false,
            imported_from_casm: true,
            casm_edited: false,
            shared_source_voice: true,
            changing_voice: false,
            tick_zero_converges: true,
            future_voice_stable: true,
        })
        .unwrap();
        assert_eq!(profile.layout, EXPANSION_GUITAR_LAYOUT);
        assert_eq!(profile.address, [0, 12, 40]);
    }
}
