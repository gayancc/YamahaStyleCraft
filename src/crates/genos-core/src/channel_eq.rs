//! Genos channel EQ High / Low as NRPN (not MIDI CC lanes).
//!
//! Black-box evidence from live SFM Style Studio V2.0.44 static Mixer EQ
//! (`part-fx` `setEq` / `readEq` wire sequence observed 2026-10-18):
//! - CC99 (NRPN MSB) = 1
//! - CC98 (NRPN LSB) = 48 (`eqLow`) or 49 (`eqHigh`)
//! - CC6  (Data Entry) = raw 0..127 (display offset = raw − 64; 64 neutral)
//! - CC101 = 127, CC100 = 127 (null RPN / end parameter)
//!
//! Timed `eqChanges` automation in SFM is a separate typed project field and
//! is **not** covered here. Arbitrary SysEx remains preserved read-only.

use crate::smf::Event;
use serde::{Deserialize, Serialize};

pub const NRPN_MSB: u8 = 99;
pub const NRPN_LSB: u8 = 98;
pub const DATA_ENTRY: u8 = 6;
pub const RPN_MSB: u8 = 101;
pub const RPN_LSB: u8 = 100;
pub const NRPN_MSB_VALUE: u8 = 1;
pub const NULL_RPN: u8 = 127;
pub const CENTER: u8 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EqBand {
    EqLow,
    EqHigh,
}

impl EqBand {
    pub fn nrpn_lsb(self) -> u8 {
        match self {
            Self::EqLow => 48,
            Self::EqHigh => 49,
        }
    }

    pub fn from_nrpn_lsb(lsb: u8) -> Option<Self> {
        match lsb {
            48 => Some(Self::EqLow),
            49 => Some(Self::EqHigh),
            _ => None,
        }
    }
}

/// Build the five CC bytes for one band (status nibble filled by caller).
pub fn nrpn_packet(band: EqBand, value: u8) -> [(u8, u8); 5] {
    [
        (NRPN_MSB, NRPN_MSB_VALUE),
        (NRPN_LSB, band.nrpn_lsb()),
        (DATA_ENTRY, value.min(127)),
        (RPN_MSB, NULL_RPN),
        (RPN_LSB, NULL_RPN),
    ]
}

fn is_cc(e: &Event, status: u8, controller: u8) -> Option<u8> {
    if e.data.len() == 3 && e.data[0] == status && e.data[1] == controller {
        Some(e.data[2])
    } else {
        None
    }
}

/// Read the last complete EQ NRPN for `band` at `tick` on `channel` (1–16).
pub fn value_at(events: &[Event], channel: u8, tick: u64, band: EqBand) -> Option<u8> {
    if !(1..=16).contains(&channel) {
        return None;
    }
    let status = 0xb0 | (channel - 1);
    let want_lsb = band.nrpn_lsb();
    let mut msb = None;
    let mut lsb = None;
    let mut found = None;
    for e in events.iter().filter(|e| e.tick == tick) {
        if let Some(v) = is_cc(e, status, NRPN_MSB) {
            msb = Some(v);
            lsb = None;
        } else if let Some(v) = is_cc(e, status, NRPN_LSB) {
            lsb = Some(v);
        } else if let Some(v) = is_cc(e, status, DATA_ENTRY) {
            if msb == Some(NRPN_MSB_VALUE) && lsb == Some(want_lsb) {
                found = Some(v);
            }
            msb = None;
            lsb = None;
        } else if is_cc(e, status, RPN_MSB).is_some() || is_cc(e, status, RPN_LSB).is_some() {
            // Null RPN ends the parameter; keep last matching data entry.
            msb = None;
            lsb = None;
        }
    }
    found
}

/// Indices of events at `tick` that belong to an EQ NRPN packet for `band`.
///
/// Tracks packet member indices in the outer state machine so interleaved
/// non-EQ CCs at the same tick do not break removal.
pub fn packet_indices(events: &[Event], channel: u8, tick: u64, band: EqBand) -> Vec<usize> {
    if !(1..=16).contains(&channel) {
        return Vec::new();
    }
    let status = 0xb0 | (channel - 1);
    let want_lsb = band.nrpn_lsb();
    let mut out = Vec::new();
    // 0 idle, 1 after MSB=1, 2 after LSB match, 3 after data, 4 after RPN MSB
    let mut phase = 0u8;
    let mut packet = [0usize; 5];
    for (i, e) in events.iter().enumerate() {
        if e.tick != tick || e.data.len() != 3 || e.data[0] != status {
            continue;
        }
        let (cc, val) = (e.data[1], e.data[2]);
        // Ignore unrelated CCs at this tick; only EQ NRPN steps advance/reset.
        match phase {
            0 if cc == NRPN_MSB && val == NRPN_MSB_VALUE => {
                phase = 1;
                packet[0] = i;
            }
            1 if cc == NRPN_LSB && val == want_lsb => {
                phase = 2;
                packet[1] = i;
            }
            1 if cc == NRPN_MSB && val == NRPN_MSB_VALUE => {
                packet[0] = i;
            }
            1 if matches!(cc, NRPN_MSB | NRPN_LSB | DATA_ENTRY | RPN_MSB | RPN_LSB) => {
                phase = 0;
            }
            2 if cc == DATA_ENTRY => {
                phase = 3;
                packet[2] = i;
            }
            2 if matches!(cc, NRPN_MSB | NRPN_LSB | DATA_ENTRY | RPN_MSB | RPN_LSB) => {
                phase = 0;
            }
            3 if cc == RPN_MSB && val == NULL_RPN => {
                phase = 4;
                packet[3] = i;
            }
            3 if matches!(cc, NRPN_MSB | NRPN_LSB | DATA_ENTRY | RPN_MSB | RPN_LSB) => {
                phase = 0;
            }
            4 if cc == RPN_LSB && val == NULL_RPN => {
                packet[4] = i;
                out.extend_from_slice(&packet);
                phase = 0;
            }
            4 if matches!(cc, NRPN_MSB | NRPN_LSB | DATA_ENTRY | RPN_MSB | RPN_LSB) => {
                phase = 0;
            }
            _ => {}
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc(tick: u64, ch: u8, controller: u8, value: u8, id: u64) -> Event {
        Event {
            id,
            tick,
            order: id,
            data: vec![0xb0 | (ch - 1), controller, value],
        }
    }

    #[test]
    fn roundtrip_low_and_high_independent() {
        let mut events = Vec::new();
        for (i, (ctrl, val)) in nrpn_packet(EqBand::EqLow, 80).into_iter().enumerate() {
            events.push(cc(0, 11, ctrl, val, i as u64));
        }
        for (i, (ctrl, val)) in nrpn_packet(EqBand::EqHigh, 40).into_iter().enumerate() {
            events.push(cc(0, 11, ctrl, val, 10 + i as u64));
        }
        assert_eq!(value_at(&events, 11, 0, EqBand::EqLow), Some(80));
        assert_eq!(value_at(&events, 11, 0, EqBand::EqHigh), Some(40));
        assert_eq!(packet_indices(&events, 11, 0, EqBand::EqLow).len(), 5);
        assert_eq!(packet_indices(&events, 11, 0, EqBand::EqHigh).len(), 5);
    }

    #[test]
    fn unrelated_nrpn_msb1_ignored() {
        let events = vec![
            cc(0, 9, NRPN_MSB, 1, 1),
            cc(0, 9, NRPN_LSB, 0x66, 2),
            cc(0, 9, DATA_ENTRY, 64, 3),
        ];
        assert_eq!(value_at(&events, 9, 0, EqBand::EqLow), None);
        assert!(packet_indices(&events, 9, 0, EqBand::EqLow).is_empty());
    }

    #[test]
    fn packet_indices_tolerates_interleaved_alien_cc() {
        let mut events = vec![cc(0, 11, 7, 100, 0)]; // volume at same tick
        for (i, (ctrl, val)) in nrpn_packet(EqBand::EqLow, 80).into_iter().enumerate() {
            if i == 2 {
                events.push(cc(0, 11, 10, 64, 50)); // pan between data-entry steps
            }
            events.push(cc(0, 11, ctrl, val, 10 + i as u64));
        }
        let idx = packet_indices(&events, 11, 0, EqBand::EqLow);
        assert_eq!(idx.len(), 5);
        assert_eq!(value_at(&events, 11, 0, EqBand::EqLow), Some(80));
        // Alien CCs must not be collected.
        assert!(!idx.contains(&0));
        assert!(!idx.iter().any(|&i| events[i].data[1] == 10));
    }
}
