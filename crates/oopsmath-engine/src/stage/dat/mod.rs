//! Low-level DAT v1 container reading.
//!
//! This module is intentionally independent of Bevy: header parsing,
//! directory validation, section payload access and the WRLD voxel decoder
//! all live here so future runtimes (asset hot-loading servers, test
//! harnesses) can reuse them.
//!
//! Layering, from the file's first byte outwards:
//!
//! 1. [`header`]    - the fixed 64-byte header.
//! 2. [`directory`] - the section table that follows the header.
//! 3. [`payload`]   - bounds-checked, decompressed, CRC-verified section data.
//! 4. [`world`]     - decoder for the `WRLD` section payload.
//!
//! Everything here is crate-internal. The error types and [`SectionType`]
//! are re-exported publicly through `stage::error`, because they appear in
//! `StageLoadError`.
//!
//! [`SectionType`]: section_type::SectionType

pub(crate) mod bytes;
pub(crate) mod directory;
pub(crate) mod error;
pub(crate) mod flags;
pub(crate) mod header;
pub(crate) mod payload;
pub(crate) mod section_type;
pub(crate) mod world;
