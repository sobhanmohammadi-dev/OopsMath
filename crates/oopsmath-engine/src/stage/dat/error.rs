//! Typed errors for every layer of the DAT v1 reader.
//!
//! * [`HeaderError`]  - the fixed 64-byte header.
//! * [`SectionError`] - the section directory and raw section payloads.
//! * [`DecodeError`]  - decoding of an already verified payload (MessagePack, WRLD).
//! * [`WorldError`]   - malformed Binary Voxel World v1 data.
//!
//! The high-level `StageLoadError` wraps all of them.

use thiserror::Error;

use crate::stage::dat::section_type::SectionType;

// -----------------------------------------------------------------------------
// Header errors
// -----------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HeaderError {
    #[error("DAT header is too small: expected at least 64 bytes, got {actual}")]
    HeaderTooSmall { actual: usize },

    #[error("Invalid DAT magic: expected OOPSMDAT")]
    InvalidMagic,

    #[error("Unsupported DAT format version: {0}")]
    UnsupportedFormatVersion(u16),

    #[error("Unsupported stage schema version: {0}")]
    UnsupportedSchemaVersion(u16),

    #[error("Invalid header size: expected 64, got {0}")]
    InvalidHeaderSize(u16),

    #[error("File size mismatch: header declares {declared} bytes, actual size is {actual} bytes")]
    FileSizeMismatch { declared: u64, actual: u64 },

    #[error("Invalid section entry size: expected 48, got {0}")]
    InvalidSectionEntrySize(u32),

    #[error("Header CRC32 mismatch: expected {expected:#010X}, calculated {actual:#010X}")]
    HeaderCrcMismatch { expected: u32, actual: u32 },

    #[error("Runtime version incompatible: required {required:#010X}, current {current:#010X}")]
    RuntimeVersionIncompatible { required: u32, current: u32 },

    #[error("Reserved header bytes must be zero; non-zero byte at offset {offset}")]
    NonZeroReservedBytes { offset: usize },

    #[error("Unknown DAT header flags: {0:#010X}")]
    UnknownHeaderFlags(u32),
}

// -----------------------------------------------------------------------------
// Section directory and section data errors
// -----------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SectionError {
    #[error("Invalid section table offset: {offset}")]
    InvalidSectionTableOffset { offset: u64 },

    #[error(
        "Section directory is out of bounds: offset={offset}, count={count}, \
         entry_size={entry_size}, file_size={file_size}"
    )]
    SectionDirectoryOutOfBounds {
        offset: u64,
        count: u32,
        entry_size: u32,
        file_size: u64,
    },

    #[error(
        "Section directory range overflow: offset={offset}, count={count}, \
         entry_size={entry_size}"
    )]
    SectionDirectoryRangeOverflow {
        offset: u64,
        count: u32,
        entry_size: u32,
    },

    #[error("Invalid section type bytes: {raw:?}")]
    InvalidSectionType { raw: [u8; 4] },

    #[error(
        "Section {section} is out of bounds: offset={offset}, stored_size={stored_size}, file_size={file_size}"
    )]
    SectionOutOfBounds {
        section: SectionType,
        offset: u64,
        stored_size: u64,
        file_size: u64,
    },

    #[error("Section {section} range overflows: offset={offset}, stored_size={stored_size}")]
    SectionRangeOverflow {
        section: SectionType,
        offset: u64,
        stored_size: u64,
    },

    #[error("Section {section} overlaps with section {overlaps_with}")]
    SectionOverlap {
        section: SectionType,
        overlaps_with: SectionType,
    },

    #[error("Section {section} overlaps with the DAT header or section directory")]
    SectionOverlapsMetadata { section: SectionType },

    #[error(
        "Invalid alignment for section {section}: offset={offset}, required alignment={alignment}"
    )]
    InvalidSectionAlignment {
        section: SectionType,
        offset: u64,
        alignment: u64,
    },

    #[error("Duplicate section: {0}")]
    DuplicateSection(SectionType),

    #[error("Unknown critical section: {0}")]
    UnknownCriticalSection(SectionType),

    #[error("Required section is missing: {0}")]
    MissingRequiredSection(SectionType),

    #[error("Unknown flags for section {section}: {flags:#010X}")]
    UnknownSectionFlags { section: SectionType, flags: u32 },

    #[error("Unsupported compression value {compression} for section {section}")]
    UnsupportedCompression {
        section: SectionType,
        compression: u8,
    },

    #[error("Reserved bytes in section entry {section} must be zero")]
    NonZeroReservedSectionBytes { section: SectionType },

    #[error("Failed to decompress section {section}: {reason}")]
    DecompressionFailed {
        section: SectionType,
        reason: String,
    },

    #[error(
        "Section {section} checksum mismatch: expected {expected:#010X}, calculated {actual:#010X}"
    )]
    SectionChecksumMismatch {
        section: SectionType,
        expected: u32,
        actual: u32,
    },

    #[error("Section {section} raw size mismatch: expected {expected} bytes, got {actual} bytes")]
    RawSizeMismatch {
        section: SectionType,
        expected: u64,
        actual: u64,
    },

    #[error(
        "Section {section} declares {declared} decompressed bytes, exceeding the limit of {limit} bytes"
    )]
    DecompressedSizeLimitExceeded {
        section: SectionType,
        declared: u64,
        limit: u64,
    },
}

// -----------------------------------------------------------------------------
// Payload decoding errors (MessagePack, WRLD, assets)
// -----------------------------------------------------------------------------

/// Errors produced while decoding the payload of an individual section.
///
/// The DAT container itself (header, directory, checksums) is validated by
/// `HeaderError` and `SectionError`; these errors concern the content of an
/// already-retrieved, checksum-verified payload.
#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)] // clear names beat clippy pedantry here
pub enum DecodeError {
    #[error("Invalid MessagePack payload in section {section}: {reason}")]
    InvalidMsgpack {
        section: SectionType,
        reason: String,
    },

    #[error("Unexpected MessagePack structure in section {section}: {reason}")]
    InvalidMsgpackStructure {
        section: SectionType,
        reason: String,
    },

    #[error("Corrupt WRLD section: {0}")]
    InvalidWorld(#[from] WorldError),
}

/// Errors describing malformed OopsMath Binary Voxel World v1 data.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorldError {
    #[error("WRLD payload is too small to contain the 28-byte header")]
    HeaderTooSmall,

    #[error("Invalid WRLD magic: expected OWLD")]
    InvalidMagic,

    #[error("Unsupported WRLD version: {0}")]
    UnsupportedVersion(u16),

    #[error("Unsupported WRLD chunk size: {0}, expected 16")]
    UnsupportedChunkSize(u16),

    #[error("WRLD world size {dims:?} exceeds the supported world axis maximum of 1_000_000")]
    WorldAxisTooLarge { dims: [u32; 3] },

    #[error("WRLD palette has {count} entries but an index references slot {index}")]
    InvalidPaletteIndex { index: u16, count: u32 },

    #[error("WRLD palette entry {index} length prefix {length} is outside the data")]
    PaletteLengthOutOfBounds { index: u32, length: u64 },

    #[error("WRLD palette declares {declared} entries, exceeding the limit of {limit}")]
    PaletteTooLarge { declared: u32, limit: u32 },

    #[error("WRLD palette entry {index} is not valid UTF-8")]
    PaletteNotUtf8 { index: u32 },

    #[error("WRLD declares {declared} chunk records, exceeding the limit of {limit}")]
    ChunkCountTooLarge { declared: u32, limit: u32 },

    #[error("WRLD chunk record is truncated at offset {offset}")]
    ChunkRecordTruncated { offset: u64 },

    #[error("WRLD chunk {x},{y},{z} payload is truncated: expected {expected} bytes, got {actual}")]
    ChunkPayloadTruncated {
        x: u16,
        y: u16,
        z: u16,
        expected: u64,
        actual: u64,
    },

    #[error("WRLD chunk {x},{y},{z} uses unknown encoding {encoding}")]
    UnknownChunkEncoding {
        x: u16,
        y: u16,
        z: u16,
        encoding: u8,
    },

    #[error("WRLD chunk {x},{y},{z} does not contain 4096 cells after decoding")]
    ChunkCellCountMismatch { x: u16, y: u16, z: u16 },

    #[error("WRLD chunk {x},{y},{z} has an RLE payload of {len} bytes that is not a whole number of runs")]
    MalformedRlePayload { x: u16, y: u16, z: u16, len: u64 },

    #[error("WRLD chunk {x},{y},{z} declares {runs} runs, exceeding the limit of {limit}")]
    ChunkRunCountTooLarge {
        x: u16,
        y: u16,
        z: u16,
        runs: u64,
        limit: u64,
    },

    #[error("WRLD chunk {x},{y},{z} appears more than once")]
    DuplicateChunk { x: u16, y: u16, z: u16 },

    #[error("WRLD chunk {x},{y},{z} lies outside the declared world {dims:?}")]
    ChunkOutOfBounds {
        x: u16,
        y: u16,
        z: u16,
        dims: [u32; 3],
    },
}
