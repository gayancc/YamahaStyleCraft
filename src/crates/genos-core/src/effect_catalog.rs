//! Genos Effect Type List from the same fingerprinted Data List PDF as voices.
//! Local catalog; live system-effect transmission is separate from Style editing.
use crate::{Error, Result, voice_catalog};
use pdf_extract::{MediaBox, OutputDev, OutputError, Transform};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

const SCHEMA: &str = "genos.effect-catalog.v2";
const MAX_CACHE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Effect {
    /// `reverb`, `chorus`, or `variation` (Variation/Insertion block).
    pub block: String,
    pub category: String,
    pub name: String,
    pub description: String,
    pub msb: u8,
    pub lsb: u8,
    pub parameter_group: String,
    pub page: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub schema: String,
    pub edition: String,
    pub source_name: String,
    pub source_sha256: String,
    pub records_sha256: String,
    pub effects: Vec<Effect>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub edition: String,
    pub source_name: String,
    pub source_sha256: String,
    pub count: usize,
    pub reverb: usize,
    pub chorus: usize,
    pub variation: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    pub total: usize,
    pub effects: Vec<Effect>,
}

fn bad(s: &str) -> Error {
    Error::Format(s.into())
}

impl Catalog {
    pub fn summary(&self) -> Summary {
        Summary {
            edition: self.edition.clone(),
            source_name: self.source_name.clone(),
            source_sha256: self.source_sha256.clone(),
            count: self.effects.len(),
            reverb: self.effects.iter().filter(|e| e.block == "reverb").count(),
            chorus: self.effects.iter().filter(|e| e.block == "chorus").count(),
            variation: self
                .effects
                .iter()
                .filter(|e| e.block == "variation")
                .count(),
        }
    }

    pub fn search(&self, query: &str, block: Option<&str>) -> Result<Search> {
        if query.chars().count() > 120 {
            return Err(bad("effect search is limited to 120 characters"));
        }
        if let Some(b) = block
            && !matches!(b, "reverb" | "chorus" | "variation" | "")
        {
            return Err(bad("effect block must be reverb, chorus, or variation"));
        }
        let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut matches = Vec::new();
        for e in &self.effects {
            if let Some(b) = block.filter(|b| !b.is_empty())
                && e.block != b
            {
                continue;
            }
            let text = format!(
                "{} {} {} {} {} {} {}",
                e.block, e.category, e.name, e.description, e.msb, e.lsb, e.parameter_group
            )
            .to_lowercase();
            if terms.iter().all(|t| text.contains(t)) {
                matches.push(e.clone());
            }
        }
        let total = matches.len();
        matches.truncate(100);
        Ok(Search {
            total,
            effects: matches,
        })
    }

    fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA
            || self.edition != voice_catalog::EDITION
            || self.source_sha256 != voice_catalog::SOURCE_SHA256
            || self.effects.len() != 528
            || self.source_name.chars().count() > 160
        {
            return Err(bad(
                "unsupported or incomplete effect catalog; import the supported Data List again",
            ));
        }
        let mut addresses = HashSet::new();
        let (mut reverb, mut chorus, mut variation) = (0usize, 0usize, 0usize);
        for e in &self.effects {
            if e.msb > 127
                || e.lsb > 127
                || !(46..=53).contains(&e.page)
                || !matches!(e.block.as_str(), "reverb" | "chorus" | "variation")
                || !addresses.insert((e.block.clone(), e.msb, e.lsb))
            {
                return Err(bad("invalid or duplicate effect type address"));
            }
            for text in [&e.category, &e.name, &e.description, &e.parameter_group] {
                if text.chars().count() > 256 || text.chars().any(char::is_control) {
                    return Err(bad("invalid effect catalog text"));
                }
            }
            if e.name.is_empty() {
                return Err(bad("effect type name required"));
            }
            match e.block.as_str() {
                "reverb" => reverb += 1,
                "chorus" => chorus += 1,
                "variation" => variation += 1,
                _ => {}
            }
        }
        if reverb != 60 || chorus != 108 || variation != 360 {
            return Err(bad("effect type list extraction is incomplete"));
        }
        if crate::project::hash(&serde_json::to_vec(&self.effects)?) != self.records_sha256 {
            return Err(bad(
                "effect catalog checksum mismatch; import the Data List again",
            ));
        }
        Ok(())
    }
}

type Cells = [Vec<(f64, String)>; 6];
#[derive(Default)]
struct PageTable {
    block_hint: String,
    rows: BTreeMap<Reverse<i32>, Cells>,
    description_borders: Vec<f64>,
}

impl OutputDev for PageTable {
    fn fill(
        &mut self,
        t: &Transform,
        color_space: &pdf_extract::ColorSpace,
        color: &[f64],
        path: &pdf_extract::Path,
    ) -> std::result::Result<(), OutputError> {
        self.stroke(t, color_space, color, path)
    }
    fn stroke(
        &mut self,
        t: &Transform,
        _: &pdf_extract::ColorSpace,
        _: &[f64],
        path: &pdf_extract::Path,
    ) -> std::result::Result<(), OutputError> {
        let transform =
            |x: f64, y: f64| (x * t.m11 + y * t.m21 + t.m31, x * t.m12 + y * t.m22 + t.m32);
        let mut previous = None;
        for op in &path.ops {
            match *op {
                pdf_extract::PathOp::MoveTo(x, y) => previous = Some(transform(x, y)),
                pdf_extract::PathOp::LineTo(x, y) => {
                    let next = transform(x, y);
                    if let Some((px, py)) = previous
                        && (py - next.1).abs() < 0.1
                        && px.min(next.0) < 300.0
                        && px.max(next.0) > 300.0
                    {
                        self.description_borders.push(py);
                    }
                    previous = Some(next);
                }
                pdf_extract::PathOp::Rect(x, y, w, h) => {
                    let a = transform(x, y);
                    let b = transform(x + w, y + h);
                    if a.0.min(b.0) < 300.0 && a.0.max(b.0) > 300.0 {
                        self.description_borders.extend([a.1, b.1]);
                    }
                    previous = None;
                }
                _ => previous = None,
            }
        }
        Ok(())
    }
    fn begin_page(
        &mut self,
        _: u32,
        _: &MediaBox,
        _: Option<(f64, f64, f64, f64)>,
    ) -> std::result::Result<(), OutputError> {
        Ok(())
    }
    fn end_page(&mut self) -> std::result::Result<(), OutputError> {
        Ok(())
    }
    fn begin_word(&mut self) -> std::result::Result<(), OutputError> {
        Ok(())
    }
    fn end_word(&mut self) -> std::result::Result<(), OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> std::result::Result<(), OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        t: &Transform,
        _: f64,
        _: f64,
        _: f64,
        text: &str,
    ) -> std::result::Result<(), OutputError> {
        // Fingerprinted Effect Type List layout (pages 46–53), single wide table.
        let y = t.m32;
        let x = t.m31;
        // pdf-extract y grows upward (page bottom is low y). Bottom table rows sit near y≈50–80.
        if !(35.0..820.0).contains(&y) || !(40.0..560.0).contains(&x) {
            return Ok(());
        }
        // Block banners ("Reverb Block", …) appear in the top margin; do not
        // exclude table rows that also sit high on the page.
        if y > 780.0 {
            self.block_hint.push_str(text);
        }
        // Category | Type | Description | MSB | LSB | Parameter group
        let bounds = [85.0, 168.0, 428.0, 448.0, 470.0, 560.0];
        if let Some(column) = bounds.iter().position(|end| x < *end) {
            self.rows
                .entry(Reverse((y * 10.0).round() as i32))
                .or_default()[column]
                .push((x, text.into()));
        }
        Ok(())
    }
}

fn join_cell(parts: &[(f64, String)]) -> String {
    let mut ordered = parts.to_vec();
    ordered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    ordered
        .into_iter()
        .map(|(_, s)| s)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn detect_block(hint: &str) -> &'static str {
    let h = hint.to_lowercase();
    if h.contains("variation") && h.contains("insertion") {
        "variation"
    } else if h.contains("chorus block") {
        "chorus"
    } else if h.contains("reverb block") {
        "reverb"
    } else {
        "unknown"
    }
}

fn extract(bytes: &[u8], source_name: &str) -> Result<Catalog> {
    let doc = pdf_extract::Document::load_mem(bytes)
        .map_err(|e| bad(&format!("PDF could not be read: {e}")))?;
    if doc.get_pages().len() != 117 {
        return Err(bad("unsupported Data List page count"));
    }
    let mut effects = Vec::new();
    for page in 46u32..=53 {
        let mut table = PageTable::default();
        pdf_extract::output_doc_page(&doc, &mut table, page)
            .map_err(|e| bad(&format!("PDF page {page} extract failed: {e}")))?;
        let mut block = detect_block(&table.block_hint).to_string();
        // Descriptions can span several type rows. Their text is centered inside
        // the ruled cell, so proximity to a program's baseline is not reliable.
        let descriptions: Vec<_> = table
            .rows
            .iter()
            .filter(|(_, cells)| !cells[2].is_empty())
            .map(|(Reverse(y), cells)| (*y as f64 / 10.0, join_cell(&cells[2])))
            .collect();
        let borders = table.description_borders;
        if block == "unknown" {
            // Continuation pages keep the previous block; infer from first category-heavy rows.
            block = match page {
                46 => "reverb".into(),
                47 | 48 => "chorus".into(),
                _ => "variation".into(),
            };
        }
        // Merge character rows whose y buckets are within 2.5pt (wrapped cells / jitter).
        let mut merged: Vec<(i32, Cells)> = Vec::new();
        for (Reverse(y), cells) in table.rows {
            if let Some((last_y, last)) = merged.last_mut()
                && (*last_y - y).abs() <= 25
            {
                for i in 0..6 {
                    last[i].extend(cells[i].iter().cloned());
                }
                continue;
            }
            merged.push((y, cells));
        }
        let mut category = String::new();
        for (y, cells) in merged {
            let joined: Vec<String> = (0..6).map(|i| join_cell(&cells[i])).collect();
            if joined.iter().all(|s| s.is_empty()) {
                continue;
            }
            // Skip header repeats.
            if joined[0].eq_ignore_ascii_case("Category")
                || joined[1].eq_ignore_ascii_case("Type")
                || joined[3].eq_ignore_ascii_case("MSB")
            {
                continue;
            }
            if !joined[0].is_empty() {
                category = joined[0].clone();
            }
            let name = joined[1].clone();
            if name.is_empty() {
                continue;
            }
            // Wrapped description fragments and footer noise lack MSB/LSB cells.
            let (Ok(msb), Ok(lsb)) = (joined[3].parse::<u8>(), joined[4].parse::<u8>()) else {
                continue;
            };
            let baseline = y as f64 / 10.0;
            let top = borders
                .iter()
                .copied()
                .filter(|b| *b > baseline)
                .min_by(f64::total_cmp)
                .ok_or_else(|| bad("effect description cell has no upper border"))?;
            let bottom = borders
                .iter()
                .copied()
                .filter(|b| *b < baseline)
                .max_by(f64::total_cmp)
                .ok_or_else(|| bad("effect description cell has no lower border"))?;
            let description = descriptions
                .iter()
                .filter(|(y, _)| *y > bottom && *y < top)
                .map(|(_, s)| s.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            effects.push(Effect {
                block: block.clone(),
                category: category.clone(),
                name,
                description,
                msb,
                lsb,
                parameter_group: joined[5].clone(),
                page,
            });
        }
    }
    let catalog = Catalog {
        schema: SCHEMA.into(),
        edition: voice_catalog::EDITION.into(),
        source_name: source_name.chars().take(160).collect(),
        source_sha256: voice_catalog::SOURCE_SHA256.into(),
        records_sha256: crate::project::hash(&serde_json::to_vec(&effects)?),
        effects,
    };
    catalog.validate()?;
    Ok(catalog)
}

pub fn import_pdf(bytes: &[u8], source_name: &str) -> Result<Catalog> {
    if bytes.len() > voice_catalog::MAX_PDF_BYTES {
        return Err(bad("Data List exceeds the 32 MiB input limit"));
    }
    if crate::project::hash(bytes) != voice_catalog::SOURCE_SHA256 {
        return Err(Error::Unsupported(format!(
            "Choose the supported {} PDF. This file is a different edition, modified, or not a Genos Data List.",
            voice_catalog::EDITION
        )));
    }
    std::panic::catch_unwind(|| extract(bytes, source_name))
        .map_err(|_| bad("PDF effect extraction failed; existing catalog retained"))?
}

pub fn save(path: &Path, catalog: &Catalog) -> Result<()> {
    catalog.validate()?;
    crate::project::write_recovery(path, &serde_json::to_vec(catalog)?)
}

pub fn load(path: &Path) -> Result<Option<Catalog>> {
    if !path.exists() {
        return Ok(None);
    }
    if fs::metadata(path)?.len() > MAX_CACHE_BYTES {
        return Err(bad(
            "effect catalog exceeds size limit; import the Data List again",
        ));
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_CACHE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return Err(bad("effect catalog exceeds size limit"));
    }
    let catalog: Catalog = serde_json::from_slice(&bytes)?;
    catalog.validate()?;
    Ok(Some(catalog))
}
