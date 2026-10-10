//! Integration tests for the stage catalog against the committed runtime
//! stage fixture (`stages/001_first_wall.dat`, produced by the Python OMSC
//! compiler).
//!
//! The directory is looked up at the path in `OOPSMATH_STAGES_DIR` or the
//! workspace-root `stages/` directory. When neither exists the tests print a
//! note and return early; the synthetic-package behaviour is covered by the
//! unit tests in `src/stage/tests/catalog.rs`.

use std::path::PathBuf;

use oopsmath_engine::stage::catalog::StageCatalog;
use oopsmath_engine::stage::load;
use oopsmath_engine::stage::localization::StageLocale;

/// The runtime stage directory, when it exists.
fn stages_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OOPSMATH_STAGES_DIR") {
        let dir = PathBuf::from(dir);
        if dir.is_dir() {
            return Some(dir);
        }
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stages");
    dir.is_dir().then_some(dir)
}

/// Discovers the catalog, skipping when the fixture is unavailable.
fn fixture_catalog() -> Option<StageCatalog> {
    let Some(dir) = stages_dir() else {
        eprintln!("skipping: no stages/ directory");
        return None;
    };
    Some(StageCatalog::discover(dir))
}

#[test]
fn discovers_the_compiler_fixture_with_resolved_metadata() {
    let Some(catalog) = fixture_catalog() else {
        return;
    };
    let Some(entry) = catalog.entry("001_first_wall") else {
        eprintln!("skipping: 001_first_wall.dat not present");
        return;
    };

    assert!(entry.dat_path.is_file());
    assert_eq!(entry.level, Some(1));
    assert_eq!(entry.difficulty.as_deref(), Some("tutorial"));
    assert_eq!(entry.grade_min, Some(7));
    assert_eq!(entry.grade_max, Some(8));
    assert_eq!(entry.topics, ["multiplication"]);
    assert_eq!(entry.locales, ["en-US", "fa"]);

    // World configuration comes from `StageDefinition`, even though the
    // fixture ships no compiled `WRLD` section.
    assert_eq!(entry.world_size, Some([16, 8, 16]));
    assert_eq!(entry.environment_preset.as_deref(), Some("meadow"));
    assert_eq!(entry.objective_task_count, Some(2));

    let en = StageLocale::from("en-US");
    assert_eq!(entry.translated_title(&en), Some("My first stage"));
    assert_eq!(entry.translated_description(&en), Some("Build a wall."));
    assert!(entry.translated_question(&en).is_some());
}

#[test]
fn selected_stage_path_loads_with_the_existing_loader() {
    let Some(catalog) = fixture_catalog() else {
        return;
    };
    let Some(entry) = catalog.entry("001_first_wall") else {
        eprintln!("skipping: 001_first_wall.dat not present");
        return;
    };

    let package = load(&entry.dat_path).expect("the selected stage must load");
    assert_eq!(package.meta.stage_id, entry.stage_id);
    assert!(package.world.is_none(), "the fixture ships no WRLD section");
    assert_eq!(package.localization.len(), 2);
}

#[test]
fn a_corrupt_package_does_not_hide_the_valid_one() {
    let Some(dir) = stages_dir() else { return };
    let Some(valid) = StageCatalog::discover(&dir)
        .entry("001_first_wall")
        .cloned()
    else {
        eprintln!("skipping: 001_first_wall.dat not present");
        return;
    };

    // Copy the valid fixture next to a corrupt file in a scratch directory.
    let scratch = std::env::temp_dir().join(format!("oopsmath-catalog-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    std::fs::copy(&valid.dat_path, scratch.join("good.dat")).expect("copy fixture");
    std::fs::write(scratch.join("broken.dat"), b"definitely not a DAT package").expect("write");

    let catalog = StageCatalog::discover(&scratch);
    let _ = std::fs::remove_dir_all(&scratch);

    assert_eq!(catalog.len(), 1, "the valid package is still discovered");
    assert_eq!(
        catalog.diagnostics.len(),
        1,
        "the corrupt package is reported"
    );
}
