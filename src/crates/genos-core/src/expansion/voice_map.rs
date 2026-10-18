//! Bank/program map recovered from installation `PACK` MIDI setup payloads.
//!
//! Expansion install packs store per-voice/drum setup as SMF (`MThd`/`MTrk`)
//! FBIN items. This scanner extracts the last CC0 (MSB), CC32 (LSB), and
//! Program Change observed on each channel — enough to feed the style-editor
//! voice picker without inventing catalog names.

use crate::Error;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAddress {
    pub channel: u8,
    pub msb: u8,
    pub lsb: u8,
    pub program: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackVoiceMap {
    pub pack_title: Option<String>,
    pub item_title: Option<String>,
    pub addresses: Vec<VoiceAddress>,
}

/// Scan one SMF payload for bank/program setup events.
pub fn scan_midi_voice_addresses(data: &[u8]) -> Result<Vec<VoiceAddress>, Error> {
    if !data.starts_with(b"MThd") {
        return Err(Error::Format("not an SMF MThd payload".into()));
    }
    let mut i = 0usize;
    let mut out = Vec::new();
    // Per-channel running state
    let mut msb = [0u8; 16];
    let mut lsb = [0u8; 16];
    let mut prog = [0u8; 16];
    let mut seen = [false; 16];

    while i + 8 <= data.len() {
        let tag = &data[i..i + 4];
        let len = u32::from_be_bytes(data[i + 4..i + 8].try_into().unwrap()) as usize;
        i += 8;
        let end = i
            .checked_add(len)
            .filter(|e| *e <= data.len())
            .ok_or_else(|| Error::Format("truncated SMF chunk".into()))?;
        if tag == b"MTrk" {
            scan_track(&data[i..end], &mut msb, &mut lsb, &mut prog, &mut seen)?;
        }
        i = end;
    }

    for ch in 0..16u8 {
        if seen[ch as usize] {
            out.push(VoiceAddress {
                channel: ch + 1,
                msb: msb[ch as usize],
                lsb: lsb[ch as usize],
                program: prog[ch as usize],
            });
        }
    }
    Ok(out)
}

fn scan_track(
    track: &[u8],
    msb: &mut [u8; 16],
    lsb: &mut [u8; 16],
    prog: &mut [u8; 16],
    seen: &mut [bool; 16],
) -> Result<(), Error> {
    let mut p = 0usize;
    let mut running: Option<u8> = None;
    while p < track.len() {
        // delta time
        let _ = read_vlq(track, &mut p)?;
        if p >= track.len() {
            break;
        }
        let mut status = track[p];
        if status < 0x80 {
            status = running.ok_or_else(|| Error::Format("SMF missing running status".into()))?;
        } else {
            p += 1;
            if status < 0xf0 {
                running = Some(status);
            } else {
                running = None;
            }
        }
        let hi = status & 0xf0;
        let ch = (status & 0x0f) as usize;
        match hi {
            0xc0 => {
                // Program change
                if p >= track.len() {
                    return Err(Error::Format("truncated program change".into()));
                }
                prog[ch] = track[p];
                seen[ch] = true;
                p += 1;
            }
            0xb0 => {
                if p + 1 >= track.len() {
                    return Err(Error::Format("truncated control change".into()));
                }
                let cc = track[p];
                let val = track[p + 1];
                p += 2;
                match cc {
                    0 => {
                        msb[ch] = val;
                        seen[ch] = true;
                    }
                    32 => {
                        lsb[ch] = val;
                        seen[ch] = true;
                    }
                    _ => {}
                }
            }
            0x80 | 0x90 | 0xa0 | 0xe0 => p = p.saturating_add(2),
            0xd0 => p = p.saturating_add(1),
            0xf0 => {
                // Meta / sysex
                if status == 0xff {
                    if p >= track.len() {
                        break;
                    }
                    p += 1; // meta type
                    let n = read_vlq(track, &mut p)? as usize;
                    p = p.saturating_add(n);
                } else if status == 0xf0 || status == 0xf7 {
                    let n = read_vlq(track, &mut p)? as usize;
                    p = p.saturating_add(n);
                } else {
                    // other system — skip one byte if present
                    p = p.saturating_add(1);
                }
            }
            _ => {
                // unknown — stop this track conservatively
                break;
            }
        }
    }
    Ok(())
}

fn read_vlq(data: &[u8], p: &mut usize) -> Result<u32, Error> {
    let mut v = 0u32;
    for _ in 0..4 {
        if *p >= data.len() {
            return Err(Error::Format("truncated VLQ".into()));
        }
        let b = data[*p];
        *p += 1;
        v = (v << 7) | u32::from(b & 0x7f);
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err(Error::Format("VLQ too long".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smf_with_bank_prog(msb: u8, lsb: u8, prog: u8) -> Vec<u8> {
        // Minimal format-0 SMF, one track, ch1 CC0/CC32/PC
        let mut track = Vec::new();
        // delta 0, CC0
        track.extend_from_slice(&[0x00, 0xb0, 0x00, msb]);
        track.extend_from_slice(&[0x00, 0xb0, 0x20, lsb]);
        track.extend_from_slice(&[0x00, 0xc0, prog]);
        track.extend_from_slice(&[0x00, 0xff, 0x2f, 0x00]); // end of track

        let mut out = Vec::new();
        out.extend_from_slice(b"MThd");
        out.extend_from_slice(&6u32.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes()); // format
        out.extend_from_slice(&1u16.to_be_bytes()); // ntrks
        out.extend_from_slice(&480u16.to_be_bytes()); // ppq
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(track.len() as u32).to_be_bytes());
        out.extend_from_slice(&track);
        out
    }

    #[test]
    fn extracts_bank_and_program() {
        let data = smf_with_bank_prog(63, 8, 12);
        let addrs = scan_midi_voice_addresses(&data).unwrap();
        assert_eq!(
            addrs,
            vec![VoiceAddress {
                channel: 1,
                msb: 63,
                lsb: 8,
                program: 12
            }]
        );
    }
}
