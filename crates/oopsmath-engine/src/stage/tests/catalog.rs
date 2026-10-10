//! Unit tests for `stage::catalog`.
//!
//! Each test builds real DAT v1 bytes with the shared byte builders and writes
//! them into a unique temporary directory, so discovery is exercised against
//! the same container the loader validates.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_bytes::ByteBuf;
use serde_json::{Value, json};

use crate::stage::catalog::{
    CatalogDiagnosticKind, STAGES_DIR_ENV, StageCatalog, default_stages_dir,
};
use crate::stage::dat::flags::HEADER_FLAG_HAS_LOCALIZATION;
use crate::stage::localization::StageLocale;

use super::support::{SectionSpec, build_dat};

#[test]
fn default_stages_dir_resolves_a_real_workspace_directory() {
    // The override is the caller's responsibility; skip when it is set so the
    // test never depends on a developer's shell.
    if std::env::var_os(STAGES_DIR_ENV).is_some() {
        return;
    }
    let dir = default_stages_dir();
    if !dir.is_dir() {
        return;
    }
    assert!(
        dir.ends_with("stages"),
        "expected the workspace stages directory, got {}",
        dir.display()
    );
}

/// Self-cleaning temporary directory (no external dependency).
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oopsmath-catalog-{}-{unique}-{label}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        std::fs::write(&path, bytes).expect("write fixture");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn msgpack(value: &Value) -> Vec<u8> {
    rmp_serde::to_vec_named(value).expect("serialize MessagePack")
}

fn meta(stage_id: &str, locales: &[&str]) -> Value {
    json!({
        "format": "OOPSMDAT",
        "format_version": 1,
        "schema_version": 1,
        "compiler": "tests/1.0.0",
        "stage_id": stage_id,
        "title": format!("stage.{stage_id}.title"),
        "grade_min": 7,
        "grade_max": 8,
        "topics": ["multiplication"],
        "locales": locales,
        "sections": ["META", "STAG"],
        "content_sha256": "",
    })
}

fn stag(stage_id: &str) -> Value {
    json!({
        "schema_version": 1,
        "stage": { "id": stage_id, "title": format!("stage.{stage_id}.title") },
        "learning": { "question": format!("stage.{stage_id}.question") },
        "world": {
            "environment": { "preset": "meadow" },
            "bounds": { "origin": [0, 0, 0], "size": [16, 8, 16] },
        },
        "construction": {},
        "objectives": { "tasks": [{ "id": "t1" }, { "id": "t2" }] },
        "rewards": { "money": 100, "xp": 10 },
    })
}

/// Builds a valid DAT with optional `level` and stage-local localization.
fn stage_dat(stage_id: &str, level: Option<i64>, localization: &[(&str, &str)]) -> Vec<u8> {
    let locales: Vec<&str> = localization.iter().map(|(locale, _)| *locale).collect();
    let mut metadata = meta(stage_id, &locales);
    metadata["description"] = json!(format!("stage.{stage_id}.description"));
    metadata["difficulty"] = json!("tutorial");
    if let Some(level) = level {
        metadata["level"] = json!(level);
    }

    let mut sections = vec![
        SectionSpec::new(b"META", msgpack(&metadata)),
        SectionSpec::new(b"STAG", msgpack(&stag(stage_id))),
    ];
    let mut flags = 0;
    if !localization.is_empty() {
        let mut map: BTreeMap<String, ByteBuf> = BTreeMap::new();
        for (locale, text) in localization {
            map.insert(
                (*locale).to_string(),
                ByteBuf::from(text.as_bytes().to_vec()),
            );
        }
        sections.push(SectionSpec::new(b"LOCL", rmp_serde::to_vec(&map).unwrap()));
        flags |= HEADER_FLAG_HAS_LOCALIZATION;
    }
    build_dat(flags, &sections)
}

fn entry_ids(catalog: &StageCatalog) -> Vec<&str> {
    catalog
        .entries
        .iter()
        .map(|entry| entry.stage_id.as_str())
        .collect()
}

#[test]
fn discovers_valid_packages_and_ignores_other_files() {
    let dir = TempDir::new("discover");
    dir.write("a.dat", &stage_dat("a", Some(1), &[]));
    dir.write("b.dat", &stage_dat("b", Some(2), &[]));
    dir.write("readme.txt", b"not a stage");
    dir.write("notes.json", b"{}");
    std::fs::create_dir_all(dir.path().join("nested.dat")).unwrap();

    let catalog = StageCatalog::discover(dir.path());
    assert_eq!(entry_ids(&catalog), ["a", "b"]);
    assert!(catalog.diagnostics.is_empty());
}

#[test]
fn orders_by_level_then_stage_id() {
    let dir = TempDir::new("order");
    // Written out of order to prove enumeration order is not trusted.
    dir.write("z.dat", &stage_dat("zeta", Some(3), &[]));
    dir.write("a.dat", &stage_dat("alpha", Some(1), &[]));
    dir.write("m.dat", &stage_dat("middle", Some(1), &[]));

    let catalog = StageCatalog::discover(dir.path());
    assert_eq!(entry_ids(&catalog), ["alpha", "middle", "zeta"]);
}

#[test]
fn missing_directory_reports_a_diagnostic() {
    let dir = TempDir::new("missing");
    let missing = dir.path().join("does-not-exist");

    let catalog = StageCatalog::discover(&missing);
    assert!(catalog.is_empty());
    assert_eq!(catalog.diagnostics.len(), 1);
    assert_eq!(
        catalog.diagnostics[0].kind,
        CatalogDiagnosticKind::DirectoryMissing
    );
    assert_eq!(catalog.diagnostics[0].path, missing);
}

#[test]
fn empty_directory_is_not_an_error() {
    let dir = TempDir::new("empty");
    let catalog = StageCatalog::discover(dir.path());
    assert!(catalog.is_empty());
    assert!(catalog.diagnostics.is_empty());
}

#[test]
fn corrupt_package_is_skipped_and_valid_kept() {
    let dir = TempDir::new("corrupt");
    dir.write("broken.dat", b"this is not a DAT file at all");
    dir.write("good.dat", &stage_dat("good", Some(1), &[]));

    let catalog = StageCatalog::discover(dir.path());
    assert_eq!(entry_ids(&catalog), ["good"]);
    assert_eq!(catalog.diagnostics.len(), 1);
    assert_eq!(
        catalog.diagnostics[0].kind,
        CatalogDiagnosticKind::PackageRejected
    );
    assert!(catalog.diagnostics[0].path.ends_with("broken.dat"));
    assert!(!catalog.diagnostics[0].message.is_empty());
}

#[test]
fn duplicate_stage_ids_are_reported_and_deduplicated() {
    let dir = TempDir::new("duplicate");
    dir.write("first.dat", &stage_dat("same", Some(1), &[]));
    dir.write("second.dat", &stage_dat("same", Some(1), &[]));

    let catalog = StageCatalog::discover(dir.path());
    assert_eq!(entry_ids(&catalog), ["same"]);
    assert_eq!(catalog.diagnostics.len(), 1);
    assert_eq!(
        catalog.diagnostics[0].kind,
        CatalogDiagnosticKind::DuplicateStageId
    );
}

#[test]
fn extracts_metadata_and_resolves_localization() {
    let dir = TempDir::new("metadata");
    dir.write(
        "001.dat",
        &stage_dat(
            "001_first_wall",
            Some(1),
            &[
                (
                    "en-US",
                    "stage.001_first_wall.title = My first stage\n\
                     stage.001_first_wall.description = Build a wall.\n\
                     stage.001_first_wall.question = How many bricks?",
                ),
                (
                    "fa",
                    "stage.001_first_wall.title = \u{0645}\u{0631}\u{062d}\u{0644}\u{0647}",
                ),
            ],
        ),
    );

    let catalog = StageCatalog::discover(dir.path());
    let entry = catalog.get(0).expect("one entry");

    assert_eq!(entry.stage_id, "001_first_wall");
    assert_eq!(entry.level, Some(1));
    assert_eq!(entry.difficulty.as_deref(), Some("tutorial"));
    assert_eq!(entry.grade_min, Some(7));
    assert_eq!(entry.grade_max, Some(8));
    assert_eq!(entry.topics, ["multiplication"]);
    assert_eq!(entry.locales, ["en-US", "fa"]);
    assert_eq!(entry.environment_preset.as_deref(), Some("meadow"));
    assert_eq!(entry.world_size, Some([16, 8, 16]));
    assert_eq!(entry.world_origin, Some([0, 0, 0]));
    assert_eq!(entry.objective_task_count, Some(2));
    assert_eq!(entry.reward_money, Some(100));
    assert_eq!(entry.reward_xp, Some(10));

    let en = StageLocale::from("en-US");
    assert_eq!(entry.translated_title(&en), Some("My first stage"));
    assert_eq!(entry.title(&en), "My first stage");
    assert_eq!(entry.translated_description(&en), Some("Build a wall."));
    assert_eq!(entry.translated_question(&en), Some("How many bricks?"));

    // Persian exists for the title; the description is missing in `fa` and
    // falls back to the `en-US` translation.
    let fa = StageLocale::from("fa");
    assert_eq!(
        entry.translated_title(&fa),
        Some("\u{0645}\u{0631}\u{062d}\u{0644}\u{0647}")
    );
    assert_eq!(entry.translated_description(&fa), Some("Build a wall."));
}

#[test]
fn missing_optional_metadata_does_not_fail() {
    let dir = TempDir::new("optional");
    let mut metadata = meta("bare", &["en-US"]);
    metadata.as_object_mut().unwrap().remove("level");
    let bare_stag = json!({
        "schema_version": 1,
        "stage": { "id": "bare" },
        "learning": {},
        "world": {},
        "construction": {},
        "objectives": {},
        "rewards": {},
    });
    dir.write(
        "bare.dat",
        &build_dat(
            0,
            &[
                SectionSpec::new(b"META", msgpack(&metadata)),
                SectionSpec::new(b"STAG", msgpack(&bare_stag)),
            ],
        ),
    );

    let catalog = StageCatalog::discover(dir.path());
    let entry = catalog.get(0).expect("one entry");
    assert_eq!(entry.stage_id, "bare");
    assert_eq!(entry.level, None);
    assert_eq!(entry.description_key, None);
    assert_eq!(entry.difficulty, None);
    assert_eq!(entry.environment_preset, None);
    assert_eq!(entry.world_size, None);
    assert_eq!(entry.world_origin, None);
    assert_eq!(entry.objective_task_count, None);
    assert_eq!(entry.reward_money, None);
    assert_eq!(entry.reward_xp, None);
    // The title key is always present; without a translation the raw key is
    // the last-resort fallback, but the UI can detect the miss.
    let locale = StageLocale::from("en-US");
    assert_eq!(entry.title(&locale), "stage.bare.title");
    assert_eq!(entry.translated_title(&locale), None);
}
