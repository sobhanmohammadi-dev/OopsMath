//! Public stage loading entry points.
//!
//! The loader verifies the DAT container (header CRC, directory, per-section
//! checksums), decompresses and decodes every supported section, validates
//! the WRLD world chunks and any ASIX/ASDT custom assets, and only then
//! returns a [`StagePackage`]. No partial package is ever returned on failure.
//!
//! * [`decode`]   - MessagePack helpers shared by the section decoders.
//! * [`sections`] - META, STAG, LOCL and DOCS decoders.
//! * [`assets`]   - ASIX/ASDT custom asset decoding and verification.

mod assets;
mod decode;
mod sections;

use std::path::Path;

use crate::stage::dat::error::{DecodeError, SectionError};
use crate::stage::dat::directory::SectionDirectory;
use crate::stage::dat::header::DatHeader;
use crate::stage::dat::payload::load_section_payload;
use crate::stage::dat::section_type::SectionType;
use crate::stage::dat::world::parse_world;
use crate::stage::error::StageLoadError;
use crate::stage::package::StagePackage;

/// Loads a compiled stage from a DAT v1 file.
///
/// ```rust,ignore
/// let package = oopsmath_engine::stage::load("build/001_first_wall.dat")?;
/// ```
pub fn load(path: impl AsRef<Path>) -> Result<StagePackage, StageLoadError> {
    let bytes = std::fs::read(path.as_ref())?;
    load_from_bytes(&bytes)
}

/// Loads a compiled stage from an in-memory DAT v1 blob.
///
/// Useful for tests and network loading; [`load`] is the file-based call.
pub fn load_from_bytes(bytes: &[u8]) -> Result<StagePackage, StageLoadError> {
    let header = DatHeader::parse(bytes)?;
    let directory = SectionDirectory::parse(bytes, &header)?;

    let required = |section| load_section_payload(bytes, &directory, section);
    let meta = sections::decode_meta(&required(SectionType::META)?, &header)?;
    let stage = sections::decode_stage(&required(SectionType::STAG)?)?;

    let world = optional_payload(bytes, &directory, SectionType::WRLD)?
        .map(|payload| parse_world(&payload).map_err(DecodeError::InvalidWorld))
        .transpose()?;

    let localization = optional_payload(bytes, &directory, SectionType::LOCL)?
        .map(|payload| sections::decode_localization(&payload))
        .transpose()?
        .unwrap_or_default();

    let documents = optional_payload(bytes, &directory, SectionType::DOCS)?
        .map(|payload| sections::decode_documents(&payload))
        .transpose()?
        .unwrap_or_default();

    let custom_assets = optional_payload(bytes, &directory, SectionType::ASIX)?
        .map(|payload| assets::decode_custom_assets(&payload, bytes, &directory))
        .transpose()?
        .unwrap_or_default();

    Ok(StagePackage {
        meta,
        stage,
        world,
        localization,
        documents,
        custom_assets,
    })
}

/// Loads the payload of an optional section, or `None` when it is absent.
fn optional_payload(
    bytes: &[u8],
    directory: &SectionDirectory,
    section: SectionType,
) -> Result<Option<Vec<u8>>, SectionError> {
    if directory.contains(section) {
        load_section_payload(bytes, directory, section).map(Some)
    } else {
        Ok(None)
    }
}
