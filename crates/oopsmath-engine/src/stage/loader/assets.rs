//! Custom assets: the `ASIX` index and the raw `ASDT` data container.
//!
//! `ASIX` lists every asset with an `(offset, size)` window into `ASDT`,
//! which is stored uncompressed by design. Each asset is sliced out of
//! `ASDT` and, when the index declares a SHA-256, verified against it.

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::stage::dat::directory::SectionDirectory;
use crate::stage::dat::payload::load_section_payload;
use crate::stage::dat::section_type::SectionType;
use crate::stage::error::StageLoadError;
use crate::stage::loader::decode::decode_structured;
use crate::stage::package::CustomAsset;

#[derive(Deserialize)]
struct AssetIndexPayload {
    assets: Vec<AssetIndexEntry>,
}

/// One entry of the `ASIX` index.
#[derive(Deserialize)]
struct AssetIndexEntry {
    id: String,
    #[serde(rename = "type")]
    asset_type: String,
    mime: String,
    offset: u64,
    size: u64,
    /// Optional metadata: the compiler may omit the checksum.
    #[serde(default)]
    sha256: String,
}

/// Decodes the `ASIX` payload and resolves every entry against `ASDT`.
pub(super) fn decode_custom_assets(
    index_payload: &[u8],
    file_bytes: &[u8],
    directory: &SectionDirectory,
) -> Result<Vec<CustomAsset>, StageLoadError> {
    let index: AssetIndexPayload = decode_structured(
        index_payload,
        SectionType::ASIX,
        "ASIX payload does not contain a valid 'assets' array",
    )?;

    let blob = if directory.contains(SectionType::ASDT) {
        load_section_payload(file_bytes, directory, SectionType::ASDT)?
    } else {
        Vec::new()
    };

    index
        .assets
        .into_iter()
        .map(|entry| resolve_asset(entry, &blob))
        .collect()
}

fn resolve_asset(entry: AssetIndexEntry, blob: &[u8]) -> Result<CustomAsset, StageLoadError> {
    let end = entry
        .offset
        .checked_add(entry.size)
        .ok_or_else(|| asset_error(SectionType::ASIX, format!("asset '{}' range overflows", entry.id)))?;
    if end > blob.len() as u64 {
        return Err(asset_error(
            SectionType::ASIX,
            format!(
                "asset '{}' range {}..{end} exceeds ASDT size {}",
                entry.id,
                entry.offset,
                blob.len()
            ),
        ));
    }
    // `end <= blob.len()`, so both bounds fit `usize`.
    let data = blob[entry.offset as usize..end as usize].to_vec();
    verify_sha256(&entry, &data)?;

    Ok(CustomAsset {
        id: entry.id,
        asset_type: entry.asset_type,
        mime: entry.mime,
        offset: entry.offset,
        size: entry.size,
        sha256_hex: entry.sha256,
        data,
    })
}

fn verify_sha256(entry: &AssetIndexEntry, data: &[u8]) -> Result<(), StageLoadError> {
    if entry.sha256.is_empty() {
        return Ok(());
    }
    let actual: String = Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !actual.eq_ignore_ascii_case(&entry.sha256) {
        return Err(asset_error(
            SectionType::ASDT,
            format!(
                "asset '{}' SHA-256 mismatch: expected {}, got {actual}",
                entry.id, entry.sha256
            ),
        ));
    }
    Ok(())
}

fn asset_error(section: SectionType, reason: String) -> StageLoadError {
    StageLoadError::Asset { section, reason }
}
