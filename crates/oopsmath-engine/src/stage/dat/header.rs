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

use crate::stage::dat::error::HeaderError;

/// DAT v1 format version accepted by this reader.
pub(crate) const DAT_FORMAT_VERSION: u16 = 1;
/// Stage Schema version accepted by this reader.
pub(crate) const STAGE_SCHEMA_VERSION: u16 = 1;
/// Fixed header size in bytes.
pub(crate) const HEADER_SIZE: u64 = 64;
/// Fixed section entry size in bytes.
pub(crate) const SECTION_ENTRY_SIZE: u32 = 48;
/// Runtime version this engine implements: 1.0.0 encoded as
/// `major << 16 | minor << 8 | patch`.
pub(crate) const CURRENT_RUNTIME_VERSION: u32 = 1 << 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DatHeader {
    pub(crate) format_version: u16,
    pub(crate) schema_version: u16,
    pub(crate) header_size: u16,
    pub(crate) flags: u32,
    pub(crate) file_size: u64,
    pub(crate) section_table_offset: u64,
    pub(crate) section_count: u32,
    pub(crate) section_entry_size: u32,
    pub(crate) minimum_runtime_version: u32,
    pub(crate) header_crc32: u32,
}

impl DatHeader {
    /// Parses and fully validates a DAT v1 header from the start of `data`.
    ///
    /// `data` must be the raw contents of the file; the header is assumed to
    /// begin at offset 0.
    pub(crate) fn parse(data: &[u8]) -> Result<Self, HeaderError> {
        if data.len() < HEADER_SIZE as usize {
            return Err(HeaderError::HeaderTooSmall { actual: data.len() });
        }

        let mut magic = [0u8; 8];
        magic.copy_from_slice(&data[0..8]);
        if magic != *b"OOPSMDAT" {
            return Err(HeaderError::InvalidMagic);
        }

        let format_version = u16::from_le_bytes([data[8], data[9]]);
        let schema_version = u16::from_le_bytes([data[10], data[11]]);
        let header_size = u16::from_le_bytes([data[12], data[13]]);
        if header_size != HEADER_SIZE as u16 {
            return Err(HeaderError::InvalidHeaderSize(header_size));
        }

        // u16 reserved at 14..16 must be zero.
        let reserved16 = u16::from_le_bytes([data[14], data[15]]);
        if reserved16 != 0 {
            return Err(HeaderError::NonZeroReservedBytes { offset: 14 });
        }

        let flags = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        // Reserved flag bits belong to a future format extension.
        if flags & !super::section::HEADER_FLAGS_KNOWN != 0 {
            return Err(HeaderError::UnknownHeaderFlags(flags));
        }
        let file_size = u64::from_le_bytes(data[20..28].try_into().expect("8-byte slice"));
        let section_table_offset =
            u64::from_le_bytes(data[28..36].try_into().expect("8-byte slice"));
        let section_count = u32::from_le_bytes([data[36], data[37], data[38], data[39]]);
        let section_entry_size = u32::from_le_bytes([data[40], data[41], data[42], data[43]]);
        let minimum_runtime_version = u32::from_le_bytes([data[44], data[45], data[46], data[47]]);
        let header_crc32 = u32::from_le_bytes([data[48], data[49], data[50], data[51]]);

        // Reserved 12 bytes at 52..64 must be zero.
        for (index, byte) in data[52..64].iter().enumerate() {
            if *byte != 0 {
                return Err(HeaderError::NonZeroReservedBytes { offset: 52 + index });
            }
        }

        // Verify CRC32 exactly as the Python writer computes it: over the
        // first 48 bytes plus 16 zero bytes (zeroed CRC field and trailing
        // reserved region).
        let mut crc_input = [0u8; HEADER_SIZE as usize];
        crc_input[..48].copy_from_slice(&data[..48]);
        let calculated_crc = crc32fast::hash(&crc_input);
        if calculated_crc != header_crc32 {
            return Err(HeaderError::HeaderCrcMismatch {
                expected: header_crc32,
                actual: calculated_crc,
            });
        }

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

        // Minimum runtime compatibility using the actual version encoding
        // (major << 16 | minor << 8 | patch); a stage requiring a version
        // greater than our own cannot be loaded safely.
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
            file_size,
            section_table_offset,
            section_count,
            section_entry_size,
            minimum_runtime_version,
            header_crc32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal valid 64-byte header for tests.
    fn build_header(
        format_version: u16,
        schema_version: u16,
        flags: u32,
        file_size: u64,
        table_offset: u64,
        count: u32,
        runtime: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(b"OOPSMDAT");
        buf.extend_from_slice(&format_version.to_le_bytes());
        buf.extend_from_slice(&schema_version.to_le_bytes());
        buf.extend_from_slice(&64u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // reserved
        buf.extend_from_slice(&flags.to_le_bytes());
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(&table_offset.to_le_bytes());
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&48u32.to_le_bytes());
        buf.extend_from_slice(&runtime.to_le_bytes());
        buf.extend_from_slice(&[0u8; 16]); // CRC placeholder + reserved
        buf
    }

    /// Sets the CRC field as the Python writer would.
    fn finalize(mut header: Vec<u8>) -> Vec<u8> {
        let mut crc_input = [0u8; 64];
        crc_input[..48].copy_from_slice(&header[..48]);
        let crc = crc32fast::hash(&crc_input);
        header[48..52].copy_from_slice(&crc.to_le_bytes());
        header
    }

    #[test]
    fn validates_minimal_header() {
        let header = finalize(build_header(1, 1, 0, 128, 64, 1, CURRENT_RUNTIME_VERSION));
        let mut data = header;
        data.resize(128, 0);
        let parsed = DatHeader::parse(&data).unwrap();
        assert_eq!(parsed.section_count, 1);
        assert_eq!(parsed.section_table_offset, 64);
        assert_eq!(parsed.format_version, DAT_FORMAT_VERSION);
    }

    #[test]
    #[test]
    fn rejects_bad_magic() {
        let mut header = finalize(build_header(1, 1, 0, 128, 64, 0, 0));
        header[0] = b'X';
        let mut data = header;
        data.resize(128, 0);
        assert_eq!(DatHeader::parse(&data), Err(HeaderError::InvalidMagic));
    }

    #[test]
    fn rejects_unsupported_format_version() {
        let header = finalize(build_header(2, 1, 0, 128, 64, 0, 0));
        let mut data = header;
        data.resize(128, 0);
        assert_eq!(
            DatHeader::parse(&data),
            Err(HeaderError::UnsupportedFormatVersion(2))
        );
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let header = finalize(build_header(1, 2, 0, 128, 64, 0, 0));
        let mut data = header;
        data.resize(128, 0);
        assert_eq!(
            DatHeader::parse(&data),
            Err(HeaderError::UnsupportedSchemaVersion(2))
        );
    }

    #[test]
    fn rejects_wrong_header_size() {
        let mut header = build_header(1, 1, 0, 128, 64, 0, 0);
        header[12] = 32; // header_size = 32 (little endian)
        header = finalize(header);
        let mut data = header;
        data.resize(128, 0);
        assert_eq!(
            DatHeader::parse(&data),
            Err(HeaderError::InvalidHeaderSize(32))
        );
    }

    #[test]
    fn rejects_nonzero_reserved_u16() {
        let mut header = build_header(1, 1, 0, 128, 64, 0, 0);
        header[14] = 1;
        header = finalize(header);
        let mut data = header;
        data.resize(128, 0);
        assert_eq!(
            DatHeader::parse(&data),
            Err(HeaderError::NonZeroReservedBytes { offset: 14 })
        );
    }

    #[test]
    fn rejects_nonzero_reserved_tail() {
        let mut header = build_header(1, 1, 0, 128, 64, 0, 0);
        header[60] = 7;
        header = finalize(header);
        let mut data = header;
        data.resize(128, 0);
        assert_eq!(
            DatHeader::parse(&data),
            Err(HeaderError::NonZeroReservedBytes { offset: 60 })
        );
    }

    #[test]
    fn rejects_incorrect_crc() {
        let mut header = finalize(build_header(1, 1, 0, 128, 64, 0, 0));
        header[49] ^= 0xFF;
        let mut data = header;
        data.resize(128, 0);
        match DatHeader::parse(&data) {
            Err(HeaderError::HeaderCrcMismatch { .. }) => {}
            other => panic!("expected CRC mismatch, got {other:?}"),
        }
    }

    #[test]
    fn rejects_file_size_mismatch() {
        let header = finalize(build_header(1, 1, 0, 256, 64, 0, 0));
        let mut data = header;
        data.resize(128, 0);
        assert_eq!(
            DatHeader::parse(&data),
            Err(HeaderError::FileSizeMismatch {
                declared: 256,
                actual: 128,
            })
        );
    }

    #[test]
    fn rejects_wrong_entry_size() {
        let mut header = build_header(1, 1, 0, 128, 64, 0, 0);
        header[40] = 16; // section_entry_size = 16
        header = finalize(header);
        let mut data = header;
        data.resize(128, 0);
        assert_eq!(
            DatHeader::parse(&data),
            Err(HeaderError::InvalidSectionEntrySize(16))
        );
    }

    #[test]
    fn rejects_truncated_header() {
        let header = finalize(build_header(1, 1, 0, 64, 64, 0, 0));
        let data = &header[..32];
        assert_eq!(
            DatHeader::parse(data),
            Err(HeaderError::HeaderTooSmall { actual: 32 })
        );
    }

    #[test]
    fn rejects_newer_runtime_requirement() {
        let required = (2u32 << 16) | (1u32 << 8) | 3;
        let header = finalize(build_header(1, 1, 0, 128, 64, 0, required));
        let mut data = header;
        data.resize(128, 0);
        assert_eq!(
            DatHeader::parse(&data),
            Err(HeaderError::RuntimeVersionIncompatible {
                required,
                current: CURRENT_RUNTIME_VERSION,
            })
        );
    }
}
