//! Expansion pack recovery orchestration.

use super::n27::{self, InstrumentInfo};
use super::ppf::{self, PpfPackage, ResourceKind};
use super::ppi::{self, PpiPackage};
use super::secure::{self, SEED_LEN};
use super::voice_map::{self, PackVoiceMap};
use super::vprs::{self, VprsInfo};
use super::wave_index::{self, WaveIndex};
use crate::Error;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub installation: Option<PpiPackage>,
    pub instrument: Option<InstrumentInfo>,
    pub project: Option<PpfPackage>,
    pub vprs: Option<VprsInfo>,
    pub wave_index: Option<WaveIndex>,
    pub voice_maps: Vec<PackVoiceMap>,
    pub capabilities: Vec<Capability>,
    pub blockers: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub category: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRequest {
    pub source_path: String,
    pub dest_dir: String,
    /// `"ppf"`, `"ppi"`, or `"vwst"`
    pub kind: String,
    pub resource_index: usize,
    /// Optional 32-char hex SME seed for `vwst` extracts (EV-0316).
    #[serde(default)]
    pub encryption_key_hex: Option<String>,
    /// Byte offset into VWST payload (from wave index `startAddress`).
    #[serde(default)]
    pub vwst_offset: Option<u64>,
    /// Byte length to read from VWST payload.
    #[serde(default)]
    pub vwst_size: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct RecoverOptions {
    pub wave_index_path: Option<PathBuf>,
    pub encryption_key_hex: Option<String>,
}

pub fn recover_pack(
    ppi_path: Option<&Path>,
    n27_path: Option<&Path>,
    ppf_path: Option<&Path>,
) -> Result<RecoveryReport, Error> {
    recover_pack_with_options(ppi_path, n27_path, ppf_path, RecoverOptions::default())
}

pub fn recover_pack_with_options(
    ppi_path: Option<&Path>,
    n27_path: Option<&Path>,
    ppf_path: Option<&Path>,
    options: RecoverOptions,
) -> Result<RecoveryReport, Error> {
    let mut notes = Vec::new();
    let mut blockers = Vec::new();
    let mut capabilities = Vec::new();
    let mut voice_maps = Vec::new();

    let instrument = match n27_path {
        Some(p) => Some(n27::parse_n27_file(p)?),
        None => {
            notes.push(
                ".n27 not supplied — optional for reading installation/project files; required by YEM when generating new installation data for a specific Genos (EV-0315)."
                    .into(),
            );
            None
        }
    };

    let installation = match ppi_path {
        Some(p) => Some(ppi::parse_ppi(p)?),
        None => None,
    };

    let project = match ppf_path {
        Some(p) => Some(ppf::parse_ppf_file(p)?),
        None => None,
    };

    let wave_index = match options.wave_index_path.as_deref() {
        Some(p) => Some(wave_index::parse_wave_index_file(p)?),
        None => None,
    };

    let mut vprs = None;

    if let Some(ref inst) = installation {
        let midi_items: Vec<_> = inst
            .packs
            .iter()
            .flat_map(|p| p.items.iter().map(move |i| (p, i)))
            .filter(|(_, i)| i.fbin_kind == "midi")
            .collect();
        capabilities.push(Capability {
            category: "pack-metadata".into(),
            status: "verified-recoverable".into(),
            detail: format!(
                "{} PACK containers, model={:?}",
                inst.packs.len(),
                inst.header.model
            ),
        });
        capabilities.push(Capability {
            category: "voice-drum-midi-setup".into(),
            status: "verified-recoverable".into(),
            detail: format!(
                "{} MIDI FBIN payloads (bank/program setup)",
                midi_items.len()
            ),
        });

        let ppi_path_ref = Path::new(&inst.path);
        let mut address_total = 0usize;
        for (pack, item) in midi_items.iter().take(64) {
            match ppi::read_pack_item_bytes(ppi_path_ref, item) {
                Ok(bytes) => match voice_map::scan_midi_voice_addresses(&bytes) {
                    Ok(addresses) if !addresses.is_empty() => {
                        address_total += addresses.len();
                        voice_maps.push(PackVoiceMap {
                            pack_title: pack.title.clone(),
                            item_title: item.title.clone(),
                            addresses,
                        });
                    }
                    Ok(_) => {}
                    Err(e) => notes.push(format!(
                        "MIDI voice scan skipped for {:?}: {e}",
                        item.title
                    )),
                },
                Err(e) => notes.push(format!("MIDI read failed for {:?}: {e}", item.title)),
            }
        }
        if address_total > 0 {
            capabilities.push(Capability {
                category: "expansion-voice-map".into(),
                status: "verified-recoverable".into(),
                detail: format!(
                    "{address_total} bank/program addresses from {} MIDI setup(s) — available for voice picker",
                    voice_maps.len()
                ),
            });
        }

        if let Some(section) = inst.sections.iter().find(|s| s.id == "VPRS") {
            match vprs::summarize_vprs(Path::new(&inst.path), section.offset, section.size) {
                Ok(info) => {
                    let header_note = info
                        .header
                        .as_ref()
                        .map(|h| format!("data_base=0x{:x}", h.data_base))
                        .unwrap_or_else(|| "no header".into());
                    capabilities.push(Capability {
                        category: "voice-parameters".into(),
                        status: "partially-recovered".into(),
                        detail: format!(
                            "VPRS banner={:?}; {header_note}; per-voice fields incomplete (EV-0317)",
                            info.banner
                        ),
                    });
                    vprs = Some(info);
                }
                Err(e) => notes.push(format!("VPRS summarize failed: {e}")),
            }
        }

        let has_vwst = inst.sections.iter().any(|s| s.id == "VWST");
        let key_status = match options.encryption_key_hex.as_deref().map(str::trim) {
            None | Some("") => Ok(false),
            Some(hex) => parse_seed_hex(Some(hex)).map(|_| true),
        };
        if has_vwst {
            match key_status {
                Ok(true) => {
                    capabilities.push(Capability {
                        category: "waveforms".into(),
                        status: "keyed-decrypt-ready".into(),
                        detail: "SME stream cipher implemented (EV-0316); offline seed provided — VWST extract available via kind=vwst".into(),
                    });
                }
                Ok(false) => {
                    capabilities.push(Capability {
                        category: "waveforms".into(),
                        status: "present-undecoded".into(),
                        detail: "VWST/VWDL use IO::Secure::sme_encryptbuf (16-byte seed → 128-byte XOR table). Offline Model Encryption supplies the seed into the encrypt context (+0x10 / CESC); not .n27 alone. Seed bytes still unidentified (EV-0316)".into(),
                    });
                    if project.is_none() {
                        blockers.push(
                            "Editable pack reconstruction with original samples is blocked for installation-only recovery until the offline SME seed is supplied (or a source .ppf is provided)."
                                .into(),
                        );
                    }
                }
                Err(e) => {
                    capabilities.push(Capability {
                        category: "waveforms".into(),
                        status: "present-undecoded".into(),
                        detail: "VWST present but encryptionkey could not be parsed".into(),
                    });
                    blockers.push(format!("encryptionkey invalid: {e}"));
                }
            }
        }
        if !inst.diagnostics.is_empty() {
            blockers.push(format!(
                "{} PACK/section diagnostic(s) — some containers may be incomplete",
                inst.diagnostics.len()
            ));
        }
    }

    if let Some(ref idx) = wave_index {
        capabilities.push(Capability {
            category: "yem-wave-index".into(),
            status: "verified-recoverable".into(),
            detail: format!(
                "{} wave entries with VWST byte offsets (frame<<4==start) from {}",
                idx.entries.len(),
                idx.path
            ),
        });
        notes.push(idx.note.clone());
    }

    if let Some(ref proj) = project {
        let aiffs = proj
            .resources
            .iter()
            .filter(|r| r.kind == ResourceKind::Aiff)
            .count();
        let uvfs = proj
            .resources
            .iter()
            .filter(|r| r.kind == ResourceKind::UvfXml)
            .count();
        capabilities.push(Capability {
            category: "ppf-waveforms".into(),
            status: "verified-recoverable".into(),
            detail: format!("{aiffs} AIFF samples extractable losslessly (EV-0311)"),
        });
        capabilities.push(Capability {
            category: "ppf-voices".into(),
            status: "verified-recoverable".into(),
            detail: format!("{uvfs} UVF XML voice definitions"),
        });
    }

    if installation.is_none() && project.is_none() {
        return Err(Error::Format(
            "provide at least a .ppi/.pqi installation file or a .ppf project".into(),
        ));
    }

    if let Some(ref info) = instrument {
        notes.push(format!(
            "Instrument {} / {} — wave ROM size fields are host constraints for pack generation, not proven PPI decryption keys (EV-0315).",
            info.model_name, info.composite_id
        ));
    }

    notes.push(
        "SME algorithm recovered from Expansion Manager (EV-0316). Provide the offline 16-byte SME seed (32 hex chars) to decrypt VWST slices."
            .into(),
    );

    if project.is_some() {
        capabilities.push(Capability {
            category: "ppf-rewrite".into(),
            status: "structural-roundtrip".into(),
            detail: "write_ppf/rewrite_ppf can rebuild XPFH+BLOB structure from extracted resources; YEM import not yet verified".into(),
        });
    }

    Ok(RecoveryReport {
        installation,
        instrument,
        project,
        vprs,
        wave_index,
        voice_maps,
        capabilities,
        blockers,
        notes,
    })
}

fn parse_seed_hex(hex: Option<&str>) -> Result<[u8; SEED_LEN], Error> {
    let Some(hex) = hex.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(Error::Format("encryption key not provided".into()));
    };
    let cleaned: String = hex.chars().filter(|c| *c != ' ' && *c != '-').collect();
    if cleaned.len() != SEED_LEN * 2 {
        return Err(Error::Format(format!(
            "encryption key must be {} hex bytes ({} hex chars), got {}",
            SEED_LEN,
            SEED_LEN * 2,
            cleaned.len()
        )));
    }
    let mut seed = [0u8; SEED_LEN];
    for (i, chunk) in cleaned.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk)
            .map_err(|_| Error::Format("encryption key is not valid hex".into()))?;
        seed[i] = u8::from_str_radix(s, 16)
            .map_err(|_| Error::Format("encryption key is not valid hex".into()))?;
    }
    Ok(seed)
}

fn resolve_dest_dir(dest: &Path) -> Result<PathBuf, Error> {
    std::fs::create_dir_all(dest)?;
    let canonical = dest
        .canonicalize()
        .map_err(|e| Error::Format(format!("dest_dir: {e}")))?;
    if !canonical.is_dir() {
        return Err(Error::Format("dest_dir is not a directory".into()));
    }
    Ok(canonical)
}

fn extract_vwst_slice(req: &ExtractRequest) -> Result<PathBuf, Error> {
    let seed = parse_seed_hex(req.encryption_key_hex.as_deref())?;
    let offset = req
        .vwst_offset
        .ok_or_else(|| Error::Format("vwst_offset required for kind=vwst".into()))?;
    let size = req
        .vwst_size
        .ok_or_else(|| Error::Format("vwst_size required for kind=vwst".into()))?;
    if size == 0 || size > 64 * 1024 * 1024 {
        return Err(Error::Format("vwst_size out of allowed range".into()));
    }

    let source = Path::new(&req.source_path);
    let dest = resolve_dest_dir(Path::new(&req.dest_dir))?;
    let pkg = ppi::parse_ppi(source)?;
    let vwst = pkg
        .sections
        .iter()
        .find(|s| s.id == "VWST")
        .ok_or_else(|| Error::Format("installation has no VWST section".into()))?;

    let payload_base = vwst
        .offset
        .checked_add(8)
        .ok_or_else(|| Error::Format("VWST payload base overflow".into()))?;
    let payload_end = payload_base
        .checked_add(vwst.size as u64)
        .ok_or_else(|| Error::Format("VWST payload end overflow".into()))?;
    let abs = payload_base
        .checked_add(offset)
        .ok_or_else(|| Error::Format("VWST slice offset overflow".into()))?;
    let end = abs
        .checked_add(size)
        .ok_or_else(|| Error::Format("VWST slice end overflow".into()))?;
    if end > payload_end {
        return Err(Error::Format(
            "requested VWST slice exceeds section bounds".into(),
        ));
    }
    if req.resource_index != 0 {
        return Err(Error::Format(
            "kind=vwst ignores resource_index; pass 0".into(),
        ));
    }

    let mut file = File::open(source)?;
    file.seek(SeekFrom::Start(abs))?;
    let mut cipher = vec![0u8; size as usize];
    file.read_exact(&mut cipher)?;
    let counter = (offset & 0x7f) as u16;
    let plain = secure::decrypt_sme(&cipher, &seed, counter)?;

    let name = format!("vwst_{offset:x}_{size:x}.bin");
    let out = dest.join(name);
    std::fs::write(&out, plain)?;
    Ok(out)
}

pub fn extract_resource(req: &ExtractRequest) -> Result<PathBuf, Error> {
    let source = Path::new(&req.source_path);
    let dest = resolve_dest_dir(Path::new(&req.dest_dir))?;
    match req.kind.as_str() {
        "ppf" => {
            let pkg = ppf::parse_ppf_file(source)?;
            let data = std::fs::read(source)?;
            let res = pkg
                .resources
                .get(req.resource_index)
                .ok_or_else(|| Error::Format("resource index out of range".into()))?;
            let ext = match res.kind {
                ResourceKind::UvfXml => "xml",
                ResourceKind::Aiff => "aif",
                _ => "bin",
            };
            let name = format!(
                "{}_{}.{}",
                req.resource_index,
                res.title
                    .as_deref()
                    .unwrap_or("resource")
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect::<String>(),
                ext
            );
            ppf::extract_resource(&data, res, &dest, &name)
        }
        "ppi" => {
            let pkg = ppi::parse_ppi(source)?;
            let mut flat = Vec::new();
            for pack in &pkg.packs {
                for item in &pack.items {
                    flat.push(item.clone());
                }
            }
            let item = flat
                .get(req.resource_index)
                .ok_or_else(|| Error::Format("resource index out of range".into()))?;
            let ext = if item.fbin_kind == "midi" {
                "mid"
            } else {
                "bin"
            };
            let name = format!(
                "{}_{}.{}",
                req.resource_index,
                item.title
                    .as_deref()
                    .unwrap_or("item")
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect::<String>(),
                ext
            );
            ppi::extract_pack_item(source, item, &dest, &name)
        }
        "vwst" => extract_vwst_slice(req),
        other => Err(Error::Format(format!("unsupported extract kind {other}"))),
    }
}
