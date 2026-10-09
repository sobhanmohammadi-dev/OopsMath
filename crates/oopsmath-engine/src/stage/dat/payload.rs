//! Section payload access: bounds, decompression, raw-size and checksum
//! validation.
//!
//! Loading a section applies these steps in order:
//!
//! 1. Locate the section entry in the directory.
//! 2. Reject declared raw sizes above [`MAX_SECTION_RAW_SIZE`].
//! 3. Slice the stored byte range from the file.
//! 4. Decompress Zstandard payloads (or copy uncompressed ones).
//! 5. Enforce the declared `raw_size`.
//! 6. Check the CRC32 over the raw (decompressed) payload.

use crate::stage::dat::directory::{Compression, SectionDirectory};
use crate::stage::dat::error::SectionError;
use crate::stage::dat::section_type::SectionType;

/// Upper bound for a single decompressed section payload (256 MiB). The
/// Python compiler caps source files at 512 MiB, but a fully decompressed
/// stage section beyond this bound would indicate a corrupt or hostile file.
pub(crate) const MAX_SECTION_RAW_SIZE: u64 = 256 * 1024 * 1024;

/// Fully loads and validates the payload of `section`.
pub(crate) fn load_section_payload(
    data: &[u8],
    directory: &SectionDirectory,
    section: SectionType,
) -> Result<Vec<u8>, SectionError> {
    let entry = directory
        .get(section)
        .ok_or(SectionError::MissingRequiredSection(section))?;

    // Bound the allocation before doing any work.
    if entry.raw_size > MAX_SECTION_RAW_SIZE {
        return Err(SectionError::DecompressedSizeLimitExceeded {
            section,
            declared: entry.raw_size,
            limit: MAX_SECTION_RAW_SIZE,
        });
    }

    let stored = &data[entry.byte_range(data.len() as u64)?];

    let raw = match entry.compression {
        Compression::None => {
            if entry.stored_size != entry.raw_size {
                return Err(SectionError::RawSizeMismatch {
                    section,
                    expected: entry.raw_size,
                    actual: entry.stored_size,
                });
            }
            stored.to_vec()
        }
        // An empty section carries no zstd frame at all.
        Compression::Zstd if entry.stored_size == 0 && entry.raw_size == 0 => Vec::new(),
        Compression::Zstd => decompress_zstd(section, stored, entry.raw_size)?,
    };

    if raw.len() as u64 != entry.raw_size {
        return Err(SectionError::RawSizeMismatch {
            section,
            expected: entry.raw_size,
            actual: raw.len() as u64,
        });
    }

    let checksum = crc32fast::hash(&raw);
    if checksum != entry.crc32 {
        return Err(SectionError::SectionChecksumMismatch {
            section,
            expected: entry.crc32,
            actual: checksum,
        });
    }

    Ok(raw)
}

/// Decompresses a Zstandard frame, refusing to produce more than `raw_size`
/// bytes.
fn decompress_zstd(
    section: SectionType,
    stored: &[u8],
    raw_size: u64,
) -> Result<Vec<u8>, SectionError> {
    // `raw_size` was checked against MAX_SECTION_RAW_SIZE, so it fits `usize`.
    zstd::bulk::decompress(stored, raw_size as usize).map_err(|err| {
        SectionError::DecompressionFailed {
            section,
            reason: err.to_string(),
        }
    })
}
