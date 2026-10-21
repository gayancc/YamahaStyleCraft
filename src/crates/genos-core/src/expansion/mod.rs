//! Yamaha Expansion Manager pack formats (.n27 / .ppf / .ppi).
//!
//! Structures and decoding rules are evidence-backed from REA/offline analysis
//! of Yamaha Expansion Manager and ConvUVx fixtures. Unknown fields stay unknown.

pub mod iff;
pub mod installed_voices;
pub mod n27;
pub mod ppf;
pub mod ppi;
pub mod recovery;
pub mod secure;
pub mod voice_map;
pub mod vprs;
pub mod wave_index;

pub use installed_voices::{InstalledVoice, installed_voices};
pub use n27::{InstrumentInfo, parse_n27};
pub use ppf::{PpfPackage, PpfResource, PpfWriteResource, parse_ppf, rewrite_ppf, write_ppf};
pub use ppi::{PpiPackage, parse_ppi};
pub use recovery::{
    ExtractAllRequest, ExtractAllResult, ExtractRequest, RecoverOptions, RecoveryReport,
    extract_all_resources, recover_pack, recover_pack_with_options,
};
pub use secure::{decrypt_sme, expand_keystream};
pub use voice_map::{PackVoiceMap, VoiceAddress, scan_midi_voice_addresses};
pub use wave_index::{WaveIndex, parse_wave_index_file};
