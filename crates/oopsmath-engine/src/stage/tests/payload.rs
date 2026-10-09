//! Tests for `dat::payload`.

use crate::stage::dat::directory::SectionDirectory;
use crate::stage::dat::error::SectionError;
use crate::stage::dat::header::DatHeader;
use crate::stage::dat::payload::{MAX_SECTION_RAW_SIZE, load_section_payload};
use crate::stage::dat::section_type::SectionType;

use super::support::{
    ENTRY_CRC, ENTRY_OFFSET, ENTRY_RAW_SIZE, SectionSpec, build_dat, entry_base, patch_u32, patch_u64, read_u64,
};

fn stag() -> SectionSpec {
    SectionSpec::new(b"STAG", vec![1])
}

fn load(data: &[u8], section: SectionType) -> Result<Vec<u8>, SectionError> {
    let header = DatHeader::parse(data).expect("test DAT has a valid header");
    let directory = SectionDirectory::parse(data, &header).expect("test DAT has a valid directory");
    load_section_payload(data, &directory, section)
}

#[test]
fn reads_uncompressed_payload() {
    let data = build_dat(0, &[SectionSpec::new(b"META", b"hello".to_vec()), stag()]);
    assert_eq!(load(&data, SectionType::META).unwrap(), b"hello");
}

#[test]
fn reads_zstd_payload() {
    let raw: Vec<u8> = (0..2000u32).map(|n| (n % 7) as u8).collect();
    let data = build_dat(0, &[SectionSpec::new(b"META", raw.clone()).zstd(), stag()]);
    assert_eq!(load(&data, SectionType::META).unwrap(), raw);
}

#[test]
fn reads_empty_payload() {
    let data = build_dat(0, &[SectionSpec::new(b"META", Vec::new()), stag()]);
    assert!(load(&data, SectionType::META).unwrap().is_empty());
}

#[test]
fn rejects_absent_section() {
    let data = build_dat(0, &[SectionSpec::new(b"META", vec![1]), stag()]);
    assert_eq!(
        load(&data, SectionType::WRLD).unwrap_err(),
        SectionError::MissingRequiredSection(SectionType::WRLD)
    );
}

#[test]
fn rejects_checksum_mismatch() {
    let mut data = build_dat(0, &[SectionSpec::new(b"META", b"hello".to_vec()), stag()]);
    patch_u32(&mut data, entry_base(0) + ENTRY_CRC, 0xDEAD_BEEF);
    assert!(matches!(
        load(&data, SectionType::META).unwrap_err(),
        SectionError::SectionChecksumMismatch {
            section: SectionType::META,
            expected: 0xDEAD_BEEF,
            ..
        }
    ));
}

#[test]
fn rejects_corrupted_payload_byte() {
    let mut data = build_dat(0, &[SectionSpec::new(b"META", b"hello".to_vec()), stag()]);
    let offset = read_u64(&data, entry_base(0) + ENTRY_OFFSET) as usize;
    data[offset] ^= 0xFF;
    assert!(matches!(
        load(&data, SectionType::META).unwrap_err(),
        SectionError::SectionChecksumMismatch { .. }
    ));
}

#[test]
fn rejects_raw_size_mismatch_without_compression() {
    let mut data = build_dat(0, &[SectionSpec::new(b"META", b"hello".to_vec()), stag()]);
    patch_u64(&mut data, entry_base(0) + ENTRY_RAW_SIZE, 6);
    assert_eq!(
        load(&data, SectionType::META).unwrap_err(),
        SectionError::RawSizeMismatch {
            section: SectionType::META,
            expected: 6,
            actual: 5,
        }
    );
}

#[test]
fn rejects_raw_size_mismatch_with_zstd() {
    let mut data = build_dat(0, &[SectionSpec::new(b"META", vec![3; 64]).zstd(), stag()]);
    patch_u64(&mut data, entry_base(0) + ENTRY_RAW_SIZE, 74);
    assert_eq!(
        load(&data, SectionType::META).unwrap_err(),
        SectionError::RawSizeMismatch {
            section: SectionType::META,
            expected: 74,
            actual: 64,
        }
    );
}

#[test]
fn rejects_oversized_declared_raw_size() {
    let mut data = build_dat(0, &[SectionSpec::new(b"META", vec![1]), stag()]);
    patch_u64(
        &mut data,
        entry_base(0) + ENTRY_RAW_SIZE,
        MAX_SECTION_RAW_SIZE + 1,
    );
    assert_eq!(
        load(&data, SectionType::META).unwrap_err(),
        SectionError::DecompressedSizeLimitExceeded {
            section: SectionType::META,
            declared: MAX_SECTION_RAW_SIZE + 1,
            limit: MAX_SECTION_RAW_SIZE,
        }
    );
}

#[test]
fn rejects_garbage_marked_as_zstd() {
    // Flag bit 0 selects zstd, but the stored bytes are not a zstd frame.
    let garbage = SectionSpec::new(b"META", vec![0xAB; 32]).with_flags(1);
    let data = build_dat(0, &[garbage, stag()]);
    assert!(matches!(
        load(&data, SectionType::META).unwrap_err(),
        SectionError::DecompressionFailed {
            section: SectionType::META,
            ..
        }
    ));
}
