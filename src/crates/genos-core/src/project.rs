use crate::{Error, Result, edit::Session, smf::MAX_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

/// Editor workspace fields stored under `.gss` extension key `uiState` (SFM-002).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUiState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    schema: String,
    name: String,
    sha256: String,
    style: Vec<u8>,
    #[serde(flatten)]
    extensions: serde_json::Map<String, serde_json::Value>,
}
pub fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
pub fn save_project(session: &Session, path: &Path) -> Result<()> {
    write_new(path, &session_bytes(session)?)
}
pub fn session_bytes(session: &Session) -> Result<Vec<u8>> {
    let style = session.document.style.bytes()?;
    Ok(serde_json::to_vec(&Project {
        schema: "genos.style-project.v1".into(),
        name: session.name.clone(),
        sha256: hash(&style),
        style,
        extensions: session.project_extensions.clone(),
    })?)
}
pub fn project_bytes(name: &str, style: &[u8]) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&Project {
        schema: "genos.style-project.v1".into(),
        name: name.into(),
        sha256: hash(style),
        style: style.to_vec(),
        extensions: Default::default(),
    })?)
}
pub fn open_file(path: &Path) -> Result<Session> {
    let limit = MAX_BYTES as u64 * 5;
    if fs::metadata(path)?.len() > limit {
        return Err(Error::Format("input exceeds size limit".into()));
    }
    let b = fs::read(path)?;
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gss"))
    {
        let p: Project = serde_json::from_slice(&b)?;
        if p.schema != "genos.style-project.v1" {
            return Err(Error::Unsupported("project schema/version".into()));
        }
        if hash(&p.style) != p.sha256 {
            return Err(Error::Format("project scaffold checksum mismatch".into()));
        }
        let mut session = Session::open(&p.style, p.name)?;
        session.project_extensions = p.extensions;
        Ok(session)
    } else {
        if !path.extension().is_some_and(|e| {
            ["sty", "prs", "sst", "pst", "fps", "mid"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        }) {
            return Err(Error::Unsupported(
                "select a Style, MIDI, or .gss project".into(),
            ));
        }
        Session::open(
            &b,
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
        )
    }
}
/// Create without overwrite. Persisting a flushed same-directory temporary file
/// with noclobber prevents both source replacement and a check/write race.
pub fn write_new(path: &Path, b: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(b)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)
        .map_err(|e| Error::Io(e.error))?;
    Ok(())
}
/// Atomic replacement for an application-owned recovery file only. User exports
/// deliberately use write_new, which cannot replace source music.
pub fn write_recovery(path: &Path, b: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Format("recovery path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(b)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Session;
    use std::path::Path;

    #[test]
    fn ui_state_roundtrips_in_project_bytes() {
        let style = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY"),
        )
        .expect("fixture");
        let mut session = Session::open(&style, "ui".into()).expect("open");
        session.set_ui_state(ProjectUiState {
            section_id: Some(1),
            channel: Some(11),
            snap: Some(16),
            zoom: Some(1.5),
            tool: Some("draw".into()),
        });
        let bytes = session_bytes(&session).expect("bytes");
        let reopened = {
            let dir = tempfile::tempdir().expect("tmp");
            let path = dir.path().join("t.gss");
            write_recovery(&path, &bytes).expect("write");
            open_file(&path).expect("reopen")
        };
        assert_eq!(
            reopened.project_extensions.get("uiState"),
            Some(&serde_json::json!({
                "sectionId": 1,
                "channel": 11,
                "snap": 16,
                "zoom": 1.5,
                "tool": "draw"
            }))
        );
        assert_eq!(
            reopened
                .view()
                .ui_state
                .as_ref()
                .and_then(|v| v.get("tool")),
            Some(&serde_json::json!("draw"))
        );
    }
}
