//! Tests for `dat::header`.

use crate::stage::dat::error::HeaderError;
use crate::stage::dat::flags::{HEADER_FLAG_HAS_WORLD, HEADER_FLAGS_KNOWN};
use crate::stage::dat::header::{CURRENT_RUNTIME_VERSION, DAT_FORMAT_VERSION, DatHeader};

use super::support::{HeaderSpec, finalize, pad};

/// Finalizes `header` and pads it into a 128-byte file.
fn file_of(header: Vec<u8>) -> Vec<u8> {
    pad(finalize(header), 128)
}

fn parse_spec(spec: HeaderSpec) -> Result<DatHeader, HeaderError> {
    DatHeader::parse(&file_of(spec.build()))
}

#[test]
fn accepts_minimal_header() {
    let header = parse_spec(HeaderSpec {
        section_count: 1,
        runtime_version: CURRENT_RUNTIME_VERSION,
        ..HeaderSpec::default()
    })
    .unwrap();
    assert_eq!(header.section_count, 1);
    assert_eq!(header.section_table_offset, 64);
    assert_eq!(header.format_version, DAT_FORMAT_VERSION);
    assert_eq!(header.header_size, 64);
}

#[test]
fn accepts_every_known_header_flag() {
    let header = parse_spec(HeaderSpec {
        flags: HEADER_FLAGS_KNOWN,
        ..HeaderSpec::default()
    })
    .unwrap();
    assert_eq!(header.flags, HEADER_FLAGS_KNOWN);
}

#[test]
fn rejects_bad_magic() {
    let mut header = HeaderSpec::default().build();
    header[0] = b'X';
    assert_eq!(
        DatHeader::parse(&file_of(header)),
        Err(HeaderError::InvalidMagic)
    );
}

#[test]
fn rejects_unsupported_format_version() {
    let result = parse_spec(HeaderSpec {
        format_version: 2,
        ..HeaderSpec::default()
    });
    assert_eq!(result, Err(HeaderError::UnsupportedFormatVersion(2)));
}

#[test]
fn rejects_unsupported_schema_version() {
    let result = parse_spec(HeaderSpec {
        schema_version: 2,
        ..HeaderSpec::default()
    });
    assert_eq!(result, Err(HeaderError::UnsupportedSchemaVersion(2)));
}

#[test]
fn rejects_wrong_header_size() {
    let mut header = HeaderSpec::default().build();
    header[12] = 32;
    assert_eq!(
        DatHeader::parse(&file_of(header)),
        Err(HeaderError::InvalidHeaderSize(32))
    );
}

#[test]
fn rejects_nonzero_reserved_u16() {
    let mut header = HeaderSpec::default().build();
    header[14] = 1;
    assert_eq!(
        DatHeader::parse(&file_of(header)),
        Err(HeaderError::NonZeroReservedBytes { offset: 14 })
    );
}

#[test]
fn rejects_nonzero_reserved_tail() {
    let mut header = HeaderSpec::default().build();
    header[60] = 7;
    assert_eq!(
        DatHeader::parse(&file_of(header)),
        Err(HeaderError::NonZeroReservedBytes { offset: 60 })
    );
}

#[test]
fn rejects_unknown_header_flags() {
    let unknown = 1 << 20;
    let result = parse_spec(HeaderSpec {
        flags: HEADER_FLAG_HAS_WORLD | unknown,
        ..HeaderSpec::default()
    });
    assert_eq!(
        result,
        Err(HeaderError::UnknownHeaderFlags(HEADER_FLAG_HAS_WORLD | unknown))
    );
}

#[test]
fn rejects_incorrect_crc() {
    let mut header = finalize(HeaderSpec::default().build());
    header[49] ^= 0xFF;
    let result = DatHeader::parse(&pad(header, 128));
    assert!(matches!(result, Err(HeaderError::HeaderCrcMismatch { .. })));
}

#[test]
fn rejects_file_size_mismatch() {
    let result = parse_spec(HeaderSpec {
        file_size: 256,
        ..HeaderSpec::default()
    });
    assert_eq!(
        result,
        Err(HeaderError::FileSizeMismatch {
            declared: 256,
            actual: 128,
        })
    );
}

#[test]
fn rejects_wrong_entry_size() {
    let mut header = HeaderSpec::default().build();
    header[40] = 16;
    assert_eq!(
        DatHeader::parse(&file_of(header)),
        Err(HeaderError::InvalidSectionEntrySize(16))
    );
}

#[test]
fn rejects_truncated_header() {
    let header = finalize(HeaderSpec::default().build());
    assert_eq!(
        DatHeader::parse(&header[..32]),
        Err(HeaderError::HeaderTooSmall { actual: 32 })
    );
}

#[test]
fn rejects_newer_runtime_requirement() {
    let required = (2u32 << 16) | (1u32 << 8) | 3;
    let result = parse_spec(HeaderSpec {
        runtime_version: required,
        ..HeaderSpec::default()
    });
    assert_eq!(
        result,
        Err(HeaderError::RuntimeVersionIncompatible {
            required,
            current: CURRENT_RUNTIME_VERSION,
        })
    );
}
