//! Integration tests for `stage::load_from_bytes` against a real DAT v1 file
//! produced by the Python compiler (OMSC).
//!
//! The fixture is looked up, in order, at:
//!
//! 1. the path in the `OOPSMATH_STAGE_DAT` environment variable;
//! 2. `stages/001_first_wall.dat` in the workspace root (the runtime stage
//!    directory);
//! 3. `build/001_first_wall.dat` in the workspace root.
//!
//! When no fixture exists the tests print a note and return early. A fixture
//! that exists but fails to load is a test failure, never a skip.
//!
//! Synthetic DAT files (container, corruption and decoder cases) are covered
//! by the unit tests in `src/stage/tests/`.

use std::env;
use std::path::PathBuf;

use oopsmath_engine::stage::error::{HeaderError, SectionError, StageLoadError};
use oopsmath_engine::stage::{StagePackage, load_from_bytes};

/// Locates the compiler-generated DAT fixture, if available.
fn fixture_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("OOPSMATH_STAGE_DAT").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    ["stages/001_first_wall.dat", "build/001_first_wall.dat"]
        .into_iter()
        .map(|relative| root.join(relative))
        .find(|path| path.is_file())
}

/// The fixture bytes, or `None` (with a note) when no fixture is available.
fn fixture_bytes() -> Option<Vec<u8>> {
    let Some(path) = fixture_path() else {
        eprintln!("skipping: no stage DAT fixture (set OOPSMATH_STAGE_DAT or build 001_first_wall)");
        return None;
    };
    Some(std::fs::read(&path).unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display())))
}

fn load_fixture() -> Option<StagePackage> {
    let bytes = fixture_bytes()?;
    Some(load_from_bytes(&bytes).expect("the compiler fixture must load"))
}

/// Little-endian `u64` at `at`.
fn read_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

#[test]
fn loads_compiler_fixture_end_to_end() {
    let Some(package) = load_fixture() else { return };

    assert_eq!(package.meta.stage_id, "001_first_wall");
    assert_eq!(package.meta.format, "OOPSMDAT");
    assert_eq!(
        package.stage.stage.get("id").and_then(|id| id.as_str()),
        Some("001_first_wall")
    );

    let mut locales: Vec<&str> = package
        .localization
        .iter()
        .map(|localization| localization.locale.as_str())
        .collect();
    locales.sort_unstable();
    assert_eq!(locales, ["en-US", "fa"]);

    let mut document_ids: Vec<&str> = package.documents.iter().map(|d| d.id.as_str()).collect();
    document_ids.sort_unstable();
    assert_eq!(document_ids, ["lesson.md", "solution.md"]);
}

#[test]
fn fixture_declares_its_localization_namespace() {
    let Some(package) = load_fixture() else { return };

    let namespace = package
        .stage
        .localization
        .as_ref()
        .and_then(|localization| localization.get("namespace"))
        .and_then(|namespace| namespace.as_str());
    assert_eq!(namespace, Some("stage.001_first_wall"));
}

#[test]
fn fixture_world_matches_meta_sections() {
    let Some(package) = load_fixture() else { return };

    // The package has a world exactly when META lists a WRLD section.
    let lists_world = package.meta.sections.iter().any(|section| section == "WRLD");
    assert_eq!(package.world.is_some(), lists_world);
}

#[test]
fn rejects_truncated_fixture() {
    let Some(bytes) = fixture_bytes() else { return };

    let err = load_from_bytes(&bytes[..40]).unwrap_err();
    assert!(matches!(
        err,
        StageLoadError::Header(HeaderError::HeaderTooSmall { actual: 40 })
    ));
}

#[test]
fn rejects_fixture_with_corrupted_section_payload() {
    let Some(mut bytes) = fixture_bytes() else { return };

    // Locate the first section payload through the directory: the table offset
    // is a u64 at header byte 28 and the first entry's offset is a u64 at
    // entry byte 8. Flipping a payload byte leaves the header CRC intact, so
    // the per-section checksum must catch it.
    let table_offset = read_u64(&bytes, 28) as usize;
    let payload_offset = read_u64(&bytes, table_offset + 8) as usize;
    bytes[payload_offset] ^= 0xFF;

    let err = load_from_bytes(&bytes).unwrap_err();
    assert!(
        matches!(
            err,
            StageLoadError::Section(
                SectionError::SectionChecksumMismatch { .. }
                    | SectionError::DecompressionFailed { .. }
            )
        ),
        "unexpected error: {err}"
    );
}
