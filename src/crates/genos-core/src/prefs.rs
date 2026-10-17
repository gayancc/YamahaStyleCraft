//! Lightweight editor preferences. Soft-fail on corrupt files.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const SCHEMA: &str = "genos.prefs.v1";
const MAX_BYTES: u64 = 32 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    pub schema: String,
    #[serde(default)]
    pub midi_output_name: Option<String>,
    #[serde(default)]
    pub midi_output_index: Option<usize>,
    #[serde(default = "default_snap")]
    pub snap: u32,
    #[serde(default = "default_zoom")]
    pub zoom: f64,
    #[serde(default = "default_tool")]
    pub tool: String,
}

fn default_snap() -> u32 {
    16
}
fn default_zoom() -> f64 {
    1.0
}
fn default_tool() -> String {
    "select".into()
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            schema: SCHEMA.into(),
            midi_output_name: None,
            midi_output_index: None,
            snap: default_snap(),
            zoom: default_zoom(),
            tool: default_tool(),
        }
    }
}

pub fn load(path: &Path) -> Result<Prefs> {
    if !path.exists() {
        return Ok(Prefs::default());
    }
    if fs::metadata(path)?.len() > MAX_BYTES {
        return Ok(Prefs::default());
    }
    let raw = fs::read(path)?;
    match serde_json::from_slice::<Prefs>(&raw) {
        Ok(p) if p.schema == SCHEMA => Ok(sanitize(p)),
        _ => Ok(Prefs::default()),
    }
}

fn sanitize(mut p: Prefs) -> Prefs {
    if !(1..=64).contains(&p.snap) {
        p.snap = default_snap();
    }
    if !p.zoom.is_finite() || !(0.125..=8.0).contains(&p.zoom) {
        p.zoom = default_zoom();
    }
    if p.tool != "select" && p.tool != "draw" {
        p.tool = default_tool();
    }
    p.schema = SCHEMA.into();
    p
}

pub fn save(path: &Path, prefs: &Prefs) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Format("prefs path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut out = sanitize(prefs.clone());
    out.schema = SCHEMA.into();
    crate::project::write_recovery(path, &serde_json::to_vec_pretty(&out)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn corrupt_prefs_soft_fail_and_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        fs::write(&path, b"{not json").unwrap();
        assert_eq!(load(&path).unwrap(), Prefs::default());
        let p = Prefs {
            midi_output_name: Some("Digital Workstation-1".into()),
            snap: 8,
            tool: "draw".into(),
            ..Prefs::default()
        };
        save(&path, &p).unwrap();
        assert_eq!(
            load(&path).unwrap().midi_output_name.as_deref(),
            Some("Digital Workstation-1")
        );
        assert_eq!(load(&path).unwrap().tool, "draw");
    }
}
