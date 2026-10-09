//! Flag bits and bit masks defined by DAT v1.

// -----------------------------------------------------------------------------
// Header flags
// -----------------------------------------------------------------------------

pub(crate) const HEADER_FLAG_HAS_WORLD: u32 = 1 << 0;
pub(crate) const HEADER_FLAG_HAS_LOCALIZATION: u32 = 1 << 1;
pub(crate) const HEADER_FLAG_HAS_DOCUMENTS: u32 = 1 << 2;
pub(crate) const HEADER_FLAG_HAS_CUSTOM_ASSETS: u32 = 1 << 3;
pub(crate) const HEADER_FLAG_HAS_COMPRESSED_SECTIONS: u32 = 1 << 4;
pub(crate) const HEADER_FLAG_DEBUG_BUILD: u32 = 1 << 5;

/// All header flag bits defined for DAT v1; anything else is a future format.
pub(crate) const HEADER_FLAGS_KNOWN: u32 = HEADER_FLAG_HAS_WORLD
    | HEADER_FLAG_HAS_LOCALIZATION
    | HEADER_FLAG_HAS_DOCUMENTS
    | HEADER_FLAG_HAS_CUSTOM_ASSETS
    | HEADER_FLAG_HAS_COMPRESSED_SECTIONS
    | HEADER_FLAG_DEBUG_BUILD;

// -----------------------------------------------------------------------------
// Section flags
// -----------------------------------------------------------------------------

/// Low two bits of the section flags: the compression id.
pub(crate) const SECTION_COMPRESSION_MASK: u32 = 0b11;
/// A reader that does not understand the section must fail.
pub(crate) const SECTION_FLAG_CRITICAL: u32 = 1 << 8;
/// Hot-loading hint carried verbatim (informational).
pub(crate) const SECTION_FLAG_STREAMABLE: u32 = 1 << 9;
/// Payload is not MessagePack (informational for readers).
pub(crate) const SECTION_FLAG_BINARY: u32 = 1 << 10;

/// All section flag bits defined for DAT v1.
pub(crate) const SECTION_FLAGS_KNOWN: u32 = SECTION_COMPRESSION_MASK
    | SECTION_FLAG_CRITICAL
    | SECTION_FLAG_STREAMABLE
    | SECTION_FLAG_BINARY;
