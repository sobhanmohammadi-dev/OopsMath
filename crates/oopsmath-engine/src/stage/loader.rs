//! Public stage loading entry point.
//!
//! Usage:
//!
//! ```rust,ignore
//! let package = oopsmath_engine::stage::loader::load("build/001.dat")?;
//! println!("{}", package.meta.stage_id);
//! ```
//!
//! The loader verifies the DAT container (header CRC, directory, per-section
//! checksums), decompresses and decodes every supported section, validates
//! the WRLD world chunks and any ASIX/ASDT custom assets, and only then
//! returns a `StagePackage`. No partial package is ever returned on failure.

use std::path::Path;

use thiserror::Error;

use crate::stage::dat::error::{DecodeError, HeaderError, SectionError, SectionType};
use crate::stage::dat::header::DatHeader;
use crate::stage::dat::reader::load_section_payload;
use crate::stage::dat::section::SectionDirectory;
use crate::stage::dat::world::{VoxelWorld, parse_world};
use crate::stage::package::{
    CustomAsset, Document, Localization, PackageMeta, StageDefinition, StagePackage,
};

/// Errors from the high-level stage loader. Wraps the container and decoders
/// and adds the section-level decode failures that the lower layers cannot
/// classify alone.
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
}

/// Loads a compiled stage from a DAT v1 file.
///
/// ```rust,ignore
/// let package = oopsmath_engine::stage::loader::load("build/001.dat")?;
/// ```
pub fn load(path: impl AsRef<Path>) -> Result<StagePackage, StageLoadError> {
    let bytes = std::fs::read(path.as_ref())?;
    load_from_bytes(&bytes)
}

/// Loads a compiled stage from an in-memory DAT v1 blob.
///
/// Useful for tests and network loading; `load(path)` is the production call.
pub fn load_from_bytes(bytes: &[u8]) -> Result<StagePackage, StageLoadError> {
    let header = DatHeader::parse(bytes)?;
    let directory = SectionDirectory::parse(bytes, &header)?;

    let meta = {
        let payload = load_section_payload(bytes, &directory, SectionType::META)?;
        decode_json(&payload, SectionType::META, "META")?.ok_or(StageLoadError::Section(
            SectionError::MissingRequiredSection(SectionType::META),
        ))?
    };
    verify_meta(&meta, &header)?;

    let stag_payload = load_section_payload(bytes, &directory, SectionType::STAG)?;
    let stage = build_stage_definition(&stag_payload, SectionType::STAG)?;

    let world: Option<VoxelWorld> = match directory.contains(SectionType::WRLD) {
        true => {
            let payload = load_section_payload(bytes, &directory, SectionType::WRLD)?;
            Some(parse_world(&payload).map_err(DecodeError::from)?)
        }
        false => None,
    };

    let localization: Vec<Localization> = if directory.contains(SectionType::LOCL) {
        let payload = load_section_payload(bytes, &directory, SectionType::LOCL)?;
        build_localization(&payload, SectionType::LOCL)?
    } else {
        Vec::new()
    };

    let documents: Vec<Document> = if directory.contains(SectionType::DOCS) {
        let payload = load_section_payload(bytes, &directory, SectionType::DOCS)?;
        build_documents(&payload, SectionType::DOCS)?
    } else {
        Vec::new()
    };

    let asset_guard = if directory.contains(SectionType::ASIX) {
        let payload = load_section_payload(bytes, &directory, SectionType::ASIX)?;
        Some(build_asset_index(
            &payload,
            bytes,
            &directory,
            SectionType::ASIX,
        )?)
    } else {
        None
    };

    let mut custom_assets: Vec<CustomAsset> = Vec::new();
    if let Some(entries) = asset_guard {
        for (asset, data) in entries {
            verify_asset_checksum(&asset, &data)?;
            custom_assets.push(CustomAsset {
                id: asset.id,
                asset_type: asset.asset_type,
                mime: asset.mime,
                offset: asset.offset,
                size: asset.size,
                sha256_hex: asset.sha256_hex,
                data,
            });
        }
    }

    Ok(StagePackage {
        meta: translate_meta(meta)?,
        stage,
        world,
        localization,
        documents,
        custom_assets,
    })
}

// ---------------------------------------------------------------------------
// MessagePack -> typed mapping helpers
// ---------------------------------------------------------------------------

fn decode_msgpack_to_value(
    payload: &[u8],
    section: SectionType,
) -> Result<serde_json::Value, StageLoadError> {
    let value: serde_json::Value = rmp_serde::from_slice(payload).map_err(|err| {
        StageLoadError::Decode(DecodeError::InvalidMsgpack {
            section,
            reason: err.to_string(),
        })
    })?;
    Ok(value)
}

fn decode_json(
    payload: &[u8],
    section: SectionType,
    desc: &str,
) -> Result<Option<serde_json::Value>, StageLoadError> {
    let _ = desc;
    Ok(Some(decode_msgpack_to_value(payload, section)?))
}

fn verify_meta(meta: &serde_json::Value, header: &DatHeader) -> Result<(), StageLoadError> {
    let format = meta
        .get("format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if format != "OOPSMDAT" {
        return Err(StageLoadError::MetaFormatMismatch {
            declared: format.to_string(),
        });
    }
    let _ = header;
    Ok(())
}

fn translate_meta(value: serde_json::Value) -> Result<PackageMeta, StageLoadError> {
    serde_json::from_value(value).map_err(|err| {
        StageLoadError::Decode(DecodeError::InvalidMsgpackStructure {
            section: SectionType::META,
            reason: format!("META payload does not match the DAT v1 metadata layout: {err}"),
        })
    })
}

fn build_stage_definition(
    payload: &[u8],
    section: SectionType,
) -> Result<StageDefinition, StageLoadError> {
    let value: serde_json::Value = rmp_serde::from_slice(payload).map_err(|err| {
        StageLoadError::Decode(DecodeError::InvalidMsgpack {
            section,
            reason: err.to_string(),
        })
    })?;
    serde_json::from_value(value).map_err(|err| {
        StageLoadError::Decode(DecodeError::InvalidMsgpackStructure {
            section,
            reason: format!("STAG does not match Stage Schema v1 layout: {err}"),
        })
    })
}

fn build_localization(
    payload: &[u8],
    section: SectionType,
) -> Result<Vec<Localization>, StageLoadError> {
    // The compiler emits FTL payloads as MessagePack binary values
    // (usage of use_bin_type=True), so read them with serde_bytes to keep
    // exact bytes, then map each entry to a typed Localization.
    let raw_map =
        rmp_serde::from_slice::<std::collections::BTreeMap<String, serde_bytes::ByteBuf>>(payload)
            .map_err(|err| {
                StageLoadError::Decode(DecodeError::InvalidMsgpack {
                    section,
                    reason: err.to_string(),
                })
            })?;
    Ok(raw_map
        .into_iter()
        .map(|(locale, bytes)| Localization {
            locale,
            raw: bytes.to_vec(),
        })
        .collect())
}
fn build_documents(payload: &[u8], section: SectionType) -> Result<Vec<Document>, StageLoadError> {
    let value = decode_msgpack_to_value(payload, section)?;
    let documents = value
        .get("documents")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            StageLoadError::Decode(DecodeError::InvalidMsgpackStructure {
                section,
                reason: "DOCS payload is missing the 'documents' array".to_string(),
            })
        })?;
    let mut docs = Vec::with_capacity(documents.len());
    for entry in documents {
        docs.push(Document {
            id: entry
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            mime: entry
                .get("mime")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            text: entry
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    }
    Ok(docs)
}

/// A raw asset index entry as loaded from ASIX.
#[derive(Debug, Clone)]
struct AssetIndex {
    id: String,
    asset_type: String,
    mime: String,
    offset: u64,
    size: u64,
    sha256_hex: String,
}

fn build_asset_index(
    payload: &[u8],
    data: &[u8],
    directory: &SectionDirectory,
    section: SectionType,
) -> Result<Vec<(AssetIndex, Vec<u8>)>, StageLoadError> {
    let value = decode_msgpack_to_value(payload, section)?;
    let assets = value
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            StageLoadError::Decode(DecodeError::InvalidMsgpackStructure {
                section,
                reason: "ASIX payload is missing the 'assets' array".to_string(),
            })
        })?;

    // ASDT is the raw payload container (not compressed by design); when it
    // exists the index addresses its contents with (offset, size) pairs.
    let asdt_payload = if directory.contains(SectionType::ASDT) {
        load_section_payload(data, directory, SectionType::ASDT)?
    } else {
        Vec::new()
    };

    let mut out = Vec::with_capacity(assets.len());
    for entry in assets {
        let id = entry
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let asset_type = entry
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mime = entry
            .get("mime")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let offset = entry
            .get("offset")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let size = entry
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let sha256_hex = entry
            .get("sha256")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();

        let end = offset
            .checked_add(size)
            .ok_or_else(|| StageLoadError::Asset {
                section,
                reason: format!("asset '{id}' range overflows"),
            })?;
        if end > asdt_payload.len() as u64 {
            return Err(StageLoadError::Asset {
                section,
                reason: format!(
                    "asset '{id}' range {offset}..{end} exceeds ASDT size {}",
                    asdt_payload.len()
                ),
            });
        }
        out.push((
            AssetIndex {
                id,
                asset_type,
                mime,
                offset,
                size,
                sha256_hex,
            },
            asdt_payload[offset as usize..end as usize].to_vec(),
        ));
    }
    Ok(out)
}

fn verify_asset_checksum(asset: &AssetIndex, data: &[u8]) -> Result<(), StageLoadError> {
    if asset.sha256_hex.is_empty() {
        return Ok(()); // SHA-256 is optional metadata in ASIX entries
    }
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02X}")).collect();
    if !hex.eq_ignore_ascii_case(&asset.sha256_hex) {
        return Err(StageLoadError::Asset {
            section: SectionType::ASDT,
            reason: format!(
                "asset '{}' SHA-256 mismatch: expected {}, got {}",
                asset.id, asset.sha256_hex, hex
            ),
        });
    }
    Ok(())
}
