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
    /// Interface chrome scale (SFM Settings A− / A+ / 100%). Independent of piano-roll zoom.
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f64,
    #[serde(default = "default_tool")]
    pub tool: String,
    /// When true, Stop parks the audition cursor at the section start (SFM Ctrl+W default).
    #[serde(default = "default_true")]
    pub return_on_stop: bool,
    /// When true, mixer CC writes apply to every section for the channel (SFM mixer scope).
    #[serde(default)]
    pub mixer_apply_all_sections: bool,
    /// When true, play a PC-audio click on each beat during section playback (SFM metronome).
    #[serde(default)]
    pub metronome_during_playback: bool,
    /// Count-in bars before MIDI capture starts (0..=8).
    #[serde(default)]
    pub count_in_bars: u8,
    /// Capture apply quantize step in ticks (0 = off). Common musical grids only.
    #[serde(default)]
    pub rec_quantize: u32,
    /// SFM Record data mode: `all` | `notes` | `controllers`.
    #[serde(default = "default_rec_data")]
    pub rec_data: String,
    /// Capture input channel filter: 0 = OMNI, 1..=16 = single channel (SFM CHANNEL).
    #[serde(default)]
    pub rec_input_channel: u8,
    /// Forward captured MIDI to the selected output while recording.
    #[serde(default)]
    pub midi_thru: bool,
    /// Recording latency compensation in ms (positive pulls notes earlier; SFM-044).
    #[serde(default)]
    pub recording_offset_ms: i32,
    /// Favorite Style Part Library file paths (SFM-LIB-FAVORITES).
    #[serde(default)]
    pub library_favorites: Vec<String>,
    /// Per-channel (1..=16) piano-roll display colors (SFM Note colors). Display-only.
    #[serde(default)]
    pub note_colors: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_snap() -> u32 {
    16
}
fn default_zoom() -> f64 {
    1.0
}
fn default_ui_scale() -> f64 {
    1.0
}
fn default_tool() -> String {
    "select".into()
}
fn default_rec_data() -> String {
    "all".into()
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            schema: SCHEMA.into(),
            midi_output_name: None,
            midi_output_index: None,
            snap: default_snap(),
            zoom: default_zoom(),
            ui_scale: default_ui_scale(),
            tool: default_tool(),
            return_on_stop: true,
            mixer_apply_all_sections: false,
            metronome_during_playback: false,
            count_in_bars: 0,
            rec_quantize: 0,
            rec_data: default_rec_data(),
            rec_input_channel: 0,
            midi_thru: false,
            recording_offset_ms: 0,
            library_favorites: Vec::new(),
            note_colors: Vec::new(),
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
    if ![0, 1, 2, 4, 6, 8, 12, 16, 24, 32, 48, 64].contains(&p.snap) {
        p.snap = default_snap();
    }
    if !p.zoom.is_finite() || !(0.125..=8.0).contains(&p.zoom) {
        p.zoom = default_zoom();
    }
    if !p.ui_scale.is_finite() || !(0.85..=1.25).contains(&p.ui_scale) {
        p.ui_scale = default_ui_scale();
    } else {
        // Snap to 0.05 steps (SFM A− / 100% / A+ family).
        p.ui_scale = (p.ui_scale * 20.0).round() / 20.0;
        p.ui_scale = p.ui_scale.clamp(0.85, 1.25);
    }
    if !["select", "draw", "paint", "erase"].contains(&p.tool.as_str()) {
        p.tool = default_tool();
    }
    if p.count_in_bars > 8 {
        p.count_in_bars = 0;
    }
    if ![
        0, 1, 2, 4, 8, 12, 16, 24, 32, 48, 64, 96, 120, 160, 240, 480,
    ]
    .contains(&p.rec_quantize)
    {
        p.rec_quantize = 0;
    }
    if !["all", "notes", "controllers"].contains(&p.rec_data.as_str()) {
        p.rec_data = default_rec_data();
    }
    if p.rec_input_channel > 16 {
        p.rec_input_channel = 0;
    }
    if !(-2000..=2000).contains(&p.recording_offset_ms) {
        p.recording_offset_ms = 0;
    }
    p.library_favorites
        .retain(|s| !s.is_empty() && s.len() < 1024);
    p.library_favorites.truncate(64);
    p.note_colors = sanitize_note_colors(&p.note_colors);
    p.schema = SCHEMA.into();
    p
}

/// Pad/truncate to 16 `#RRGGBB` channel colors; invalid entries cleared (UI fills defaults).
fn sanitize_note_colors(raw: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(16);
    for i in 0..16 {
        let ok = raw
            .get(i)
            .map(|s| {
                let b = s.as_bytes();
                b.len() == 7
                    && b[0] == b'#'
                    && b[1..].iter().all(|c| c.is_ascii_hexdigit())
            })
            .unwrap_or(false);
        out.push(if ok {
            raw[i].clone()
        } else {
            String::new()
        });
    }
    out
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
            return_on_stop: false,
            mixer_apply_all_sections: true,
            ui_scale: 1.1,
            ..Prefs::default()
        };
        save(&path, &p).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(
            loaded.midi_output_name.as_deref(),
            Some("Digital Workstation-1")
        );
        assert_eq!(loaded.tool, "draw");
        assert!(!loaded.return_on_stop);
        assert!(loaded.mixer_apply_all_sections);
        assert!((loaded.ui_scale - 1.1).abs() < f64::EPSILON);
    }

    #[test]
    fn ui_scale_sanitizes_out_of_range() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        fs::write(
            &path,
            br#"{"schema":"genos.prefs.v1","uiScale":2.5,"snap":16,"zoom":1,"tool":"select"}"#,
        )
        .unwrap();
        assert!((load(&path).unwrap().ui_scale - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rec_input_channel_roundtrip_and_sanitize() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let p = Prefs {
            rec_data: "notes".into(),
            rec_input_channel: 12,
            ..Prefs::default()
        };
        save(&path, &p).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.rec_data, "notes");
        assert_eq!(loaded.rec_input_channel, 12);
        fs::write(
            &path,
            br#"{"schema":"genos.prefs.v1","recInputChannel":99,"snap":16,"zoom":1,"tool":"select"}"#,
        )
        .unwrap();
        assert_eq!(load(&path).unwrap().rec_input_channel, 0);
    }

    #[test]
    fn note_colors_sanitizes_length_and_hex() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let mut colors = vec!["#ff0000".into(), "nope".into()];
        colors.extend((0..20).map(|_| "#00ff00".into()));
        let p = Prefs {
            note_colors: colors,
            ..Prefs::default()
        };
        save(&path, &p).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.note_colors.len(), 16);
        assert_eq!(loaded.note_colors[0], "#ff0000");
        assert_eq!(loaded.note_colors[1], "");
        assert_eq!(loaded.note_colors[2], "#00ff00");
    }
}
