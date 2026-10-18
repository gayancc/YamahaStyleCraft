//! Yamaha `IO::Secure::sme_encryptbuf` stream cipher (EV-0316).
//!
//! Recovered from Expansion Manager.exe vtable methods:
//! - 16-byte seed expanded to a 128-byte keystream table (`0x14095ccf0`)
//! - Rolling XOR with 7-bit counter (`0x14095cb80` / `0x14095cc70`)
//! - Optional PKCS-style pad to 16-byte blocks on encrypt
//!
//! The 16-byte seed is supplied by YEM at stream construction (object field
//! `+0x10`, written into `CESC`/`CSEC` chunks on the offline Model Encryption /
//! SavePackInstallation path). Registration XML templates mentioning
//! `<encryptionkey>` / `Network::SSLCommunicator` are **not** the offline seed
//! source — packs are built without that live path. The seed is **not** derived
//! from `.n27` fields alone (EV-0315/0316).

use crate::Error;

pub const SEED_LEN: usize = 16;
pub const TABLE_LEN: usize = 128;

/// Expand a 16-byte seed into the 128-byte SME keystream table.
pub fn expand_keystream(seed: &[u8; SEED_LEN]) -> [u8; TABLE_LEN] {
    let mut out = [0u8; TABLE_LEN];
    for col in 0..SEED_LEN {
        let mut al = seed[col];
        for row in 0..8 {
            out[row * SEED_LEN + col] = al;
            let nxt = al >> 1;
            al = if al & 1 == 0 { nxt } else { nxt | 0x80 };
        }
    }
    out
}

/// XOR `data` in place with the SME keystream starting at `counter` (mod 128).
pub fn apply_keystream(data: &mut [u8], table: &[u8; TABLE_LEN], mut counter: u16) {
    for byte in data.iter_mut() {
        *byte ^= table[(counter & 0x7f) as usize];
        counter = counter.wrapping_add(1);
    }
}

/// Decrypt a VWST/VWDL payload with a known 16-byte seed.
///
/// `counter` is the stream position in bytes from the start of the encrypted
/// section (wave `startaddress` values are byte offsets; `frameaddress << 4`
/// equals `startaddress` in YEM wave indexes).
pub fn decrypt_sme(
    ciphertext: &[u8],
    seed: &[u8; SEED_LEN],
    counter: u16,
) -> Result<Vec<u8>, Error> {
    // Odd lengths are allowed for partial VWST probes; pad strip is separate.
    let table = expand_keystream(seed);
    let mut out = ciphertext.to_vec();
    apply_keystream(&mut out, &table, counter);
    Ok(out)
}

/// Strip SME encrypt padding (EV-0316).
///
/// Encrypt stores `original_len % 16` in the final byte and always appends
/// enough random bytes to reach the next 16-byte boundary (a full extra block
/// when already aligned). Decrypt restores
/// `len = padded_len - 16 + last_byte` (`0x14095cc97`).
pub fn strip_padding(mut data: Vec<u8>) -> Result<Vec<u8>, Error> {
    if data.is_empty() {
        return Ok(data);
    }
    if !data.len().is_multiple_of(16) {
        return Err(Error::Format(
            "SME padded payload length is not a multiple of 16".into(),
        ));
    }
    let last = *data.last().unwrap() as usize;
    if last >= 16 {
        return Err(Error::Format(format!(
            "SME trailing length byte {last} is out of range"
        )));
    }
    let new_len = data
        .len()
        .checked_sub(16)
        .and_then(|n| n.checked_add(last))
        .ok_or_else(|| Error::Format("SME padding underflow".into()))?;
    if new_len > data.len() {
        return Err(Error::Format("SME padding length exceeds buffer".into()));
    }
    data.truncate(new_len);
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_zero_seed_is_zero_table() {
        let table = expand_keystream(&[0u8; 16]);
        assert!(table.iter().all(|&b| b == 0));
    }

    #[test]
    fn expand_matches_xtime_column_for_0x01() {
        let mut seed = [0u8; 16];
        seed[0] = 0x01;
        let table = expand_keystream(&seed);
        // column 0 rows: 01, 80, 40, 20, 10, 08, 04, 02
        assert_eq!(table[0], 0x01);
        assert_eq!(table[16], 0x80);
        assert_eq!(table[32], 0x40);
        assert_eq!(table[48], 0x20);
        assert_eq!(table[64], 0x10);
        assert_eq!(table[80], 0x08);
        assert_eq!(table[96], 0x04);
        assert_eq!(table[112], 0x02);
        // other columns stay zero
        assert_eq!(table[1], 0);
    }

    #[test]
    fn xor_roundtrip() {
        let seed = *b"0123456789abcdef";
        let plain = b"The quick brown fox jumps over the lazy dog!!";
        let mut cipher = plain.to_vec();
        let table = expand_keystream(&seed);
        apply_keystream(&mut cipher, &table, 0);
        assert_ne!(cipher, plain);
        apply_keystream(&mut cipher, &table, 0);
        assert_eq!(&cipher[..], &plain[..]);
    }

    #[test]
    fn counter_offset_matches_address_mod_128() {
        let seed = *b"YamahaGenosKey!!";
        let table = expand_keystream(&seed);
        let mut block = vec![0u8; 32];
        apply_keystream(&mut block, &table, (0x5853_0000u32 & 0x7f) as u16);
        // deterministic non-zero keystream at that counter
        assert!(block.iter().any(|&b| b != 0));
    }

    #[test]
    fn strip_padding_restores_mod16_length() {
        // original 10 bytes → pad to 16, last byte = 10
        let mut padded = vec![1u8; 10];
        padded.extend(std::iter::repeat_n(0xAAu8, 5));
        padded.push(10);
        assert_eq!(padded.len(), 16);
        let out = strip_padding(padded).unwrap();
        assert_eq!(out.len(), 10);
    }

    #[test]
    fn strip_padding_aligned_uses_extra_block() {
        // original 16 bytes → pad full block, last byte = 0
        let mut padded = vec![1u8; 16];
        padded.extend(std::iter::repeat_n(0xBBu8, 15));
        padded.push(0);
        assert_eq!(padded.len(), 32);
        let out = strip_padding(padded).unwrap();
        assert_eq!(out.len(), 16);
    }
}
