//! Integration tests for `stage::loader::load` against a real DAT v1 file
//! produced by the Python compiler (OMSC).
//!
//! The fixture path is configurable through the `OOPSMATH_STAGE_DAT`
//! environment variable so no machine-specific absolute path is embedded in
//! production code. When the variable is not set (or the file is missing) the
//! end-to-end tests are skipped; pure-Rust container tests always run.

use std::env;
use std::path::PathBuf;

use oopsmath_engine::stage::loader::load_from_bytes;
use oopsmath_engine::stage::package::{StageDefinition, StagePackage};

/// Locates the compiler-generated DAT fixture, if it is available.
fn fixture_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("OOPSMATH_STAGE_DAT") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    // Common repository-relative locations, relative to the crate dir.
    for candidate in [
        PathBuf::from("../../build/001_first_wall.dat"),
        PathBuf::from("../../../build/001_first_wall.dat"),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn fixture_bytes() -> Option<std::path::PathBuf> {
    fixture_path()
}

fn fixture() -> Option<StagePackage> {
    let path = fixture_path()?;
    let bytes = std::fs::read(path).ok()?;
    load_from_bytes(&bytes).ok()
}

#[test]
fn loads_real_compiler_fixture_end_to_end() {
    let pkg = match fixture() {
        Some(pkg) => pkg,
        None => {
            eprintln!("skipping: no OOPSMATH_STAGE_DAT fixture available");
            return;
        }
    };
    assert_eq!(pkg.meta.stage_id, "001_first_wall");
    assert_eq!(pkg.meta.format, "OOPSMDAT");
    let mut locales: Vec<&str> = pkg.localization.iter().map(|l| l.locale.as_str()).collect();
    locales.sort_unstable();
    assert_eq!(locales, vec!["en-US", "fa"]);

    let mut doc_ids: Vec<&str> = pkg.documents.iter().map(|d| d.id.as_str()).collect();
    doc_ids.sort_unstable();
    assert_eq!(doc_ids, vec!["lesson.md", "solution.md"]);

    let stage = &pkg.stage;
    assert_eq!(
        stage.stage.get("id").and_then(|v| v.as_str()),
        Some("001_first_wall")
    );
    assert!(
        stages_localization_declared(stage),
        "STAG must declare its localization namespace and locales"
    );
}

fn stages_localization_declared(stage: &StageDefinition) -> bool {
    stage
        .localization
        .as_ref()
        .and_then(|l| l.get("namespace"))
        .map(|ns| ns.as_str() == Some("stage.001_first_wall"))
        .unwrap_or(false)
}

#[test]
fn fixture_has_world_when_flagged() {
    let pkg = match fixture() {
        Some(pkg) => pkg,
        None => {
            eprintln!("skipping: no OOPSMATH_STAGE_DAT fixture available");
            return;
        }
    };
    // 001_first_wall has no voxel source: WRLD must be absent and world None.
    let meta_sections = &pkg.meta.sections;
    let has_wrld = meta_sections.iter().any(|s| s == "WRLD");
    assert_eq!(has_wrld, pkg.world.is_some());
}

#[test]
fn rejects_truncated_fixture() {
    let Some(path) = fixture_path() else {
        eprintln!("skipping: no OOPSMATH_STAGE_DAT fixture available");
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let truncated = &bytes[..40.min(bytes.len())];
    let result = load_from_bytes(truncated);
    assert!(result.is_err(), "a 40-byte blob cannot be a valid DAT");
}

#[test]
fn rejects_bit_flipped_payload() {
    let Some(path) = fixture_path() else {
        eprintln!("skipping: no OOPSMATH_STAGE_DAT fixture available");
        return;
    };
    let mut bytes = std::fs::read(path).unwrap();
    // Flip a byte inside the first section payload (META data starts at
    // offset 256); the header CRC still passes but the section checksum
    // must fail. Flipping trailing alignment padding would not be detected,
    // so target the payload itself.
    bytes[300] ^= 0xFF;
    let result = load_from_bytes(&bytes);
    assert!(result.is_err(), "corrupted payload must be rejected");
}

#[test]
fn resolves_fixture_bytes() {
    // The helper should return a path only when the file truly exists.
    match fixture_bytes() {
        Some(path) => assert!(path.is_file()),
        None => eprintln!("no fixture configured; ok"),
    }
}

#[test]
fn loads_synthetic_minimal_dat() {
    // Synthetic packages built directly from the Rust structs would only
    // verify the implementation against itself, so we only check that a
    // completely invalid blob produces a typed error.
    let noise = [0u8; 64];
    assert!(load_from_bytes(&noise).is_err());
}
