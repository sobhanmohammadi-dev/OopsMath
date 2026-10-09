//! Section payload access: decompression, checksum, and raw-size validation.
//!
//! A section-loading operation, in the exact order the Python compiler's own
//! verifier (`dat.parse_dat`) applies them:
//!
//! 1. Locate the section entry in the directory.
//! 2. Slice the stored byte range (already bounds-validated with the header).
//! 3. Check the CRC32 over the **raw** (decompressed) payload.
//! 4. Decompress Zstandard payloads when required.
//! 5. Enforce the declared `raw_size`.
//!
//! The Python writer checks `len(raw) != raw_size` first and the CRC second;
//! order of checks does not affect correctness, both are verified here before
//! the payload is handed to the caller.

use crate::stage::dat::error::{SectionError, SectionType};
use crate::stage::dat::section::SectionDirectory;

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

    // Optional sections: a payload of size 0 is only allowed when the entry
    // itself declares zero stored bytes; otherwise treat as missing content.
    if entry.stored_size == 0 && entry.raw_size == 0 {
        return Ok(Vec::new());
    }

    let start = entry.offset as usize;
    let end = start
        + usize::try_from(entry.stored_size).map_err(|_| SectionError::SectionRangeOverflow {
            section,
            offset: entry.offset,
            stored_size: entry.stored_size,
        })?;
    let stored = &data[start..end];

    // Bounds-check the decompressed size before doing any work.
    if entry.raw_size > MAX_SECTION_RAW_SIZE {
        return Err(SectionError::DecompressedSizeLimitExceeded {
            section,
            declared: entry.raw_size,
            limit: MAX_SECTION_RAW_SIZE,
        });
    }

    let raw: Vec<u8> = match entry.compression {
        crate::stage::dat::section::COMPRESSION_NONE => {
            if entry.stored_size != entry.raw_size {
                return Err(SectionError::RawSizeMismatch {
                    section,
                    expected: entry.raw_size,
                    actual: entry.stored_size,
                });
            }
            stored.to_vec()
        }
        crate::stage::dat::section::COMPRESSION_ZSTD => {
            decompress_zstd(section, stored, entry.raw_size)?
        }
        other => {
            return Err(SectionError::UnsupportedCompression {
                section,
                compression: other,
            });
        }
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

/// Decompresses a Zstandard frame while enforcing the declared raw size.
fn decompress_zstd(
    section: SectionType,
    stored: &[u8],
    raw_size: u64,
) -> Result<Vec<u8>, SectionError> {
    // zstd decompression with an explicit output-size limit; the writer
    // records the exact content size (`write_content_size=True`) and the
    // result size is verified against `raw_size` right after.
    let mut decoder =
        zstd::bulk::Decompressor::new().map_err(|err| SectionError::DecompressionFailed {
            section,
            reason: err.to_string(),
        })?;
    let capacity = usize::try_from(raw_size).unwrap_or(usize::MAX);
    let decoded =
        decoder
            .decompress(stored, capacity)
            .map_err(|err| SectionError::DecompressionFailed {
                section,
                reason: err.to_string(),
            })?;
    Ok(decoded)
}
