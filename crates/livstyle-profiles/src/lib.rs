//! Declarative chip and board profiles (PRD §11.1).
//!
//! Chip and board differences live in TOML files; nothing in the runner,
//! supervisor or agent tools branches on a chip name.

mod model;
mod registry;
mod revision;

pub use model::{
    Arch, BoardProfile, BootInfo, Capability, ChipProfile, Companion, Console, DebugInfo,
    FlashDefaults, IdfSupport, OnboardPeripheral, Provenance, ProvenanceStatus, Psram, Radio,
    ReservedPin, RevisionPolicy, SupportTier, UsbInfo, WifiBand, WifiSpec,
};
pub use registry::{ProfileError, Registry};
pub use revision::{ChipRevision, RevisionParseError};
