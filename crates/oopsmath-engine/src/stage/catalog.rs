//! Runtime stage catalog: discover compiled `.dat` packages and summarise
//! them for the stage browser.
//!
//! Discovery is intentionally separate from UI construction and from gameplay
//! state transitions. [`StageCatalog::discover`] scans a directory once, loads
//! each package through the existing [`loader`], extracts a lightweight
//! [`StageCatalogEntry`], and then drops the full package so the browser never
//! holds decoded worlds or asset bytes in memory just to show a list.
//!
//! Invalid packages never abort discovery: each failure becomes a
//! [`CatalogDiagnostic`] carrying the offending path and the loader error,
//! and every valid package is still returned. Duplicate stage ids are reported
//! the same way instead of producing an ambiguous selection.

use std::cmp::Ordering;
use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::stage::loader;
use crate::stage::localization::{StageLocale, StageMessages};
use crate::stage::package::StagePackage;

/// Environment variable overriding the runtime stage directory.
pub const STAGES_DIR_ENV: &str = "OOPSMATH_STAGES_DIR";

/// Directory name used by the workspace layout at the repository root.
const STAGES_DIR_NAME: &str = "stages";

/// Historical / documented alternative directory name.
const LEVELS_DIR_NAME: &str = "levels";

/// One discoverable stage, summarised for display.
///
/// Only fields that are cheap to keep are stored; the parsed message tables
/// are small, but the decoded `WRLD` world and custom assets are dropped after
/// discovery. Localized strings are resolved on demand through [`StageLocale`]
/// so a locale change does not require rescanning the directory.
///
/// [`StageLocale`]: crate::stage::localization::StageLocale
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageCatalogEntry {
    /// `META.stage_id`.
    pub stage_id: String,
    /// Resolved path of the `.dat` package on disk.
    pub dat_path: PathBuf,
    /// Localization key for the title (e.g. `stage.001.title`).
    pub title_key: String,
    /// Localization key for the description, when declared.
    pub description_key: Option<String>,
    /// `META.level`, when present.
    pub level: Option<i64>,
    /// `META.difficulty`, when present.
    pub difficulty: Option<String>,
    /// `META.grade_min`.
    pub grade_min: Option<i64>,
    /// `META.grade_max`.
    pub grade_max: Option<i64>,
    /// `META.topics`.
    pub topics: Vec<String>,
    /// `META.locales`.
    pub locales: Vec<String>,
    /// Localization key for the learning question, when present.
    pub question_key: Option<String>,
    /// `stage.world.environment.preset`, when present.
    pub environment_preset: Option<String>,
    /// `stage.world.bounds.size`, when a 3-element number array.
    pub world_size: Option<[i64; 3]>,
    /// `stage.world.bounds.origin`, when a 3-element number array.
    pub world_origin: Option<[i64; 3]>,
    /// Number of `stage.objectives.tasks`.
    pub objective_task_count: Option<usize>,
    /// `stage.rewards.money`, when present.
    pub reward_money: Option<i64>,
    /// `stage.rewards.xp`, when present.
    pub reward_xp: Option<i64>,
    /// Parsed stage-local FTL messages, keyed by locale.
    pub messages: StageMessages,
}

impl StageCatalogEntry {
    /// Localized title, falling back to the raw key when untranslated.
    pub fn title(&self, locale: &StageLocale) -> &str {
        locale.resolve(&self.messages, &self.title_key)
    }

    /// Localized description, when the stage declares one.
    pub fn description(&self, locale: &StageLocale) -> Option<&str> {
        let key = self.description_key.as_deref()?;
        Some(locale.resolve(&self.messages, key))
    }

    /// Localized learning question, when the stage declares one.
    pub fn question(&self, locale: &StageLocale) -> Option<&str> {
        let key = self.question_key.as_deref()?;
        Some(locale.resolve(&self.messages, key))
    }
}

/// A stage whose file could not be used, or a structural problem with the
/// directory itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDiagnostic {
    /// The file (or directory) the diagnostic is about.
    pub path: PathBuf,
    /// What kind of problem was found.
    pub kind: CatalogDiagnosticKind,
    /// Human-readable detail (includes the loader error for rejected files).
    pub message: String,
}

/// Category of a [`CatalogDiagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogDiagnosticKind {
    /// The stage directory does not exist.
    DirectoryMissing,
    /// The stage directory exists but could not be read.
    DirectoryUnreadable,
    /// A `.dat` file failed DAT v1 validation or decoding.
    PackageRejected,
    /// Two packages declared the same stage id.
    DuplicateStageId,
}

impl fmt::Display for CatalogDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

/// The result of scanning a directory: valid entries plus diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageCatalog {
    /// Valid stages, ordered by level, then stage id, then path.
    pub entries: Vec<StageCatalogEntry>,
    /// Problems encountered while scanning; may be non-empty even when
    /// `entries` is not.
    pub diagnostics: Vec<CatalogDiagnostic>,
    /// The directory that was scanned.
    pub dir: PathBuf,
}

impl StageCatalog {
    /// Scans `dir` for compiled stage packages.
    ///
    /// The scan is a single pass over the direct children of `dir` (the
    /// runtime stages directory is flat). File enumeration order is not
    /// trusted: candidate paths are sorted before loading, and the resulting
    /// entries are sorted deterministically afterwards.
    pub fn discover(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref().to_path_buf();
        let mut catalog = Self {
            entries: Vec::new(),
            diagnostics: Vec::new(),
            dir: dir.clone(),
        };

        if !dir.exists() {
            catalog.diagnostics.push(CatalogDiagnostic {
                path: dir,
                kind: CatalogDiagnosticKind::DirectoryMissing,
                message: "stage directory does not exist".to_string(),
            });
            return catalog;
        }
        if !dir.is_dir() {
            catalog.diagnostics.push(CatalogDiagnostic {
                path: dir,
                kind: CatalogDiagnosticKind::DirectoryUnreadable,
                message: "stage path is not a directory".to_string(),
            });
            return catalog;
        }

        let mut candidates = match dat_files(&dir) {
            Ok(files) => files,
            Err(err) => {
                catalog.diagnostics.push(CatalogDiagnostic {
                    path: dir,
                    kind: CatalogDiagnosticKind::DirectoryUnreadable,
                    message: format!("cannot read stage directory: {err}"),
                });
                return catalog;
            }
        };
        candidates.sort();

        let mut by_id: std::collections::BTreeMap<String, PathBuf> =
            std::collections::BTreeMap::new();
        for path in candidates {
            match loader::load(&path) {
                Ok(package) => {
                    let stage_id = package.meta.stage_id.clone();
                    if let Some(first) = by_id.get(&stage_id) {
                        catalog.diagnostics.push(CatalogDiagnostic {
                            path: path.clone(),
                            kind: CatalogDiagnosticKind::DuplicateStageId,
                            message: format!(
                                "duplicate stage id '{}' already provided by {}",
                                stage_id,
                                first.display()
                            ),
                        });
                        continue;
                    }
                    by_id.insert(stage_id, path.clone());
                    catalog.entries.push(entry_from_package(path, &package));
                }
                Err(err) => catalog.diagnostics.push(CatalogDiagnostic {
                    path: path.clone(),
                    kind: CatalogDiagnosticKind::PackageRejected,
                    message: err.to_string(),
                }),
            }
        }

        catalog.entries.sort_by(entry_order);
        catalog
    }

    /// Scans the resolved runtime stage directory ([`default_stages_dir`]).
    pub fn discover_default() -> Self {
        Self::discover(default_stages_dir())
    }

    /// Whether any usable stage was found.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of usable stages.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The entry with the given stage id.
    pub fn entry(&self, stage_id: &str) -> Option<&StageCatalogEntry> {
        self.entries.iter().find(|entry| entry.stage_id == stage_id)
    }

    /// The entry at `index`.
    pub fn get(&self, index: usize) -> Option<&StageCatalogEntry> {
        self.entries.get(index)
    }
}

/// Deterministic ordering: by level (missing last), then stage id, then path.
fn entry_order(left: &StageCatalogEntry, right: &StageCatalogEntry) -> Ordering {
    let level = left
        .level
        .unwrap_or(i64::MAX)
        .cmp(&right.level.unwrap_or(i64::MAX));
    level
        .then_with(|| left.stage_id.cmp(&right.stage_id))
        .then_with(|| left.dat_path.cmp(&right.dat_path))
}

/// Resolves the runtime stage directory without trusting the current working
/// directory alone.
///
/// Order: the `OOPSMATH_STAGES_DIR` override, then `stages/` and `levels/`
/// relative to the workspace root (located from this crate's manifest
/// directory), then `stages/` and `levels/` relative to the current working
/// directory. If none exists, the workspace-root `stages/` path is returned so
/// the resulting diagnostic is meaningful.
pub fn default_stages_dir() -> PathBuf {
    if let Some(override_dir) = std::env::var_os(STAGES_DIR_ENV) {
        if !override_dir.is_empty() {
            return PathBuf::from(override_dir);
        }
    }
    for candidate in stage_dir_candidates() {
        if candidate.is_dir() {
            return candidate;
        }
    }
    workspace_root().join(STAGES_DIR_NAME)
}

/// Candidate directories, in priority order.
fn stage_dir_candidates() -> Vec<PathBuf> {
    let root = workspace_root();
    vec![
        root.join(STAGES_DIR_NAME),
        root.join(LEVELS_DIR_NAME),
        PathBuf::from(STAGES_DIR_NAME),
        PathBuf::from(LEVELS_DIR_NAME),
    ]
}

/// The workspace root, derived from this crate's manifest directory
/// (`<root>/crates/oopsmath-engine`). This is a development-time hint; the
/// `OOPSMATH_STAGES_DIR` override is the supported escape hatch for packaged
/// builds.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Direct `.dat` files in `dir`, sorted is left to the caller.
fn dat_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if has_dat_extension(&path) {
            files.push(path);
        }
    }
    Ok(files)
}

/// Whether `path` ends in `.dat`, case-insensitively.
fn has_dat_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dat"))
}

/// Builds a lightweight catalog entry from a fully loaded package.
fn entry_from_package(dat_path: PathBuf, package: &StagePackage) -> StageCatalogEntry {
    let meta = &package.meta;
    let stage = &package.stage;

    StageCatalogEntry {
        stage_id: meta.stage_id.clone(),
        dat_path,
        title_key: meta.title.clone(),
        description_key: meta.description.clone(),
        level: meta.level,
        difficulty: meta.difficulty.clone(),
        grade_min: Some(meta.grade_min),
        grade_max: Some(meta.grade_max),
        topics: meta.topics.clone(),
        locales: meta.locales.clone(),
        question_key: string_at(&stage.learning, &["question"]),
        environment_preset: string_at(&stage.world, &["environment", "preset"]),
        world_size: array3_at(&stage.world, &["bounds", "size"]),
        world_origin: array3_at(&stage.world, &["bounds", "origin"]),
        objective_task_count: array_len_at(&stage.objectives, &["tasks"]),
        reward_money: integer_at(&stage.rewards, &["money"]),
        reward_xp: integer_at(&stage.rewards, &["xp"]),
        messages: StageMessages::from_localizations(&package.localization),
    }
}

/// Follows a key path through a JSON value, returning `None` on any miss.
fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    Some(current)
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    value_at(value, path)?.as_str().map(str::to_string)
}

fn integer_at(value: &Value, path: &[&str]) -> Option<i64> {
    value_at(value, path)?.as_i64()
}

fn array_len_at(value: &Value, path: &[&str]) -> Option<usize> {
    value_at(value, path)?.as_array().map(Vec::len)
}

/// Reads a 3-element integer array (world bounds), tolerating missing or
/// malformed values by returning `None`.
fn array3_at(value: &Value, path: &[&str]) -> Option<[i64; 3]> {
    let array = value_at(value, path)?.as_array()?;
    if array.len() != 3 {
        return None;
    }
    Some([array[0].as_i64()?, array[1].as_i64()?, array[2].as_i64()?])
}
