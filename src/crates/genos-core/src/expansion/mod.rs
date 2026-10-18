//! Yamaha Expansion Manager pack formats (.n27 / .ppf / .ppi).
//!
//! Structures and decoding rules are evidence-backed from REA/offline analysis
//! of Yamaha Expansion Manager and ConvUVx fixtures. Unknown fields stay unknown.

pub mod iff;
pub mod n27;
pub mod ppf;
pub mod ppi;
pub mod recovery;

pub use n27::{InstrumentInfo, parse_n27};
pub use ppf::{PpfPackage, PpfResource, parse_ppf};
pub use ppi::{PpiPackage, parse_ppi};
pub use recovery::{ExtractRequest, RecoveryReport, recover_pack};
