//! Genos system-effect SysEx (F021 wire path: reverb/chorus type + return).
//!
//! Evidence (static, no invented addresses):
//! - ProbeService `_xg_parameter_change` / `_send_genos_system_effect` in
//!   `service.cp313-win_amd64.pyd` SHA-256 `d5a9273ac5d2c2100b0aa86c55a0ff31c124396601ae03d1a9d753d379a4e034`
//!   (EV-0127 / REA): frame `F0 43 10 4C` + 3-byte address + ≥1 data + `F7`;
//!   blocks limited to `reverb`/`chorus`; type data is `[msb, lsb]`; return is one byte.
//! - Fingerprinted Genos v2.00 Data List EFFECT1 table: reverb type `02 01 00`,
//!   reverb return `02 01 0C`, chorus type `02 01 20`, chorus return `02 01 2C`.
//! - REA: `GENOS_SYSTEM_EFFECT_ADDRESSES["reverb"]["type"]` shares the integer `0`
//!   object with `_effect_7bit`'s lower bound (address low byte 0).

use crate::{Error, Result};
use midir::MidiOutput;
use serde::Serialize;

/// XG parameter-change prefix used by ProbeService (`n` = 0 → `0x10`).
pub const XG_PARAM_PREFIX: [u8; 4] = [0xF0, 0x43, 0x10, 0x4C];
pub const XG_EOX: u8 = 0xF7;

const SYSTEM_HH: u8 = 0x02;
const SYSTEM_MM: u8 = 0x01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemEffectAddresses {
    pub type_ll: u8,
    pub return_ll: u8,
}

/// `GENOS_SYSTEM_EFFECT_ADDRESSES` — reverb/chorus only (ProbeService gate).
pub fn addresses_for(block: &str) -> Result<SystemEffectAddresses> {
    match block {
        "reverb" => Ok(SystemEffectAddresses {
            type_ll: 0x00,
            return_ll: 0x0C,
        }),
        "chorus" => Ok(SystemEffectAddresses {
            type_ll: 0x20,
            return_ll: 0x2C,
        }),
        _ => Err(Error::Midi(
            "Genos system effect block must be reverb or chorus".into(),
        )),
    }
}

fn seven_bit(label: &str, value: u8) -> Result<u8> {
    if value > 127 {
        return Err(Error::Midi(format!(
            "{label}: XG address and data bytes must be 7-bit values"
        )));
    }
    Ok(value)
}

/// Build one XG parameter-change SysEx packet (ProbeService `_xg_parameter_change`).
pub fn xg_parameter_change(address: [u8; 3], data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Err(Error::Midi(
            "A three-byte XG address and at least one data byte are required".into(),
        ));
    }
    for (i, b) in address.iter().enumerate() {
        seven_bit(&format!("address[{i}]"), *b)?;
    }
    for (i, b) in data.iter().enumerate() {
        seven_bit(&format!("data[{i}]"), *b)?;
    }
    let mut out = Vec::with_capacity(4 + 3 + data.len() + 1);
    out.extend_from_slice(&XG_PARAM_PREFIX);
    out.extend_from_slice(&address);
    out.extend_from_slice(data);
    out.push(XG_EOX);
    Ok(out)
}

/// Build optional type and/or return packets for one system-effect block.
///
/// Mirrors `_send_genos_system_effect(block, msb, lsb, return_level)`:
/// type packet only when both MSB and LSB are present; return only when set.
pub fn system_effect_packets(
    block: &str,
    msb: Option<u8>,
    lsb: Option<u8>,
    return_level: Option<u8>,
) -> Result<Vec<Vec<u8>>> {
    let addrs = addresses_for(block)?;
    if msb.is_some() != lsb.is_some() {
        return Err(Error::Midi("Effect type requires both MSB and LSB".into()));
    }
    let mut packets = Vec::new();
    if let (Some(msb), Some(lsb)) = (msb, lsb) {
        let msb = seven_bit("msb", msb)?;
        let lsb = seven_bit("lsb", lsb)?;
        packets.push(xg_parameter_change(
            [SYSTEM_HH, SYSTEM_MM, addrs.type_ll],
            &[msb, lsb],
        )?);
    }
    if let Some(level) = return_level {
        let level = seven_bit("returnLevel", level)?;
        packets.push(xg_parameter_change(
            [SYSTEM_HH, SYSTEM_MM, addrs.return_ll],
            &[level],
        )?);
    }
    if packets.is_empty() {
        return Err(Error::Midi("Invalid Genos system effect setup".into()));
    }
    Ok(packets)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub block: String,
    pub packets: Vec<Vec<u8>>,
    pub packet_count: usize,
}

/// Transmit system-effect SysEx on an exact MIDI output (name + index check).
pub fn apply_system_effect(
    index: usize,
    name: &str,
    block: &str,
    msb: Option<u8>,
    lsb: Option<u8>,
    return_level: Option<u8>,
) -> Result<ApplyResult> {
    let packets = system_effect_packets(block, msb, lsb, return_level)?;
    let output = MidiOutput::new("GenosStyleStudio system effect").map_err(map_err)?;
    let ports = output.ports();
    let port = ports
        .get(index)
        .ok_or_else(|| Error::Midi("output disconnected; refresh devices".into()))?;
    if output.port_name(port).map_err(map_err)? != name {
        return Err(Error::Midi(
            "device list changed; select the output again".into(),
        ));
    }
    let mut connection = output
        .connect(port, "GenosStyleStudio system effect")
        .map_err(map_err)?;
    for packet in &packets {
        connection.send(packet).map_err(map_err)?;
    }
    Ok(ApplyResult {
        block: block.to_string(),
        packet_count: packets.len(),
        packets,
    })
}

fn map_err(e: impl std::fmt::Display) -> Error {
    Error::Midi(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xg_parameter_change_matches_probe_framing() {
        let packet = xg_parameter_change([0x02, 0x01, 0x00], &[0x01, 0x00]).unwrap();
        assert_eq!(
            packet,
            vec![0xF0, 0x43, 0x10, 0x4C, 0x02, 0x01, 0x00, 0x01, 0x00, 0xF7]
        );
    }

    #[test]
    fn reverb_type_and_return_packets() {
        let packets = system_effect_packets("reverb", Some(1), Some(0), Some(64)).unwrap();
        assert_eq!(packets.len(), 2);
        assert_eq!(
            packets[0],
            vec![0xF0, 0x43, 0x10, 0x4C, 0x02, 0x01, 0x00, 0x01, 0x00, 0xF7]
        );
        assert_eq!(
            packets[1],
            vec![0xF0, 0x43, 0x10, 0x4C, 0x02, 0x01, 0x0C, 0x40, 0xF7]
        );
    }

    #[test]
    fn chorus_type_only_omits_return() {
        let packets = system_effect_packets("chorus", Some(65), Some(0), None).unwrap();
        assert_eq!(packets.len(), 1);
        assert_eq!(
            packets[0],
            vec![0xF0, 0x43, 0x10, 0x4C, 0x02, 0x01, 0x20, 0x41, 0x00, 0xF7]
        );
    }

    #[test]
    fn variation_block_is_rejected() {
        let err = system_effect_packets("variation", Some(1), Some(0), None).unwrap_err();
        assert!(err.to_string().contains("reverb or chorus"));
    }

    #[test]
    fn empty_setup_is_rejected() {
        let err = system_effect_packets("reverb", None, None, None).unwrap_err();
        assert!(
            err.to_string()
                .contains("Invalid Genos system effect setup")
        );
    }

    #[test]
    fn rejects_8bit_data() {
        assert!(xg_parameter_change([0x02, 0x01, 0x00], &[0x80]).is_err());
    }

    #[test]
    fn partial_type_never_silently_sends_only_return() {
        assert!(system_effect_packets("reverb", Some(1), None, Some(64)).is_err());
        assert!(system_effect_packets("chorus", None, Some(128), Some(64)).is_err());
    }
}
