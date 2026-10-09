//! Low-level DAT v1 container reading.
//!
//! This module is intentionally independent of Bevy: header parsing,
//! directory validation, section payload access, the WRLD voxel decoder and
//! the typed errors all live here so future runtimes (asset hot-loading
//! servers, test harnesses) can reuse them.
//!
//! The error types are re-exported because they appear in the public
//! `StageLoadError`; the header, directory, reader and world decoders stay
//! internal to `stage::dat` and are consumed by `stage::loader`.

pub mod error;
pub(crate) mod header;
pub(crate) mod reader;
pub(crate) mod section;
pub(crate) mod world;

#[allow(unused)]
pub(crate) use error::{DatError, SectionType};
