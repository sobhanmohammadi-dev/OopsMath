//! Error types of the stage loader.
//!
//! [`StageLoadError`] is what callers see. It wraps the container-level
//! errors of the DAT reader, which are re-exported here so that callers can
//! match on them without reaching into the internal `dat` module.

use thiserror::Error;

pub use crate::stage::dat::error::{DecodeError, HeaderError, SectionError, WorldError};
pub use crate::stage::dat::section_type::SectionType;

/// Errors from the high-level stage loader. Wraps the container and decoders
/// and adds the failures that only the loader can classify.
#[derive(Debug, Error)]
pub enum StageLoadError {
    #[error("Failed to read DAT file: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Header(#[from] HeaderError),

    #[error(transparent)]
    Section(#[from] SectionError),

    #[error(transparent)]
    Decode(#[from] DecodeError),

    #[error("Asset index issue in section {section}: {reason}")]
    Asset {
        section: SectionType,
        reason: String,
    },

    #[error("META section declares format {declared:?}, expected OOPSMDAT")]
    MetaFormatMismatch { declared: String },

    #[error("META {field} is {meta} but the DAT header declares {header}")]
    MetaHeaderMismatch {
        field: &'static str,
        meta: u16,
        header: u16,
    },
}
