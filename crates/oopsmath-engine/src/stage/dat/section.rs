//! Section directory parsing and per-section access for DAT v1.
//!
//! Section entry layout (48 bytes, little endian), matching
//! `constants.SECTION_STRUCT = "<4sIQQQII8s"`:
//!
//! ```text
//!  0  section type[4]     printable ASCII, e.g. "STAG"
//!  4  u32 flags           low 2 bits = compression id, bit 8 = critical,
//!                         bit 9 = streamable, bit 10 = binary
//!  8  u64 offset          absolute file offset, 16-byte aligned
//! 16  u64 stored_size     byte length on disk (before decompression)
//! 24  u64 raw_size        byte length after decompression (== stored for
//!                         COMPRESSION_NONE)
//! 32  u32 crc32           CRC32 of the raw (decompressed) payload
//! 36  u32 reserved        must be zero
//! 40  reserved[8]         must be zero
//! ```
//!
//! Compression ids (low 2 bits of flags):
//! `0` none, `1` zstd; `2`/`3` reserved and rejected.
//!
//! The section body immediately follows the directory. The Python writer only
//! fills the header flag when a section is present, so LOCL/DOCS/WRLD/ASIX are
//! optional sections gated by the corresponding `HeaderFlag` bit, while META
//! and STAG are mandatory.

use std::collections::HashMap;

use crate::stage::dat::error::{SectionError, SectionType};
use crate::stage::dat::header::DatHeader;

/// Compression identifier stored in the low 2 bits of section flags.
pub(crate) const COMPRESSION_NONE: u8 = 0;
pub(crate) const COMPRESSION_ZSTD: u8 = 1;
/// Reserved compression ids (2 and 3), rejected on read.
pub(crate) const COMPRESSION_MAX_KNOWN: u8 = 1;
pub(crate) const COMPRESSION_MASK: u32 = 0b11;
pub(crate) const COMPRESSION_SHIFT: u32 = 0;

/// Section flag bit: a reader that does not understand the section must fail.
pub(crate) const SECTION_CRITICAL: u32 = 1 << 8;
/// Section flag bit: hot-loading hint carried verbatim (informational).
pub(crate) const SECTION_STREAMABLE: u32 = 1 << 9;
/// Section flag bit: payload is not MessagePack (informational for readers).
pub(crate) const SECTION_BINARY: u32 = 1 << 10;

/// All flag bits defined for DAT v1; anything else is a future format.
pub(crate) const SECTION_FLAGS_KNOWN: u32 =
    COMPRESSION_MASK | SECTION_CRITICAL | SECTION_STREAMABLE | SECTION_BINARY;

/// Header flag bit meanings (subset relevant to section presence).
pub(crate) const HEADER_FLAG_HAS_WORLD: u32 = 1 << 0;
pub(crate) const HEADER_FLAG_HAS_LOCALIZATION: u32 = 1 << 1;
pub(crate) const HEADER_FLAG_HAS_DOCUMENTS: u32 = 1 << 2;
pub(crate) const HEADER_FLAG_HAS_CUSTOM_ASSETS: u32 = 1 << 3;
pub(crate) const HEADER_FLAG_HAS_COMPRESSED_SECTIONS: u32 = 1 << 4;
pub(crate) const HEADER_FLAG_DEBUG_BUILD: u32 = 1 << 5;

/// All known header flag bits.
pub(crate) const HEADER_FLAGS_KNOWN: u32 = HEADER_FLAG_HAS_WORLD
    | HEADER_FLAG_HAS_LOCALIZATION
    | HEADER_FLAG_HAS_DOCUMENTS
    | HEADER_FLAG_HAS_CUSTOM_ASSETS
    | HEADER_FLAG_HAS_COMPRESSED_SECTIONS
    | HEADER_FLAG_DEBUG_BUILD;

/// Required alignment for section offsets: 16.
pub(crate) const SECTION_ALIGNMENT: u64 = 16;

/// Recognized DAT v1 section types.
fn is_known_section_type(bytes: [u8; 4]) -> bool {
    bytes == *b"META"
        || bytes == *b"STAG"
        || bytes == *b"WRLD"
        || bytes == *b"LOCL"
        || bytes == *b"DOCS"
        || bytes == *b"ASIX"
        || bytes == *b"ASDT"
}

/// A parsed, validated section entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SectionEntry {
    pub(crate) section_type: SectionType,
    pub(crate) flags: u32,
    pub(crate) offset: u64,
    pub(crate) stored_size: u64,
    pub(crate) raw_size: u64,
    pub(crate) crc32: u32,
    pub(crate) compression: u8,
    pub(crate) critical: bool,
}

impl SectionEntry {
    /// Extracts the compression id from the flag bits.
    pub(crate) fn compression_id(flags: u32) -> u8 {
        ((flags & COMPRESSION_MASK) >> COMPRESSION_SHIFT) as u8
    }
}

/// The parsed section directory of one DAT file.
#[derive(Debug, Clone)]
pub(crate) struct SectionDirectory {
    by_type: HashMap<SectionType, SectionEntry>,
}

impl SectionDirectory {
    /// Parses and validates the 48-byte section entries starting at
    /// `header.section_table_offset`.
    pub(crate) fn parse(data: &[u8], header: &DatHeader) -> Result<Self, SectionError> {
        let file_size = data.len() as u64;

        // Validate the table location and its complete bounds.
        if header.section_table_offset < header.header_size as u64 {
            return Err(SectionError::InvalidSectionTableOffset {
                offset: header.section_table_offset,
            });
        }
        let count = header.section_count as u64;
        let entry = header.section_entry_size as u64;
        let table = header
            .section_table_offset
            .checked_add(
                count
                    .checked_mul(entry)
                    .expect("entry_size * count fits u32*48"),
            )
            .ok_or(SectionError::SectionDirectoryRangeOverflow {
                offset: header.section_table_offset,
                count: header.section_count,
                entry_size: header.section_entry_size,
            })?;
        if table > file_size {
            return Err(SectionError::SectionDirectoryOutOfBounds {
                offset: header.section_table_offset,
                count: header.section_count,
                entry_size: header.section_entry_size,
                file_size,
            });
        }

        let mut by_type: HashMap<SectionType, SectionEntry> = HashMap::new();
        let mut seen_critical: Vec<(SectionType, u64, u64)> = Vec::new();

        for index in 0..header.section_count as usize {
            let base =
                header.section_table_offset as usize + index * header.section_entry_size as usize;
            let mut type_bytes = [0u8; 4];
            type_bytes.copy_from_slice(&data[base..base + 4]);
            let section_type = SectionType(type_bytes);
            if !section_type.is_printable_ascii() {
                return Err(SectionError::InvalidSectionType { raw: type_bytes });
            }

            let mut blob = [0u8; 48];
            blob.copy_from_slice(&data[base..base + 48]);
            let flags = u32::from_le_bytes(blob[4..8].try_into().expect("4-byte slice"));
            let offset = u64::from_le_bytes(blob[8..16].try_into().expect("8-byte slice"));
            let stored_size = u64::from_le_bytes(blob[16..24].try_into().expect("8-byte slice"));
            let raw_size = u64::from_le_bytes(blob[24..32].try_into().expect("8-byte slice"));
            let crc32 = u32::from_le_bytes(blob[32..36].try_into().expect("4-byte slice"));
            let reserved32 = u32::from_le_bytes(blob[36..40].try_into().expect("4-byte slice"));
            let reserved8: &[u8; 8] = blob[40..48].try_into().expect("8-byte slice");

            let compression = SectionEntry::compression_id(flags);
            let critical = flags & SECTION_CRITICAL != 0;
            // Unknown section types must be rejected when they are declared
            // critical; otherwise they may be skipped.
            if !is_known_section_type(type_bytes) {
                if critical {
                    return Err(SectionError::UnknownCriticalSection(section_type));
                }
                // Unknown non-critical sections are skipped (not added to the
                // directory map); keep them out of by_type.
                continue;
            }
            if reserved32 != 0 || reserved8.iter().any(|b| *b != 0) {
                return Err(SectionError::NonZeroReservedSectionBytes {
                    section: section_type,
                });
            }
            // Unknown non-compression flag bits are a future format extension;
            // only criticality makes them blocking.
            if flags & !SECTION_FLAGS_KNOWN != 0 && critical {
                return Err(SectionError::UnknownSectionFlags {
                    section: section_type,
                    flags,
                });
            }
            if compression > COMPRESSION_MAX_KNOWN {
                return Err(SectionError::UnsupportedCompression {
                    section: section_type,
                    compression,
                });
            }
            if offset % SECTION_ALIGNMENT != 0 {
                return Err(SectionError::InvalidSectionAlignment {
                    section: section_type,
                    offset,
                    alignment: SECTION_ALIGNMENT,
                });
            }
            let end =
                offset
                    .checked_add(stored_size)
                    .ok_or(SectionError::SectionRangeOverflow {
                        section: section_type,
                        offset,
                        stored_size,
                    })?;
            if end > file_size {
                return Err(SectionError::SectionOutOfBounds {
                    section: section_type,
                    offset,
                    stored_size,
                    file_size,
                });
            }
            // Sections must not overlap the header or the directory.
            let metadata_end = (header.section_table_offset
                + count
                    .checked_mul(entry)
                    .expect("entry_size * count fits u32*48"))
            .max(header.header_size as u64);
            if offset < metadata_end {
                return Err(SectionError::SectionOverlapsMetadata {
                    section: section_type,
                });
            }

            if by_type.contains_key(&section_type) {
                return Err(SectionError::DuplicateSection(section_type));
            }
            for (other_type, other_offset, other_stored) in &seen_critical {
                let other_end = other_offset + other_stored;
                if *other_offset < end && offset < other_end {
                    return Err(SectionError::SectionOverlap {
                        section: section_type,
                        overlaps_with: *other_type,
                    });
                }
            }
            seen_critical.push((section_type, offset, stored_size));
            let entry = SectionEntry {
                section_type,
                flags,
                offset,
                stored_size,
                raw_size,
                crc32,
                compression,
                critical,
            };
            by_type.insert(section_type, entry);
        }

        // Required sections: META and STAG are always mandatory.
        if !by_type.contains_key(&SectionType::META) {
            return Err(SectionError::MissingRequiredSection(SectionType::META));
        }
        if !by_type.contains_key(&SectionType::STAG) {
            return Err(SectionError::MissingRequiredSection(SectionType::STAG));
        }
        // Header flag consistency: each HAS_* flag implies its section.
        let expected_gated: &[(u32, SectionType)] = &[
            (HEADER_FLAG_HAS_WORLD, SectionType::WRLD),
            (HEADER_FLAG_HAS_LOCALIZATION, SectionType::LOCL),
            (HEADER_FLAG_HAS_DOCUMENTS, SectionType::DOCS),
            (HEADER_FLAG_HAS_CUSTOM_ASSETS, SectionType::ASIX),
        ];
        for (flag, section) in expected_gated {
            let has_entry = by_type.contains_key(section);
            let has_flag = header.flags & flag != 0;
            if has_flag && !has_entry {
                return Err(SectionError::MissingRequiredSection(*section));
            }
        }

        Ok(Self { by_type })
    }

    /// Returns the entry for `section`, or `None` when the section is absent.
    pub(crate) fn get(&self, section: SectionType) -> Option<&SectionEntry> {
        self.by_type.get(&section)
    }

    /// Returns true when the directory contains `section`.
    pub(crate) fn contains(&self, section: SectionType) -> bool {
        self.by_type.contains_key(&section)
    }
}
