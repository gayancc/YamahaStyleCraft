//! Local-only catalog extracted from the independently inspected Genos v2.00 Data List.
//! No Yamaha voice catalog is bundled. Other editions require a verified extraction profile.
use crate::{
    Error, Result,
    document::Document,
    smf::{Event, Style},
};
use pdf_extract::{MediaBox, OutputDev, OutputError, Transform};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

pub const MAX_PDF_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 2 * 1024 * 1024;
pub const SOURCE_SHA256: &str = "6022804163e4548abfd26db1889307d78f913297a3cc78b6cf6905bb3060a9bd";
pub const EDITION: &str = "Genos v2.00 · English Data List · 11/2019";
const SCHEMA: &str = "genos.voice-catalog.v1";
/// Verified Data List category for kit program addresses (F019 kit picker scope).
pub const DRUM_KIT_CATEGORY: &str = "DrumKit";
/// Shell-observed Genos drum preview note bounds (findings/02); not a kit map.
pub const DRUM_PREVIEW_NOTE_MIN: u8 = 13;
pub const DRUM_PREVIEW_NOTE_MAX: u8 = 91;
pub const DRUM_CHANNELS: [u8; 2] = [9, 10];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Address {
    pub msb: u8,
    pub lsb: u8,
    pub program: u8,
}
impl Address {
    pub fn id(self) -> u32 {
        (u32::from(self.msb) << 14) | (u32::from(self.lsb) << 7) | u32::from(self.program)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Voice {
    pub category: String,
    pub subcategory: String,
    pub name: String,
    pub msb: u8,
    pub lsb: u8,
    pub program: u8,
    pub voice_type: String,
    pub page: u32,
}
impl Voice {
    pub fn address(&self) -> Address {
        Address {
            msb: self.msb,
            lsb: self.lsb,
            program: self.program,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub schema: String,
    pub edition: String,
    pub source_name: String,
    pub source_sha256: String,
    pub records_sha256: String,
    pub voices: Vec<Voice>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub edition: String,
    pub source_name: String,
    pub source_sha256: String,
    pub count: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    pub total: usize,
    pub voices: Vec<Voice>,
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
            count: self.voices.len(),
        }
    }
    pub fn find(&self, address: Address) -> Option<&Voice> {
        self.voices.iter().find(|v| v.address() == address)
    }
    pub fn search(&self, query: &str) -> Result<Search> {
        self.search_in(self.voices.iter(), query)
    }
    /// DrumKit category rows from the verified Data List (not SysEx Drum Setup).
    pub fn drum_kits(&self) -> Vec<Voice> {
        let mut kits: Vec<_> = self
            .voices
            .iter()
            .filter(|v| v.category == DRUM_KIT_CATEGORY)
            .cloned()
            .collect();
        kits.sort_by_key(|a| a.name.to_lowercase());
        kits
    }
    pub fn search_drum_kits(&self, query: &str) -> Result<Search> {
        self.search_in(self.drum_kits().iter(), query)
    }
    fn search_in<'a>(
        &self,
        voices: impl Iterator<Item = &'a Voice>,
        query: &str,
    ) -> Result<Search> {
        if query.chars().count() > 120 {
            return Err(bad("voice search is limited to 120 characters"));
        }
        let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut matches = Vec::new();
        for v in voices {
            let text = format!(
                "{} {} {} {} {} {} {}",
                v.name,
                v.category,
                v.subcategory,
                v.voice_type,
                v.msb,
                v.lsb,
                v.program + 1
            )
            .to_lowercase();
            if terms.iter().all(|t| text.contains(t)) {
                matches.push(v.clone());
            }
        }
        let total = matches.len();
        matches.truncate(100);
        Ok(Search {
            total,
            voices: matches,
        })
    }
    fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA
            || self.edition != EDITION
            || self.source_sha256 != SOURCE_SHA256
            || self.voices.len() != 1711
            || self.source_name.chars().count() > 160
        {
            return Err(bad(
                "unsupported or incomplete voice catalog; import the supported Data List again",
            ));
        }
        let mut addresses = HashSet::new();
        let mut pages = [0usize; 12];
        for v in &self.voices {
            if v.msb > 127
                || v.lsb > 127
                || v.program > 127
                || !(3..=14).contains(&v.page)
                || !addresses.insert(v.address().id())
            {
                return Err(bad("invalid or duplicate catalog voice address"));
            }
            for text in [&v.name, &v.category, &v.subcategory, &v.voice_type] {
                if text.is_empty()
                    || text.chars().count() > 128
                    || text.chars().any(char::is_control)
                {
                    return Err(bad("invalid voice catalog text"));
                }
            }
            pages[(v.page - 3) as usize] += 1;
        }
        if pages[..11].iter().any(|n| *n != 150) || pages[11] != 61 {
            return Err(bad("voice list extraction is incomplete"));
        }
        if crate::project::hash(&serde_json::to_vec(&self.voices)?) != self.records_sha256 {
            return Err(bad(
                "voice catalog checksum mismatch; import the Data List again",
            ));
        }
        Ok(())
    }
}

type Cells = [Vec<(f64, String)>; 7];
#[derive(Default)]
struct Table {
    rows: BTreeMap<(u8, Reverse<i32>), Cells>,
}
impl OutputDev for Table {
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
        // Coordinates belong only to the fingerprinted 117-page edition. Individual
        // cells are essential: plain-text extraction can collapse adjacent bank digits.
        if !(45.0..752.0).contains(&t.m32) || !(42.48..554.17).contains(&t.m31) {
            return Ok(());
        }
        let side = u8::from(t.m31 >= 300.0);
        let x = t.m31 - f64::from(side) * 262.2;
        if !(42.48..291.97).contains(&x) {
            return Ok(());
        }
        let bounds = [90.69, 136.05, 198.39, 215.43, 232.41, 249.45, 291.97];
        if let Some(column) = bounds.iter().position(|end| x < *end) {
            self.rows
                .entry((side, Reverse((t.m32 * 10.0).round() as i32)))
                .or_default()[column]
                .push((x, text.into()));
        }
        Ok(())
    }
}
fn extract(bytes: &[u8], source_name: &str) -> Result<Catalog> {
    let doc = pdf_extract::Document::load_mem(bytes)
        .map_err(|e| bad(&format!("PDF could not be read: {e}")))?;
    if doc.get_pages().len() != 117 {
        return Err(bad("unsupported Data List page count"));
    }
    let mut voices = Vec::new();
    for page in 3..=14 {
        let mut table = Table::default();
        pdf_extract::output_doc_page(&doc, &mut table, page)
            .map_err(|e| bad(&format!("PDF page {page} could not be extracted: {e}")))?;
        let mut side = 2;
        let mut category = String::new();
        let mut subcategory = String::new();
        for ((next_side, _), cells) in table.rows {
            if side != next_side {
                side = next_side;
                category.clear();
                subcategory.clear();
            }
            let cells: Vec<String> = cells
                .into_iter()
                .map(|mut glyphs| {
                    glyphs.sort_by(|a, b| a.0.total_cmp(&b.0));
                    glyphs
                        .into_iter()
                        .map(|(_, s)| s)
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect();
            if !cells[0].is_empty() {
                category = cells[0].clone();
            }
            if !cells[1].is_empty() {
                subcategory = cells[1].clone();
            }
            if cells[2..].iter().all(|s| s.is_empty()) {
                continue;
            }
            let number = |index: usize| {
                cells[index]
                    .parse::<u8>()
                    .map_err(|_| bad(&format!("unreadable voice address on PDF page {page}")))
            };
            let program = number(5)?
                .checked_sub(1)
                .ok_or_else(|| bad("Data List program numbers must be 1..128"))?;
            voices.push(Voice {
                category: category.clone(),
                subcategory: subcategory.clone(),
                name: cells[2].clone(),
                msb: number(3)?,
                lsb: number(4)?,
                program,
                voice_type: cells[6].clone(),
                page,
            });
        }
    }
    let catalog = Catalog {
        schema: SCHEMA.into(),
        edition: EDITION.into(),
        source_name: source_name.chars().take(160).collect(),
        source_sha256: SOURCE_SHA256.into(),
        records_sha256: crate::project::hash(&serde_json::to_vec(&voices)?),
        voices,
    };
    catalog.validate()?;
    Ok(catalog)
}
pub fn import_pdf(bytes: &[u8], source_name: &str) -> Result<Catalog> {
    if bytes.len() > MAX_PDF_BYTES {
        return Err(bad("Data List exceeds the 32 MiB input limit"));
    }
    if crate::project::hash(bytes) != SOURCE_SHA256 {
        return Err(Error::Unsupported(format!(
            "Choose the supported {EDITION} PDF. This file is a different edition, modified, or not a Genos Data List."
        )));
    }
    // Reject unknown bytes before invoking a third-party PDF interpreter. Catch
    // extractor panics too; an import failure must not terminate an editing session.
    std::panic::catch_unwind(|| extract(bytes, source_name))
        .map_err(|_| bad("PDF extraction failed; existing catalog retained"))?
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
            "voice catalog exceeds size limit; import the Data List again",
        ));
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_CACHE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return Err(bad("voice catalog exceeds size limit"));
    }
    let catalog: Catalog = serde_json::from_slice(&bytes)?;
    catalog.validate()?;
    Ok(Some(catalog))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrumChannelStatus {
    pub channel: u8,
    pub address: Option<Address>,
    pub voice: Option<Voice>,
    /// True when a Program Change exists after the section start tick on this channel.
    pub late_program: bool,
    pub kit_category: bool,
}
/// Section-start kit addresses for rhythm channels 9/10 using the imported catalog.
/// Does not decode Yamaha Drum Setup SysEx or note remaps (those remain gated).
pub fn drum_channel_status(
    doc: &Document,
    catalog: &Catalog,
    section: u64,
) -> Result<Vec<DrumChannelStatus>> {
    let s = doc
        .sections
        .iter()
        .find(|s| s.id == section)
        .ok_or_else(|| bad("section not found"))?;
    Ok(DRUM_CHANNELS
        .iter()
        .map(|&channel| {
            let address = address_at(doc, s.start, channel);
            let voice = address.and_then(|a| catalog.find(a).cloned());
            let late_program = doc.style.tracks.iter().flatten().any(|e| {
                e.channel() == Some(channel)
                    && e.tick > s.start
                    && e.tick < s.end
                    && e.data.first().is_some_and(|st| st >> 4 == 0xc)
            });
            DrumChannelStatus {
                channel,
                address,
                kit_category: voice
                    .as_ref()
                    .is_some_and(|v| v.category == DRUM_KIT_CATEGORY),
                voice,
                late_program,
            }
        })
        .collect())
}

/// Resolve the selected program's latched banks, not later pending Bank Select CCs.
pub fn address_at(doc: &Document, tick: u64, channel: u8) -> Option<Address> {
    let mut events: Vec<_> = doc
        .style
        .tracks
        .iter()
        .flatten()
        .filter(|e| e.tick <= tick && e.channel() == Some(channel))
        .collect();
    events.sort_by_key(|e| (e.tick, e.order, e.id));
    let (mut msb, mut lsb, mut active) = (None, None, None);
    for e in events {
        match e.data[0] >> 4 {
            0xb if e.data[1] == 0 => msb = Some(e.data[2]),
            0xb if e.data[1] == 32 => lsb = Some(e.data[2]),
            0xc => {
                active = msb.zip(lsb).map(|(msb, lsb)| Address {
                    msb,
                    lsb,
                    program: e.data[1],
                })
            }
            _ => {}
        }
    }
    active
}
pub(crate) fn set_voice(
    style: &mut Style,
    section: u64,
    channel: u8,
    address: Address,
) -> Result<()> {
    if !(1..=16).contains(&channel)
        || address.msb > 127
        || address.lsb > 127
        || address.program > 127
    {
        return Err(Error::Edit("voice address is outside MIDI bounds".into()));
    }
    let doc = Document::from_style(style.clone())?;
    let start = doc
        .sections
        .iter()
        .find(|s| s.id == section && s.end > s.start)
        .ok_or_else(|| Error::Edit("section not found or empty".into()))?
        .start;
    // Only exact canonical triples at this boundary count as no-op. An inherited
    // value may have a pending bank selection and must be made explicit on Apply.
    let at_start: Vec<_> = style
        .tracks
        .iter()
        .flatten()
        .filter(|e| {
            e.tick == start
                && e.channel() == Some(channel)
                && (e.data[0] >> 4 == 0xc || (e.data[0] >> 4 == 0xb && matches!(e.data[1], 0 | 32)))
        })
        .collect();
    let mut ordered = at_start.clone();
    ordered.sort_by_key(|e| (e.order, e.id));
    let data = [
        vec![0xb0 | (channel - 1), 0, address.msb],
        vec![0xb0 | (channel - 1), 32, address.lsb],
        vec![0xc0 | (channel - 1), address.program],
    ];
    if ordered.len() == 3
        && ordered.iter().zip(&data).all(|(e, d)| e.data == *d)
        && address_at(&doc, start, channel) == Some(address)
        && style
            .tracks
            .iter()
            .flatten()
            .filter(|e| e.tick == start && e.channel() == Some(channel) && e.is_on())
            .all(|e| (e.order, e.id) > (ordered[2].order, ordered[2].id))
    {
        return Ok(());
    }
    let remove: HashSet<_> = at_start.iter().map(|e| e.id).collect();
    let mut next = style
        .tracks
        .iter()
        .flatten()
        .map(|e| e.id.max(e.order))
        .max()
        .unwrap_or(0)
        + 1;
    for es in &mut style.tracks {
        es.retain(|e| !remove.contains(&e.id));
    }
    for bytes in data {
        style.tracks[0].push(Event {
            id: next,
            tick: start,
            order: 0,
            data: bytes,
        });
        next += 1;
    }
    Ok(())
}
