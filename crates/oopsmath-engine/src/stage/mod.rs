//! Stage loading: compiled DAT v1 files -> typed [`StagePackage`].
//!
//! ```text
//! stage
//! |- dat/      low-level container reader (header, directory, payloads, WRLD)
//! |- loader/   section decoding and assembly of the package
//! |- package   the typed result (`StagePackage`, `PackageMeta`, ...)
//! |- world     the decoded voxel world
//! `- error     `StageLoadError` and the re-exported container errors
//! ```
//!
//! Most callers only need [`load`] (from a path) or [`load_from_bytes`]:
//!
//! ```rust,ignore
//! let package = oopsmath_engine::stage::load("build/001_first_wall.dat")?;
//! println!("{}", package.meta.stage_id);
//! ```

pub(crate) mod dat;
pub mod error;
pub mod loader;
pub mod package;
pub mod world;

#[cfg(test)]
mod tests;

pub use error::StageLoadError;
pub use loader::{load, load_from_bytes};
pub use package::StagePackage;
pub use world::VoxelWorld;
