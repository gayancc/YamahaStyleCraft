pub mod document;
pub mod edit;
pub mod midi;
pub mod project;
pub mod smf;

use thiserror::Error;
#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid Style: {0}")]
    Format(String),
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
    #[error("Edit rejected: {0}")]
    Edit(String),
    #[error("MIDI: {0}")]
    Midi(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

/// EV-0202/0282: half-even per edge, deliberately separate from editing snap.
pub fn scale_tick(tick: u64, source: u16, target: u16) -> u64 {
    let n = tick as u128 * target as u128;
    let d = source.max(1) as u128;
    let q = n / d;
    let r = n % d;
    (q + u128::from(r * 2 > d || (r * 2 == d && q % 2 == 1))) as u64
}
