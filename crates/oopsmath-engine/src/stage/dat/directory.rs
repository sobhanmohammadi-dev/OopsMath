//! Section directory parsing and validation for DAT v1.
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
//!                         no compression)
//! 32  u32 crc32           CRC32 of the raw (decompressed) payload
//! 36  u32 reserved        must be zero
//! 40  reserved[8]         must be zero
//! ```
//!
//! Compression ids (low 2 bits of flags): `0` none, `1` zstd; `2`/`3` are
//! reserved and rejected.
//!
//! Section bodies follow the directory. META and STAG are mandatory; the other
//! sections are optional and announced by a `HEADER_FLAG_HAS_*` bit, which
//! must be backed by an actual directory entry.

use std::ops::Range;

use crate::stage::dat::bytes::{le_u32, le_u64};
use crate::stage::dat::error::SectionError;
use crate::stage::dat::flags::{
    HEADER_FLAG_HAS_CUSTOM_ASSETS, HEADER_FLAG_HAS_DOCUMENTS, HEADER_FLAG_HAS_LOCALIZATION,
    HEADER_FLAG_HAS_WORLD, SECTION_COMPRESSION_MASK, SECTION_FLAG_CRITICAL, SECTION_FLAGS_KNOWN,
};
use crate::stage::dat::header::DatHeader;
use crate::stage::dat::section_type::SectionType;

/// Required alignment for section offsets.
pub(crate) const SECTION_ALIGNMENT: u64 = 16;

/// Sections that are always present.
const REQUIRED_SECTIONS: [SectionType; 2] = [SectionType::META, SectionType::STAG];

/// Header flags that promise an optional section.
const FLAG_GATED_SECTIONS: [(u32, SectionType); 4] = [
    (HEADER_FLAG_HAS_WORLD, SectionType::WRLD),
    (HEADER_FLAG_HAS_LOCALIZATION, SectionType::LOCL),
    (HEADER_FLAG_HAS_DOCUMENTS, SectionType::DOCS),
    (HEADER_FLAG_HAS_CUSTOM_ASSETS, SectionType::ASIX),
];

/// Payload compression of a stored section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Compression {
    None,
    Zstd,
}

impl Compression {
    fn from_id(id: u8) -> Option<Self> {
        match id {
            0 => Some(Self::None),
            1 => Some(Self::Zstd),
            _ => None,
        }
    }
}

/// A parsed, validated section entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SectionEntry {
    pub(crate) section_type: SectionType,
    pub(crate) offset: u64,
    pub(crate) stored_size: u64,
    pub(crate) raw_size: u64,
    pub(crate) crc32: u32,
    pub(crate) compression: Compression,
}

impl SectionEntry {
    /// The stored byte range of this section inside a file of `file_size`
    /// bytes, with every overflow and bounds condition checked.
    pub(crate) fn byte_range(&self, file_size: u64) -> Result<Range<usize>, SectionError> {
        let overflow = || SectionError::SectionRangeOverflow {
            section: self.section_type,
            offset: self.offset,
            stored_size: self.stored_size,
        };
        let end = self
            .offset
            .checked_add(self.stored_size)
            .ok_or_else(overflow)?;
        if end > file_size {
            return Err(SectionError::SectionOutOfBounds {
                section: self.section_type,
                offset: self.offset,
                stored_size: self.stored_size,
                file_size,
            });
        }
        let start = usize::try_from(self.offset).map_err(|_| overflow())?;
        let end = usize::try_from(end).map_err(|_| overflow())?;
        Ok(start..end)
    }

    /// Whether the stored byte ranges of two entries intersect.
    fn overlaps(&self, other: &Self) -> bool {
        // Ranges are bounds-checked before entries are stored, so these sums
        // cannot overflow.
        let (end, other_end) = (
            self.offset + self.stored_size,
            other.offset + other.stored_size,
        );
        other.offset < end && self.offset < other_end
    }
}

/// The parsed section directory of one DAT file, in file order.
#[derive(Debug, Clone)]
pub(crate) struct SectionDirectory {
    entries: Vec<SectionEntry>,
}

impl SectionDirectory {
    /// Parses and validates the 48-byte section entries starting at
    /// `header.section_table_offset`.
    pub(crate) fn parse(data: &[u8], header: &DatHeader) -> Result<Self, SectionError> {
        let file_size = data.len() as u64;
        let table_end = locate_table(header, file_size)?;

        let mut entries: Vec<SectionEntry> = Vec::with_capacity(header.section_count as usize);
        for index in 0..header.section_count as usize {
            // The table was bounds-checked against the file size above, so
            // this offset fits in `usize`.
            let base = header.section_table_offset as usize
                + index * header.section_entry_size as usize;
            // Unknown non-critical sections are skipped entirely.
            let Some(entry) = parse_entry(data, base, file_size)? else {
                continue;
            };

            // Sections must start after the header and the directory.
            if entry.offset < table_end {
                return Err(SectionError::SectionOverlapsMetadata {
                    section: entry.section_type,
                });
            }
            if entries
                .iter()
                .any(|other| other.section_type == entry.section_type)
            {
                return Err(SectionError::DuplicateSection(entry.section_type));
            }
            if let Some(other) = entries.iter().find(|other| entry.overlaps(other)) {
                return Err(SectionError::SectionOverlap {
                    section: entry.section_type,
                    overlaps_with: other.section_type,
                });
            }
            entries.push(entry);
        }

        let directory = Self { entries };
        directory.check_required_sections()?;
        directory.check_header_flags(header.flags)?;
        Ok(directory)
    }

    /// Returns the entry for `section`, or `None` when the section is absent.
    pub(crate) fn get(&self, section: SectionType) -> Option<&SectionEntry> {
        self.entries
            .iter()
            .find(|entry| entry.section_type == section)
    }

    /// Returns true when the directory contains `section`.
    pub(crate) fn contains(&self, section: SectionType) -> bool {
        self.get(section).is_some()
    }

    fn check_required_sections(&self) -> Result<(), SectionError> {
        match REQUIRED_SECTIONS
            .into_iter()
            .find(|section| !self.contains(*section))
        {
            Some(missing) => Err(SectionError::MissingRequiredSection(missing)),
            None => Ok(()),
        }
    }

    /// Each `HAS_*` header flag implies its section is present.
    fn check_header_flags(&self, header_flags: u32) -> Result<(), SectionError> {
        for (flag, section) in FLAG_GATED_SECTIONS {
            if header_flags & flag != 0 && !self.contains(section) {
                return Err(SectionError::MissingRequiredSection(section));
            }
        }
        Ok(())
    }
}

/// Validates the location of the section table and returns the offset of its
/// first byte past the end.
fn locate_table(header: &DatHeader, file_size: u64) -> Result<u64, SectionError> {
    let offset = header.section_table_offset;
    if offset < u64::from(header.header_size) {
        return Err(SectionError::InvalidSectionTableOffset { offset });
    }
    let end = u64::from(header.section_count)
        .checked_mul(u64::from(header.section_entry_size))
        .and_then(|len| offset.checked_add(len))
        .ok_or(SectionError::SectionDirectoryRangeOverflow {
            offset,
            count: header.section_count,
            entry_size: header.section_entry_size,
        })?;
    if end > file_size {
        return Err(SectionError::SectionDirectoryOutOfBounds {
            offset,
            count: header.section_count,
            entry_size: header.section_entry_size,
            file_size,
        });
    }
    Ok(end)
}

/// Parses the directory entry at `base`.
///
/// Returns `Ok(None)` for unknown non-critical sections, which readers must
/// skip. The caller guarantees that `data[base..base + 48]` exists.
fn parse_entry(
    data: &[u8],
    base: usize,
    file_size: u64,
) -> Result<Option<SectionEntry>, SectionError> {
    let mut type_bytes = [0u8; 4];
    type_bytes.copy_from_slice(&data[base..base + 4]);
    let section_type = SectionType::new(type_bytes);
    if !section_type.is_printable_ascii() {
        return Err(SectionError::InvalidSectionType { raw: type_bytes });
    }

    let flags = le_u32(data, base + 4);
    let critical = flags & SECTION_FLAG_CRITICAL != 0;
    if !section_type.is_known() {
        return if critical {
            Err(SectionError::UnknownCriticalSection(section_type))
        } else {
            Ok(None)
        };
    }

    let reserved_is_zero =
        le_u32(data, base + 36) == 0 && data[base + 40..base + 48].iter().all(|b| *b == 0);
    if !reserved_is_zero {
        return Err(SectionError::NonZeroReservedSectionBytes {
            section: section_type,
        });
    }
    // Unknown flag bits are a future format extension; only criticality makes
    // them blocking.
    if critical && flags & !SECTION_FLAGS_KNOWN != 0 {
        return Err(SectionError::UnknownSectionFlags {
            section: section_type,
            flags,
        });
    }

    let compression_id = (flags & SECTION_COMPRESSION_MASK) as u8;
    let compression =
        Compression::from_id(compression_id).ok_or(SectionError::UnsupportedCompression {
            section: section_type,
            compression: compression_id,
        })?;

    let entry = SectionEntry {
        section_type,
        offset: le_u64(data, base + 8),
        stored_size: le_u64(data, base + 16),
        raw_size: le_u64(data, base + 24),
        crc32: le_u32(data, base + 32),
        compression,
    };
    if entry.offset % SECTION_ALIGNMENT != 0 {
        return Err(SectionError::InvalidSectionAlignment {
            section: section_type,
            offset: entry.offset,
            alignment: SECTION_ALIGNMENT,
        });
    }
    entry.byte_range(file_size)?;
    Ok(Some(entry))
}
