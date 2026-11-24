use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    sync::OnceLock,
};

pub(crate) const CHORD_TYPE_COUNT: u8 = 59;
const ROW: usize = CHORD_TYPE_COUNT as usize * 12;
const ELF_HASH: &str = "be2f6b1a9815c217a2c8944a0123e5a533d5284a0aee29e0fe25858fbbc46b31";
const PRG_HASH: &str = "9e115019bfb4ed369cbd7c4d288828b58d9597ee587a85bb7dc11edc7322d4d8";
const PRG_SIZE: u64 = 324_384_256;
const ELF_SIZE: usize = 55_599_219;
// Ordered byte ranges of firmware.tar.gz in the exact digest-pinned PRG.
const ARCHIVE_RANGES: [(u64, usize); 10] = [
    (1_083_392, 16_384),
    (165_102_592, 49_152),
    (159_679_488, 65_536),
    (159_777_792, 131_072),
    (160_039_936, 262_144),
    (165_151_744, 524_288),
    (168_559_616, 1_048_576),
    (169_870_336, 6_291_456),
    (178_258_944, 6_291_456),
    (186_647_552, 4_367_344),
];

pub(crate) struct Tables {
    adjustments: Vec<u8>,
    attributes: Vec<u8>,
    shuffle: Vec<u8>,
    chord_masks: Vec<u8>,
    guitar: [Vec<u8>; 3],
    casm_defaults: Vec<u8>,
}

static TABLES: OnceLock<std::result::Result<Option<Tables>, String>> = OnceLock::new();
/// Tables the user selected in the app; they take precedence over the environment.
static USER_TABLES: OnceLock<Tables> = OnceLock::new();

/// Loads the conversion tables from the user's own Genos 2.13 update image (`.PRG`)
/// or its extracted ARM ELF. Both are digest-checked; anything else is rejected.
/// Returns `Ok(false)` when tables were already loaded from a file this session.
pub fn configure_file(path: &Path) -> Result<bool> {
    if USER_TABLES.get().is_some() {
        return Ok(false);
    }
    let size = std::fs::metadata(path)
        .map_err(|e| Error::Unsupported(format!("Cannot read the firmware file: {e}")))?
        .len();
    let elf = if size == PRG_SIZE {
        read_prg_elf(path).map_err(|_| {
            Error::Unsupported("Expected the verified Genos 2.13 update image".into())
        })?
    } else if size == ELF_SIZE as u64 {
        std::fs::read(path)
            .map_err(|e| Error::Unsupported(format!("Cannot read the firmware file: {e}")))?
    } else {
        return Err(Error::Unsupported(
            "Choose the Genos 2.13 update file (.PRG) or its extracted 17wsh ELF".into(),
        ));
    };
    let tables = Tables::load(&elf).map_err(Error::Unsupported)?;
    Ok(USER_TABLES.set(tables).is_ok())
}

/// Whether Genos firmware conversion tables are active.
pub fn is_configured() -> bool {
    configured().is_ok_and(|tables| tables.is_some())
}

pub(crate) fn configured() -> Result<Option<&'static Tables>> {
    if let Some(tables) = USER_TABLES.get() {
        return Ok(Some(tables));
    }
    TABLES
        .get_or_init(|| {
            if let Some(path) = std::env::var_os("GENOS_NTT_PRG") {
                let elf = read_prg_elf(Path::new(&path))
                    .map_err(|e| format!("Cannot load GENOS_NTT_PRG: {e}"))?;
                return Tables::load(&elf).map(Some);
            }
            let Some(path) = std::env::var_os("GENOS_NTT_ELF") else {
                return Ok(None);
            };
            let elf =
                std::fs::read(&path).map_err(|e| format!("Cannot read GENOS_NTT_ELF: {e}"))?;
            Tables::load(&elf).map(Some)
        })
        .as_ref()
        .map(|tables| tables.as_ref())
        .map_err(|error| Error::Unsupported(error.clone()))
}

fn read_prg_elf(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Expected the verified Genos 2.13 update image",
        )
    };
    if file.metadata()?.len() != PRG_SIZE {
        return Err(invalid());
    }
    let mut hash = Sha256::new();
    let bytes_read = io::copy(&mut (&mut file).take(PRG_SIZE), &mut hash)?;
    if bytes_read != PRG_SIZE || format!("{:x}", hash.finalize()) != PRG_HASH {
        return Err(invalid());
    }
    let mut archive = Vec::with_capacity(ARCHIVE_RANGES.iter().map(|(_, size)| size).sum());
    for (offset, size) in ARCHIVE_RANGES {
        file.seek(SeekFrom::Start(offset))?;
        let start = archive.len();
        archive.resize(start + size, 0);
        file.read_exact(&mut archive[start..])?;
    }
    let mut tar = flate2::read::GzDecoder::new(archive.as_slice());
    // The pinned tar places the ELF at byte 2560 after its member header.
    let mut prefix = [0; 2560];
    tar.read_exact(&mut prefix)?;
    if !prefix[2048..].starts_with(b"program/build_cross/17wsh\0") {
        return Err(invalid());
    }
    let mut elf = vec![0; ELF_SIZE];
    tar.read_exact(&mut elf)?;
    Ok(elf)
}

impl Tables {
    fn load(elf: &[u8]) -> std::result::Result<Self, String> {
        if format!("{:x}", Sha256::digest(elf)) != ELF_HASH {
            return Err("Expected the verified Genos 2.13 build 52 ARM ELF".into());
        }
        // The pinned image is ELF32 little-endian. Read the virtual address
        // mapping from its section headers rather than hardcoding file offsets.
        let word = |at| u32::from_le_bytes(elf[at..at + 4].try_into().unwrap()) as usize;
        let half = |at| u16::from_le_bytes(elf[at..at + 2].try_into().unwrap()) as usize;
        let table = |address, size| {
            for index in 0..half(48) {
                let section = word(32) + index * half(46);
                let start = word(section + 12);
                if word(section + 8) & 2 != 0
                    && word(section + 4) != 8
                    && address >= start
                    && address + size <= start + word(section + 20)
                {
                    let offset = word(section + 16) + address - start;
                    return Ok(elf[offset..offset + size].to_vec());
                }
            }
            Err(format!("Missing Genos firmware table at {address:#x}"))
        };
        Ok(Self {
            adjustments: table(0x1420940, 13 * ROW)?,
            attributes: table(0x1422D34, 11 * ROW)?,
            shuffle: table(0x14208A4, 13 * 12)?,
            chord_masks: table(0x1416318, usize::from(CHORD_TYPE_COUNT) * 2)?,
            guitar: [
                table(0x14BDFE0, 4 * 12 * ROW)?,
                table(0x14B5B20, 4 * 12 * ROW)?,
                table(0x14AD660, 4 * 12 * ROW)?,
            ],
            casm_defaults: table(0x14AD37C, 8 * 55)?,
        })
    }

    pub(crate) fn chord_mask(&self, chord_type: u8) -> Option<(usize, u8)> {
        let offset = usize::from(chord_type) * 2;
        let entry = self.chord_masks.get(offset..offset + 2)?;
        Some((13 + usize::from(entry[0]), entry[1]))
    }

    pub(crate) fn casm_default(&self, destination: u8) -> Option<&[u8]> {
        let index = usize::from(destination.checked_sub(8)?);
        let offset = index * 55;
        let record = self.casm_defaults.get(offset..offset + 55)?;
        (record.starts_with(b"Ctb2\0\0\0/") && record[17] == destination).then_some(&record[8..])
    }

    pub(crate) fn pitch(
        &self,
        note: u8,
        rule: &crate::chord_preview::PitchRule<'_>,
        ntt: u8,
        source_type: u8,
        target_type: u8,
    ) -> i16 {
        let note = i16::from(note);
        let source = i16::from(rule.source_root);
        let target = i16::from(rule.target_root);
        let delta = target - source;
        let source_pc = (note - source).rem_euclid(12) as usize;
        let mut row = usize::from(
            self.attributes[usize::from(ntt) * ROW + usize::from(source_type) * 12 + source_pc],
        );
        let (base, pc) = if rule.ntr == "root_trans" {
            let octave = if rule.exceeds_firmware_high_key() {
                12
            } else {
                0
            };
            (note + delta - octave, source_pc)
        } else {
            row = usize::from(self.shuffle[row * 12 + delta.rem_euclid(12) as usize]);
            (note, (note - target).rem_euclid(12) as usize)
        };
        base + i16::from(self.adjustments[row * ROW + usize::from(target_type) * 12 + pc] as i8)
    }

    pub(crate) fn guitar_pitch(&self, note: u8, ntt: u8, root: u8, quality: u8) -> Option<u8> {
        match note {
            0..=47 => None,
            48..=95 => {
                let lane = usize::from(note - 48);
                let index = (lane / 12 * 12 + usize::from(root)) * ROW
                    + usize::from(quality) * 12
                    + lane % 12;
                let pitch = self.guitar[usize::from(ntt)][index];
                (pitch < 128).then_some(pitch)
            }
            _ => Some(note),
        }
    }
}

#[cfg(test)]
mod configure_file_tests {
    use super::configure_file;

    #[test]
    fn rejects_a_file_that_is_not_the_genos_update_or_elf() {
        let dir = std::env::temp_dir().join(format!("gss-fw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not-firmware.bin");
        std::fs::write(&path, b"not firmware").unwrap();
        let error = configure_file(&path).unwrap_err().to_string();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(error.contains("Genos 2.13 update file"), "{error}");
    }

    #[test]
    fn reports_a_missing_file() {
        let error = configure_file(std::path::Path::new("Z:/missing/genos.prg"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("Cannot read the firmware file"), "{error}");
    }
}
