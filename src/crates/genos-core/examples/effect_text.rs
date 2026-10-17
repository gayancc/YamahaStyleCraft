use pdf_extract::{MediaBox, OutputDev, OutputError, Transform};
use std::collections::BTreeMap;

#[derive(Default)]
struct Probe {
    // page -> y_bucket -> string
    rows: BTreeMap<(u32, i32), String>,
    page: u32,
}
impl OutputDev for Probe {
    fn begin_page(
        &mut self,
        page: u32,
        _: &MediaBox,
        _: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.page = page;
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn begin_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        t: &Transform,
        _: f64,
        _: f64,
        _: f64,
        text: &str,
    ) -> Result<(), OutputError> {
        let y = (t.m32 * 10.0).round() as i32;
        self.rows.entry((self.page, y)).or_default().push_str(text);
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: effect_text <Genos Data List PDF>")?;
    let bytes = std::fs::read(path)?;
    if genos_core::project::hash(&bytes) != genos_core::voice_catalog::SOURCE_SHA256 {
        return Err("Unsupported PDF edition".into());
    }
    let doc = pdf_extract::Document::load_mem(&bytes)?;
    let mut probe = Probe::default();
    for page in [47u32, 49, 50, 52, 53] {
        pdf_extract::output_doc_page(&doc, &mut probe, page)?;
    }
    for ((page, y), text) in probe.rows {
        if text.contains("Celeste")
            || text.contains("RingMod")
            || text.contains("Ring Mod")
            || text.contains("Legend Blues")
            || text.contains("Loop FX")
            || text.contains("No Effect")
            || text.contains("Through")
        {
            println!("p{page} y={:.1} {}", y as f64 / 10.0, text);
        }
    }
    Ok(())
}
