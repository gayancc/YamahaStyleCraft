//! Bounded, playback-only RTR Pitch Shift math — EV-0108
//! `yamaha_device_probe/pitch_shift_preview.py`.
//!
//! Overlay only when slots agree and an authored bend sensitivity exists.
//! Never writes RPN, mutates Style, steals a channel, or enables Mono/Portamento.
//! Callers own note FIFO queues; this module returns message lists only
//! (display-safe — no hardware send).

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Retrigger rules that use continuous pitch-shift overlay.
pub const CONTINUOUS_RTR: [&str; 2] = ["pitch_shift", "pitch_shift_to_root"];

/// Note kinds eligible for pitched overlay.
pub const PITCHED_KINDS: [&str; 2] = ["basic", "guitar"];

/// Controllers that imply tuning / RPN traffic (do not invent ranges).
pub const TUNING_CONTROLLERS: [u8; 9] = [6, 38, 96, 97, 98, 99, 100, 101, 121];

/// Channel controllers that conflict with overlay ownership.
pub const CONFLICTING_CHANNEL_CONTROLLERS: [u8; 7] = [5, 65, 84, 120, 123, 126, 127];

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BendMessage {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
    pub value14: u16,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MidiBytes {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}

impl From<&BendMessage> for MidiBytes {
    fn from(b: &BendMessage) -> Self {
        Self {
            status: b.status,
            data1: b.data1,
            data2: b.data2,
        }
    }
}

/// Status byte + 14-bit value packed as MIDI pitch bend — bytecode `bend_message`.
pub fn bend_message(channel: u8, offset: i8, sensitivity: u8) -> Result<BendMessage> {
    if !(1..=16).contains(&channel) || !(1..=24).contains(&sensitivity) {
        return Err(Error::Unsupported(
            "Invalid playback pitch-shift bend".into(),
        ));
    }
    let sens = i16::from(sensitivity);
    let off = i16::from(offset);
    if off < -sens || off > sens {
        return Err(Error::Unsupported(
            "Invalid playback pitch-shift bend".into(),
        ));
    }
    let raw = 8192.0 + (8192.0 * f64::from(off) / f64::from(sens));
    let value = raw.round().clamp(0.0, 16383.0) as u16;
    Ok(BendMessage {
        status: 0xE0 | (channel - 1),
        data1: (value & 0x7F) as u8,
        data2: ((value >> 7) & 0x7F) as u8,
        value14: value,
    })
}

fn note_on(channel: u8, pitch: u8, velocity: u8) -> MidiBytes {
    MidiBytes {
        status: 0x90 | (channel - 1),
        data1: pitch,
        data2: velocity,
    }
}

fn note_off(channel: u8, pitch: u8) -> MidiBytes {
    MidiBytes {
        status: 0x80 | (channel - 1),
        data1: pitch,
        data2: 0,
    }
}

/// One sounding / scheduled slot — bytecode `retarget` slot dict.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PitchShiftSlot {
    pub wire_pitch: Option<u8>,
    pub target_pitch: Option<u8>,
    pub velocity: u8,
    pub kind: String,
    pub retrigger_rule: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetargetResult {
    pub messages: Vec<MidiBytes>,
    pub wire_pitches: Vec<Option<u8>>,
    pub shifted: bool,
    pub fallback: bool,
}

/// Channel overlay state — bytecode `PitchShiftPreview`.
#[derive(Clone, Debug, Default)]
pub struct PitchShiftOverlay {
    /// Authored bend ranges per channel (semitones).
    pub ranges: BTreeMap<u8, u8>,
    offsets: BTreeMap<u8, i8>,
}

impl PitchShiftOverlay {
    pub fn new(ranges: BTreeMap<u8, u8>) -> Self {
        Self {
            ranges,
            offsets: BTreeMap::new(),
        }
    }

    pub fn range_for(&self, channel: u8) -> Option<u8> {
        self.ranges.get(&channel).copied()
    }

    pub fn offset(&self, channel: u8) -> i8 {
        self.offsets.get(&channel).copied().unwrap_or(0)
    }

    pub fn set_offset(&mut self, channel: u8, offset: i8) {
        if offset == 0 {
            self.offsets.remove(&channel);
        } else {
            self.offsets.insert(channel, offset);
        }
    }

    /// Compensate a future Note On — bytecode `wire_pitch`.
    pub fn wire_pitch(&self, channel: u8, target_pitch: u8) -> Option<u8> {
        if !(1..=16).contains(&channel) || target_pitch > 127 {
            return None;
        }
        let wire = i16::from(target_pitch) - i16::from(self.offset(channel));
        if (0..=127).contains(&wire) {
            Some(wire as u8)
        } else {
            None
        }
    }

    /// Center overlays for selected channels; returns bend-center messages.
    pub fn reset(&mut self, channels: Option<&[u8]>) -> Result<Vec<BendMessage>> {
        let selected: Vec<u8> = match channels {
            None => self.offsets.keys().copied().collect(),
            Some(cs) => {
                let set: BTreeSet<_> = cs.iter().copied().collect();
                set.into_iter().collect()
            }
        };
        let mut messages = Vec::new();
        for channel in selected {
            if !self.offsets.contains_key(&channel) {
                continue;
            }
            let Some(sensitivity) = self.range_for(channel) else {
                continue;
            };
            messages.push(bend_message(channel, 0, sensitivity)?);
            self.offsets.remove(&channel);
        }
        Ok(messages)
    }

    /// Bytecode `disable` — reset overlay and drop authored range for channel.
    pub fn disable(&mut self, channel: u8) -> Result<Vec<BendMessage>> {
        if !(1..=16).contains(&channel) {
            return Err(Error::Unsupported(
                "Invalid playback pitch-shift channel".into(),
            ));
        }
        let messages = self.reset(Some(&[channel]))?;
        self.ranges.remove(&channel);
        Ok(messages)
    }

    fn validate_slot(slot: &PitchShiftSlot) -> Result<()> {
        for pitch in [slot.wire_pitch, slot.target_pitch].into_iter().flatten() {
            if pitch > 127 {
                return Err(Error::Unsupported(
                    "Invalid playback pitch-shift note".into(),
                ));
            }
        }
        if !(1..=127).contains(&slot.velocity) {
            return Err(Error::Unsupported(
                "Invalid playback pitch-shift velocity".into(),
            ));
        }
        Ok(())
    }

    /// Bytecode `rebase` — fill missing targets from wire+offset, then retarget(converted=False).
    pub fn rebase(&mut self, channel: u8, slots: &[PitchShiftSlot]) -> Result<RetargetResult> {
        let off = self.offset(channel);
        let mut prepared = Vec::with_capacity(slots.len());
        for slot in slots {
            Self::validate_slot(slot)?;
            let mut next = slot.clone();
            if next.target_pitch.is_none()
                && let Some(wire) = next.wire_pitch
            {
                let target = i16::from(wire) + i16::from(off);
                if (0..=127).contains(&target) {
                    next.target_pitch = Some(target as u8);
                }
            }
            prepared.push(next);
        }
        self.retarget(channel, &prepared, false)
    }

    /// Bytecode `retarget` — continuous overlay or Note Off/On fallback.
    pub fn retarget(
        &mut self,
        channel: u8,
        slots: &[PitchShiftSlot],
        converted: bool,
    ) -> Result<RetargetResult> {
        if !(1..=16).contains(&channel) {
            return Err(Error::Unsupported(
                "Invalid playback pitch-shift channel".into(),
            ));
        }
        for slot in slots {
            Self::validate_slot(slot)?;
        }

        let old_offset = self.offset(channel);
        let audible: Vec<&PitchShiftSlot> =
            slots.iter().filter(|s| s.wire_pitch.is_some()).collect();
        let sensitivity = self.range_for(channel);

        let mut candidates = BTreeSet::new();
        for slot in &audible {
            if let (Some(target), Some(wire)) = (slot.target_pitch, slot.wire_pitch) {
                candidates.insert(i16::from(target) - i16::from(wire));
            }
        }
        let agreed_offset = if candidates.len() == 1 {
            candidates.iter().next().copied()
        } else {
            None
        };

        let continuous = converted
            && sensitivity.is_some()
            && !audible.is_empty()
            && candidates.len() == 1
            && slots.iter().all(|s| {
                s.target_pitch.is_some()
                    && PITCHED_KINDS.contains(&s.kind.as_str())
                    && CONTINUOUS_RTR.contains(&s.retrigger_rule.as_str())
            })
            && agreed_offset.is_some_and(|off| {
                let sens = i16::from(sensitivity.unwrap());
                (-sens..=sens).contains(&off)
            })
            && slots.iter().all(|s| {
                s.target_pitch
                    .map(|t| {
                        let wire = i16::from(t) - agreed_offset.unwrap();
                        (0..=127).contains(&wire)
                    })
                    .unwrap_or(true)
            });

        if continuous {
            let offset = agreed_offset.unwrap() as i8;
            let sens = sensitivity.unwrap();
            let mut messages = Vec::new();
            if offset != old_offset {
                messages.push(MidiBytes::from(&bend_message(channel, offset, sens)?));
            }
            let mut wires = Vec::with_capacity(slots.len());
            for slot in slots {
                let mut wire = slot.wire_pitch;
                if wire.is_none()
                    && let Some(target) = slot.target_pitch
                {
                    let w = (i16::from(target) - i16::from(offset)) as u8;
                    wire = Some(w);
                    messages.push(note_on(channel, w, slot.velocity));
                }
                wires.push(wire);
            }
            self.set_offset(channel, offset);
            return Ok(RetargetResult {
                messages,
                wire_pitches: wires,
                shifted: offset != 0,
                fallback: false,
            });
        }

        // Fallback path
        let mut messages = Vec::new();
        let mut changed = Vec::with_capacity(slots.len());
        for slot in slots {
            let wire = slot.wire_pitch;
            let target = slot.target_pitch;
            let replace = old_offset != 0 || wire != target;
            changed.push(replace);
            if replace && let Some(w) = wire {
                messages.push(note_off(channel, w));
            }
        }
        for bend in self.reset(Some(&[channel]))? {
            messages.push(MidiBytes::from(&bend));
        }
        for (slot, replace) in slots.iter().zip(changed.iter()) {
            if *replace && let Some(target) = slot.target_pitch {
                messages.push(note_on(channel, target, slot.velocity));
            }
        }
        let wire_pitches: Vec<Option<u8>> = slots.iter().map(|s| s.target_pitch).collect();
        let any_changed = changed.iter().any(|&c| c);
        Ok(RetargetResult {
            messages,
            wire_pitches,
            shifted: any_changed,
            fallback: any_changed,
        })
    }
}

pub fn is_continuous_rtr(rule: &str) -> bool {
    CONTINUOUS_RTR.contains(&rule)
}

pub fn is_pitched_kind(kind: &str) -> bool {
    PITCHED_KINDS.contains(&kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(wire: Option<u8>, target: Option<u8>, vel: u8) -> PitchShiftSlot {
        PitchShiftSlot {
            wire_pitch: wire,
            target_pitch: target,
            velocity: vel,
            kind: "basic".into(),
            retrigger_rule: "pitch_shift".into(),
        }
    }

    #[test]
    fn bend_center_is_8192() {
        let m = bend_message(1, 0, 12).unwrap();
        assert_eq!(m.value14, 8192);
        assert_eq!(m.status, 0xE0);
        assert_eq!(m.data1, 0);
        assert_eq!(m.data2, 0x40);
    }

    #[test]
    fn bend_full_up_clamps_to_16383() {
        let m = bend_message(2, 12, 12).unwrap();
        assert_eq!(m.value14, 16383);
        assert_eq!(m.status, 0xE1);
    }

    #[test]
    fn bend_rejects_bad_sensitivity() {
        assert!(bend_message(1, 0, 0).is_err());
        assert!(bend_message(1, 2, 1).is_err());
    }

    #[test]
    fn wire_pitch_subtracts_offset() {
        let mut overlay = PitchShiftOverlay::new(BTreeMap::from([(1, 12)]));
        overlay.set_offset(1, 2);
        assert_eq!(overlay.wire_pitch(1, 64), Some(62));
        overlay.set_offset(1, 70);
        assert_eq!(overlay.wire_pitch(1, 60), None);
    }

    #[test]
    fn reset_emits_center_and_clears() {
        let mut overlay = PitchShiftOverlay::new(BTreeMap::from([(3, 12)]));
        overlay.set_offset(3, 4);
        let msgs = overlay.reset(None).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].value14, 8192);
        assert_eq!(overlay.offset(3), 0);
    }

    #[test]
    fn disable_clears_range() {
        let mut overlay = PitchShiftOverlay::new(BTreeMap::from([(1, 12)]));
        overlay.set_offset(1, 3);
        let _ = overlay.disable(1).unwrap();
        assert!(overlay.range_for(1).is_none());
        assert_eq!(overlay.offset(1), 0);
    }

    #[test]
    fn retarget_continuous_emits_bend_when_offset_changes() {
        let mut overlay = PitchShiftOverlay::new(BTreeMap::from([(1, 12)]));
        let slots = vec![slot(Some(60), Some(62), 80)];
        let result = overlay.retarget(1, &slots, true).unwrap();
        assert!(!result.fallback);
        assert!(result.shifted);
        assert_eq!(overlay.offset(1), 2);
        assert_eq!(result.messages[0].status, 0xE0);
        assert_eq!(result.wire_pitches, vec![Some(60)]);
    }

    #[test]
    fn retarget_continuous_note_on_when_wire_missing() {
        let mut overlay = PitchShiftOverlay::new(BTreeMap::from([(1, 12)]));
        // Need at least one audible slot for continuous; use two slots
        let slots = vec![
            slot(Some(60), Some(62), 80),
            PitchShiftSlot {
                wire_pitch: None,
                target_pitch: Some(64),
                velocity: 90,
                kind: "basic".into(),
                retrigger_rule: "pitch_shift".into(),
            },
        ];
        // Second slot has no wire — candidates only from audible; both need same offset
        // audible only first: offset=2. Second target 64 → wire 62 Note On.
        let result = overlay.retarget(1, &slots, true).unwrap();
        assert!(!result.fallback);
        assert!(
            result
                .messages
                .iter()
                .any(|m| m.status == 0x90 && m.data1 == 62 && m.data2 == 90)
        );
        assert_eq!(result.wire_pitches[1], Some(62));
    }

    #[test]
    fn retarget_fallback_when_candidates_disagree() {
        let mut overlay = PitchShiftOverlay::new(BTreeMap::from([(1, 12)]));
        let slots = vec![
            slot(Some(60), Some(62), 80),
            slot(Some(64), Some(67), 80), // offsets 2 vs 3
        ];
        let result = overlay.retarget(1, &slots, true).unwrap();
        assert!(result.fallback);
        assert!(result.messages.iter().any(|m| m.status == 0x80));
        assert!(result.messages.iter().any(|m| m.status == 0x90));
        assert_eq!(result.wire_pitches, vec![Some(62), Some(67)]);
    }

    #[test]
    fn rebase_uses_converted_false_fallback() {
        let mut overlay = PitchShiftOverlay::new(BTreeMap::from([(2, 12)]));
        overlay.set_offset(2, 2);
        let slots = vec![slot(Some(60), None, 70)];
        let result = overlay.rebase(2, &slots).unwrap();
        assert!(result.fallback);
        // target filled as 62; fallback note-ons at target
        assert_eq!(result.wire_pitches, vec![Some(62)]);
    }
}
