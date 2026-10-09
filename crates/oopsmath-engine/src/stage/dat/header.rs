//! DAT v1 header parsing.
//!
//! The 64-byte header layout (little endian, no padding) mirrors the writer in
//! `tools/oopsmath-stage-compiler/oopsmath_stage/dat.py` via
//! `constants.HEADER_STRUCT = "<8sHHHHIQQIIII12s"`:
//!
//! ```text
//!  0  magic[8]                 "OOPSMDAT"
//!  8  u16 format_version       DAT v1 => 1
//! 10  u16 schema_version       Stage Schema v1 => 1
//! 12  u16 header_size          always 64
//! 14  u16 reserved             must be zero
//! 16  u32 flags                HeaderFlag bits reported to the loader
//! 20  u64 file_size            must equal the actual file length
//! 28  u64 section_table_offset
//! 36  u32 section_count
//! 40  u32 section_entry_size   always 48
//! 44  u32 minimum_runtime_version   major<<16 | minor<<8 | patch
//! 48  u32 header_crc32
//! 52  reserved[12]             must be zero
//! ```
//!
//! The header CRC32 is computed by the Python writer as `crc32(raw(0))`:
//! CRC32 over the full 64 bytes with the CRC field itself (48..52) and the
//! trailing reserved 12 bytes (52..64) zeroed. The u16 reserved field (14..16)
//! is part of the prefix and is included in the CRC input.

use crate::stage::dat::bytes::{le_u16, le_u32, le_u64};
use crate::stage::dat::error::HeaderError;
use crate::stage::dat::flags::HEADER_FLAGS_KNOWN;

/// File magic.
pub(crate) const DAT_MAGIC: [u8; 8] = *b"OOPSMDAT";
/// DAT v1 format version accepted by this reader.
pub(crate) const DAT_FORMAT_VERSION: u16 = 1;
/// Stage Schema version accepted by this reader.
pub(crate) const STAGE_SCHEMA_VERSION: u16 = 1;
/// Fixed header size in bytes.
pub(crate) const HEADER_SIZE: usize = 64;
/// Fixed section entry size in bytes.
pub(crate) const SECTION_ENTRY_SIZE: u32 = 48;
/// Runtime version this engine implements: 1.0.0 encoded as
/// `major << 16 | minor << 8 | patch`.
pub(crate) const CURRENT_RUNTIME_VERSION: u32 = 1 << 16;

/// Number of leading header bytes covered by the CRC (everything before the
/// CRC field). The CRC field and the trailing reserved bytes count as zero.
const CRC_COVERED_PREFIX: usize = 48;
const RESERVED_TAIL_START: usize = 52;

/// The validated fields of a DAT v1 header that later stages need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DatHeader {
    pub(crate) format_version: u16,
    pub(crate) schema_version: u16,
    pub(crate) header_size: u16,
    pub(crate) flags: u32,
    pub(crate) section_table_offset: u64,
    pub(crate) section_count: u32,
    pub(crate) section_entry_size: u32,
}

impl DatHeader {
    /// Parses and fully validates a DAT v1 header from the start of `data`.
    ///
    /// `data` must be the raw contents of the whole file; the header is
    /// assumed to begin at offset 0.
    pub(crate) fn parse(data: &[u8]) -> Result<Self, HeaderError> {
        if data.len() < HEADER_SIZE {
            return Err(HeaderError::HeaderTooSmall { actual: data.len() });
        }
        if data[..DAT_MAGIC.len()] != DAT_MAGIC {
            return Err(HeaderError::InvalidMagic);
        }

        let format_version = le_u16(data, 8);
        let schema_version = le_u16(data, 10);
        let header_size = le_u16(data, 12);
        if usize::from(header_size) != HEADER_SIZE {
            return Err(HeaderError::InvalidHeaderSize(header_size));
        }
        if le_u16(data, 14) != 0 {
            return Err(HeaderError::NonZeroReservedBytes { offset: 14 });
        }

        let flags = le_u32(data, 16);
        // Reserved flag bits belong to a future format extension.
        if flags & !HEADER_FLAGS_KNOWN != 0 {
            return Err(HeaderError::UnknownHeaderFlags(flags));
        }
        let file_size = le_u64(data, 20);
        let section_table_offset = le_u64(data, 28);
        let section_count = le_u32(data, 36);
        let section_entry_size = le_u32(data, 40);
        let minimum_runtime_version = le_u32(data, 44);
        let header_crc32 = le_u32(data, 48);

        if let Some(index) = data[RESERVED_TAIL_START..HEADER_SIZE]
            .iter()
            .position(|byte| *byte != 0)
        {
            return Err(HeaderError::NonZeroReservedBytes {
                offset: RESERVED_TAIL_START + index,
            });
        }

        verify_crc(data, header_crc32)?;

        if format_version != DAT_FORMAT_VERSION {
            return Err(HeaderError::UnsupportedFormatVersion(format_version));
        }
        if schema_version != STAGE_SCHEMA_VERSION {
            return Err(HeaderError::UnsupportedSchemaVersion(schema_version));
        }
        if section_entry_size != SECTION_ENTRY_SIZE {
            return Err(HeaderError::InvalidSectionEntrySize(section_entry_size));
        }
        if file_size != data.len() as u64 {
            return Err(HeaderError::FileSizeMismatch {
                declared: file_size,
                actual: data.len() as u64,
            });
        }
        // A stage requiring a newer runtime than ours cannot be loaded safely.
        if minimum_runtime_version > CURRENT_RUNTIME_VERSION {
            return Err(HeaderError::RuntimeVersionIncompatible {
                required: minimum_runtime_version,
                current: CURRENT_RUNTIME_VERSION,
            });
        }

        Ok(Self {
            format_version,
            schema_version,
            header_size,
            flags,
            section_table_offset,
            section_count,
            section_entry_size,
        })
    }
}

/// Verifies the header CRC32 exactly as the Python writer computes it: over
/// the first 48 bytes followed by 16 zero bytes.
fn verify_crc(data: &[u8], expected: u32) -> Result<(), HeaderError> {
    let mut crc_input = [0u8; HEADER_SIZE];
    crc_input[..CRC_COVERED_PREFIX].copy_from_slice(&data[..CRC_COVERED_PREFIX]);
    let actual = crc32fast::hash(&crc_input);
    if actual != expected {
        return Err(HeaderError::HeaderCrcMismatch { expected, actual });
    }
    Ok(())
}
