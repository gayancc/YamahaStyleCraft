//! Expansion pack recovery orchestration.

use super::n27::{self, InstrumentInfo};
use super::ppf::{self, PpfPackage, ResourceKind};
use super::ppi::{self, PpiPackage};
use crate::Error;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub installation: Option<PpiPackage>,
    pub instrument: Option<InstrumentInfo>,
    pub project: Option<PpfPackage>,
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
    /// `"ppf"` or `"ppi"`
    pub kind: String,
    pub resource_index: usize,
}

pub fn recover_pack(
    ppi_path: Option<&Path>,
    n27_path: Option<&Path>,
    ppf_path: Option<&Path>,
) -> Result<RecoveryReport, Error> {
    let mut notes = Vec::new();
    let mut blockers = Vec::new();
    let mut capabilities = Vec::new();

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

    if let Some(ref inst) = installation {
        let midi_items = inst
            .packs
            .iter()
            .flat_map(|p| p.items.iter())
            .filter(|i| i.fbin_kind == "midi")
            .count();
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
            detail: format!("{midi_items} MIDI FBIN payloads (bank/program setup)"),
        });
        let has_vwst = inst.sections.iter().any(|s| s.id == "VWST");
        if has_vwst {
            capabilities.push(Capability {
                category: "waveforms".into(),
                status: "present-undecoded".into(),
                detail: "VWST/VWDL high-entropy sections use IO::Secure::encryptbuf; samples not recovered from installation data yet (EV-0312/0313)".into(),
            });
            if project.is_none() {
                blockers.push(
                    "Editable pack reconstruction with original samples is blocked for installation-only recovery until VWST decryption is recovered, or a source .ppf is provided."
                        .into(),
                );
            }
        }
        if !inst.diagnostics.is_empty() {
            blockers.push(format!(
                "{} PACK/section diagnostic(s) — some containers may be incomplete",
                inst.diagnostics.len()
            ));
        }
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

    Ok(RecoveryReport {
        installation,
        instrument,
        project,
        capabilities,
        blockers,
        notes,
    })
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
        other => Err(Error::Format(format!("unsupported extract kind {other}"))),
    }
}
