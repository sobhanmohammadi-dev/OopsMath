//! Tests for `dat::directory`.

use crate::stage::dat::directory::{Compression, SECTION_ALIGNMENT, SectionDirectory};
use crate::stage::dat::error::SectionError;
use crate::stage::dat::flags::{HEADER_FLAG_HAS_WORLD, SECTION_FLAG_CRITICAL};
use crate::stage::dat::header::DatHeader;
use crate::stage::dat::section_type::SectionType;

use super::support::{
    ENTRY_FLAGS, ENTRY_OFFSET, ENTRY_RESERVED, ENTRY_STORED_SIZE, HeaderSpec, SectionSpec,
    build_dat, entry_base, finalize, pad, patch_u32, patch_u64, read_u64,
};

const ZZZZ: SectionType = SectionType::new(*b"ZZZZ");

fn meta_and_stag() -> Vec<SectionSpec> {
    vec![
        SectionSpec::new(b"META", vec![1, 2, 3]),
        SectionSpec::new(b"STAG", vec![4, 5, 6, 7]),
    ]
}

fn parse(data: &[u8]) -> Result<SectionDirectory, SectionError> {
    let header = DatHeader::parse(data).expect("test DAT has a valid header");
    SectionDirectory::parse(data, &header)
}

#[test]
fn parses_required_sections() {
    let data = build_dat(0, &meta_and_stag());
    let directory = parse(&data).unwrap();

    let meta = directory.get(SectionType::META).unwrap();
    assert_eq!(meta.raw_size, 3);
    assert_eq!(meta.stored_size, 3);
    assert_eq!(meta.crc32, crc32fast::hash(&[1, 2, 3]));
    assert_eq!(meta.compression, Compression::None);
    assert_eq!(meta.offset % SECTION_ALIGNMENT, 0);

    assert!(directory.contains(SectionType::STAG));
    assert!(!directory.contains(SectionType::WRLD));
}

#[test]
fn reports_zstd_compression() {
    let sections = vec![
        SectionSpec::new(b"META", vec![9; 100]).zstd(),
        SectionSpec::new(b"STAG", vec![1]),
    ];
    let directory = parse(&build_dat(0, &sections)).unwrap();
    let meta = directory.get(SectionType::META).unwrap();
    assert_eq!(meta.compression, Compression::Zstd);
    assert_eq!(meta.raw_size, 100);
    assert!(meta.stored_size < 100);
}

#[test]
fn rejects_missing_stag() {
    let data = build_dat(0, &meta_and_stag()[..1]);
    assert_eq!(
        parse(&data).unwrap_err(),
        SectionError::MissingRequiredSection(SectionType::STAG)
    );
}

#[test]
fn rejects_missing_meta() {
    let data = build_dat(0, &meta_and_stag()[1..]);
    assert_eq!(
        parse(&data).unwrap_err(),
        SectionError::MissingRequiredSection(SectionType::META)
    );
}

#[test]
fn rejects_duplicate_section() {
    let mut sections = meta_and_stag();
    sections.push(SectionSpec::new(b"META", vec![0]));
    assert_eq!(
        parse(&build_dat(0, &sections)).unwrap_err(),
        SectionError::DuplicateSection(SectionType::META)
    );
}

#[test]
fn rejects_header_flag_without_section() {
    let data = build_dat(HEADER_FLAG_HAS_WORLD, &meta_and_stag());
    assert_eq!(
        parse(&data).unwrap_err(),
        SectionError::MissingRequiredSection(SectionType::WRLD)
    );
}

#[test]
fn accepts_header_flag_with_section() {
    let mut sections = meta_and_stag();
    sections.push(SectionSpec::new(b"WRLD", vec![0; 8]));
    let directory = parse(&build_dat(HEADER_FLAG_HAS_WORLD, &sections)).unwrap();
    assert!(directory.contains(SectionType::WRLD));
}

#[test]
fn rejects_misaligned_offset() {
    let mut data = build_dat(0, &meta_and_stag());
    let base = entry_base(0);
    let offset = read_u64(&data, base + ENTRY_OFFSET) + 1;
    patch_u64(&mut data, base + ENTRY_OFFSET, offset);
    assert_eq!(
        parse(&data).unwrap_err(),
        SectionError::InvalidSectionAlignment {
            section: SectionType::META,
            offset,
            alignment: SECTION_ALIGNMENT,
        }
    );
}

#[test]
fn rejects_section_past_end_of_file() {
    let mut data = build_dat(0, &meta_and_stag());
    patch_u64(&mut data, entry_base(0) + ENTRY_STORED_SIZE, 1 << 20);
    assert!(matches!(
        parse(&data).unwrap_err(),
        SectionError::SectionOutOfBounds {
            section: SectionType::META,
            ..
        }
    ));
}

#[test]
fn rejects_section_range_overflow() {
    let mut data = build_dat(0, &meta_and_stag());
    patch_u64(&mut data, entry_base(0) + ENTRY_STORED_SIZE, u64::MAX);
    assert!(matches!(
        parse(&data).unwrap_err(),
        SectionError::SectionRangeOverflow {
            section: SectionType::META,
            ..
        }
    ));
}

#[test]
fn rejects_overlapping_sections() {
    let mut data = build_dat(0, &meta_and_stag());
    let meta_offset = read_u64(&data, entry_base(0) + ENTRY_OFFSET);
    patch_u64(&mut data, entry_base(1) + ENTRY_OFFSET, meta_offset);
    assert_eq!(
        parse(&data).unwrap_err(),
        SectionError::SectionOverlap {
            section: SectionType::STAG,
            overlaps_with: SectionType::META,
        }
    );
}

#[test]
fn rejects_section_inside_directory() {
    let mut data = build_dat(0, &meta_and_stag());
    patch_u64(&mut data, entry_base(0) + ENTRY_OFFSET, 16);
    assert_eq!(
        parse(&data).unwrap_err(),
        SectionError::SectionOverlapsMetadata {
            section: SectionType::META,
        }
    );
}

#[test]
fn rejects_nonzero_reserved_entry_bytes() {
    let mut data = build_dat(0, &meta_and_stag());
    data[entry_base(0) + ENTRY_RESERVED] = 1;
    assert_eq!(
        parse(&data).unwrap_err(),
        SectionError::NonZeroReservedSectionBytes {
            section: SectionType::META,
        }
    );
}

#[test]
fn rejects_reserved_compression_id() {
    let mut sections = meta_and_stag();
    sections[0] = sections[0].clone().with_flags(2);
    assert_eq!(
        parse(&build_dat(0, &sections)).unwrap_err(),
        SectionError::UnsupportedCompression {
            section: SectionType::META,
            compression: 2,
        }
    );
}

#[test]
fn rejects_unknown_flags_on_critical_section() {
    let mut data = build_dat(0, &meta_and_stag());
    let flags = SECTION_FLAG_CRITICAL | (1 << 20);
    patch_u32(&mut data, entry_base(0) + ENTRY_FLAGS, flags);
    assert_eq!(
        parse(&data).unwrap_err(),
        SectionError::UnknownSectionFlags {
            section: SectionType::META,
            flags,
        }
    );
}

#[test]
fn tolerates_unknown_flags_on_non_critical_section() {
    let mut data = build_dat(0, &meta_and_stag());
    patch_u32(&mut data, entry_base(0) + ENTRY_FLAGS, 1 << 20);
    assert!(parse(&data).is_ok());
}

#[test]
fn rejects_unknown_critical_section() {
    let mut sections = meta_and_stag();
    sections.push(SectionSpec::new(b"ZZZZ", vec![0]).with_flags(SECTION_FLAG_CRITICAL));
    assert_eq!(
        parse(&build_dat(0, &sections)).unwrap_err(),
        SectionError::UnknownCriticalSection(ZZZZ)
    );
}

#[test]
fn skips_unknown_non_critical_section() {
    let mut sections = meta_and_stag();
    sections.push(SectionSpec::new(b"ZZZZ", vec![0]));
    let directory = parse(&build_dat(0, &sections)).unwrap();
    assert!(!directory.contains(ZZZZ));
    assert!(directory.contains(SectionType::STAG));
}

#[test]
fn rejects_non_printable_section_type() {
    let mut sections = meta_and_stag();
    sections.push(SectionSpec::new(&[b'M', 0x00, b'T', b'A'], vec![0]));
    assert_eq!(
        parse(&build_dat(0, &sections)).unwrap_err(),
        SectionError::InvalidSectionType {
            raw: [b'M', 0x00, b'T', b'A'],
        }
    );
}

#[test]
fn rejects_table_before_end_of_header() {
    let header_bytes = HeaderSpec {
        table_offset: 32,
        ..HeaderSpec::default()
    }
    .build();
    let data = pad(finalize(header_bytes), 128);
    assert_eq!(
        parse(&data).unwrap_err(),
        SectionError::InvalidSectionTableOffset { offset: 32 }
    );
}

#[test]
fn rejects_table_past_end_of_file() {
    let header_bytes = HeaderSpec {
        section_count: 1000,
        ..HeaderSpec::default()
    }
    .build();
    let data = pad(finalize(header_bytes), 128);
    assert!(matches!(
        parse(&data).unwrap_err(),
        SectionError::SectionDirectoryOutOfBounds { count: 1000, .. }
    ));
}
