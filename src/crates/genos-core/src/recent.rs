//! Application-owned recent files list (FEAT-011 / EV-0139, EV-0170, report04).
//!
//! Schema shape mirrors OBSERVED `recent-files.json` fields (`id`, `path`, `name`,
//! `kind`, `openedAt`) under an independent `genos.recent_files.v1` schema.
//! Path-id hashing is our stable helper (first 24 hex of SHA-256 over a
//! normalized path string); exact SFM path normalization was not isolated.

use crate::{Error, Result, project};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const SCHEMA: &str = "genos.recent_files.v1";
pub const FILE_NAME: &str = "recent-files.json";
const MAX_ENTRIES: usize = 10;
const MAX_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecentKind {
    Sty,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentEntry {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: RecentKind,
    pub opened_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentEntryView {
    pub entry: RecentEntry,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecentFile {
    schema: String,
    files: Vec<RecentEntry>,
}

/// Normalize a path string for stable recent-file identity hashing.
/// VERIFIED field set from report04; exact SFM normalization UNKNOWN — we fold
/// separators and trim trailing separators without inventing music algorithms.
pub fn normalize_path_key(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::with_capacity(raw.len());
    if raw.starts_with("\\\\") || raw.starts_with("//") {
        out.push('/');
    }
    let mut prev_sep = false;
    for ch in raw.chars() {
        let sep = ch == '/' || ch == '\\';
        if sep {
            if !prev_sep {
                out.push('/');
            }
            prev_sep = true;
            continue;
        }
        prev_sep = false;
        #[cfg(windows)]
        {
            out.push(ch.to_ascii_lowercase());
        }
        #[cfg(not(windows))]
        {
            out.push(ch);
        }
    }
    while out.ends_with('/')
        && out.len() > 1
        && out != "//"
        && !(out.len() == 3 && out.as_bytes()[1] == b':')
    {
        out.pop();
    }
    out
}

pub fn entry_id(path: &Path) -> String {
    let digest = Sha256::digest(normalize_path_key(path).as_bytes());
    format!("{digest:x}")[..24].to_string()
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn kind_for(path: &Path) -> Result<RecentKind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "gss" => Ok(RecentKind::Project),
        "sty" | "prs" | "sst" | "pst" | "fps" | "mid" => Ok(RecentKind::Sty),
        _ => Err(Error::Unsupported(
            "recent files accept Style, MIDI, or .gss project paths".into(),
        )),
    }
}

fn now_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Second precision is enough for home-list ordering; sub-second format
    // matched OBSERVED ISO strings when callers supply opened_at explicitly.
    format_unix_secs(secs)
}

fn format_unix_secs(secs: u64) -> String {
    // Minimal UTC formatter without pulling chrono (YYYY-MM-DDTHH:MM:SSZ).
    let days = secs / 86_400;
    let tod = secs % 86_400;
    let hour = tod / 3_600;
    let min = (tod % 3_600) / 60;
    let sec = tod % 60;
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Days since Unix epoch → Gregorian Y/M/D (Howard Hinnant algorithm).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn empty_store() -> RecentFile {
    RecentFile {
        schema: SCHEMA.into(),
        files: Vec::new(),
    }
}

fn read_store(path: &Path) -> Result<RecentFile> {
    if !path.is_file() {
        return Ok(empty_store());
    }
    let meta = fs::metadata(path)?;
    if meta.len() > MAX_BYTES as u64 {
        return Err(Error::Format("recent-files.json exceeds 64 KiB".into()));
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(empty_store());
    }
    let parsed: RecentFile = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => {
            // Corrupt file: fail soft with empty list (FEAT-011 errors UNKNOWN).
            return Ok(empty_store());
        }
    };
    if parsed.schema != SCHEMA {
        return Ok(empty_store());
    }
    Ok(RecentFile {
        schema: SCHEMA.into(),
        files: parsed.files.into_iter().take(MAX_ENTRIES).collect(),
    })
}

fn write_store(path: &Path, store: &RecentFile) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(store)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::Format("recent-files payload exceeds 64 KiB".into()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    project::write_recovery(path, &bytes)
}

pub fn recent_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(FILE_NAME)
}

/// List recent entries with on-disk availability (missing → unavailable, not removed).
pub fn list(app_data_dir: &Path) -> Result<Vec<RecentEntryView>> {
    let store = read_store(&recent_path(app_data_dir))?;
    Ok(store
        .files
        .into_iter()
        .map(|entry| {
            let available = Path::new(&entry.path).is_file();
            RecentEntryView { entry, available }
        })
        .collect())
}

/// Record a successful open/save. Dedupes by id, newest first, caps at 10.
pub fn record(app_data_dir: &Path, path: &Path) -> Result<Vec<RecentEntryView>> {
    record_at(app_data_dir, path, &now_iso8601())
}

pub fn record_at(
    app_data_dir: &Path,
    path: &Path,
    opened_at: &str,
) -> Result<Vec<RecentEntryView>> {
    let kind = kind_for(path)?;
    let path_str = path.to_string_lossy().into_owned();
    let entry = RecentEntry {
        id: entry_id(path),
        path: path_str,
        name: display_name(path),
        kind,
        opened_at: opened_at.to_string(),
    };
    let store_path = recent_path(app_data_dir);
    let mut store = read_store(&store_path)?;
    store.files.retain(|e| e.id != entry.id);
    store.files.insert(0, entry);
    store.files.truncate(MAX_ENTRIES);
    write_store(&store_path, &store)?;
    list(app_data_dir)
}

/// Remove one entry by id without crashing if already gone.
pub fn remove(app_data_dir: &Path, id: &str) -> Result<Vec<RecentEntryView>> {
    let store_path = recent_path(app_data_dir);
    let mut store = read_store(&store_path)?;
    let before = store.files.len();
    store.files.retain(|e| e.id != id);
    if store.files.len() != before {
        write_store(&store_path, &store)?;
    }
    list(app_data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_folds_separators_and_trailing_slash() {
        let a = normalize_path_key(Path::new(r"C:\Styles\Main.STY"));
        let b = normalize_path_key(Path::new(r"C:/Styles/Main.STY/"));
        assert_eq!(a, b);
    }

    #[test]
    fn entry_id_is_24_hex() {
        let id = entry_id(Path::new(r"D:\music\demo.STY"));
        assert_eq!(id.len(), 24);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
    #[test]
    fn normalization_preserves_root_and_unc_identity() {
        assert_ne!(
            entry_id(Path::new("/Styles/Main.STY")),
            entry_id(Path::new("Styles/Main.STY"))
        );
        assert_ne!(
            entry_id(Path::new(r"\\server\share\Main.STY")),
            entry_id(Path::new(r"server\share\Main.STY"))
        );
        assert!(normalize_path_key(Path::new(r"\\server\share\Main.STY")).starts_with("//"));
    }
}
