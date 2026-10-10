//! Stage loading: compiled DAT v1 files -> typed [`StagePackage`].
//!
//! ```text
//! stage
//! |- dat/          low-level container reader (header, directory, payloads, WRLD)
//! |- loader/       section decoding and assembly of the package
//! |- catalog       runtime discovery of compiled `.dat` packages
//! |- localization  stage-local FTL parsing and locale resolution
//! |- package       the typed result (`StagePackage`, `PackageMeta`, ...)
//! |- world         the decoded voxel world
//! `- error         `StageLoadError` and the re-exported container errors
//! ```
//!
//! The [`catalog`] scans the runtime stages directory, loads each `.dat`
//! through [`load`] and keeps only a lightweight summary for the stage
//! browser. [`localization`] resolves the localization keys inside a package
//! against the app locale with a predictable fallback chain.
//!
//! Most callers only need [`load`] (from a path) or [`load_from_bytes`]:
//!
//! ```rust,ignore
//! let package = oopsmath_engine::stage::load("build/001_first_wall.dat")?;
//! println!("{}", package.meta.stage_id);
//! ```

pub mod catalog;
pub(crate) mod dat;
pub mod error;
pub mod loader;
pub mod localization;
pub mod package;
pub mod world;

#[cfg(test)]
mod tests;

pub use catalog::{StageCatalog, StageCatalogEntry};
pub use error::StageLoadError;
pub use loader::{load, load_from_bytes};
pub use localization::StageLocale;
pub use package::StagePackage;
pub use world::VoxelWorld;
