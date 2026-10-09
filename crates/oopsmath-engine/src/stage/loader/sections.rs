//! Decoders for the small MessagePack sections: META, STAG, LOCL and DOCS.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_bytes::ByteBuf;

use crate::stage::dat::header::{DAT_MAGIC, DatHeader};
use crate::stage::dat::section_type::SectionType;
use crate::stage::error::StageLoadError;
use crate::stage::loader::decode::{decode_structured, invalid_msgpack};
use crate::stage::package::{Document, Localization, PackageMeta, StageDefinition};

/// Decodes META and checks it against the DAT header it came with.
pub(super) fn decode_meta(
    payload: &[u8],
    header: &DatHeader,
) -> Result<PackageMeta, StageLoadError> {
    let meta: PackageMeta = decode_structured(
        payload,
        SectionType::META,
        "META payload does not match the DAT v1 metadata layout",
    )?;

    if meta.format.as_bytes() != DAT_MAGIC {
        return Err(StageLoadError::MetaFormatMismatch {
            declared: meta.format,
        });
    }
    if meta.format_version != header.format_version {
        return Err(StageLoadError::MetaHeaderMismatch {
            field: "format_version",
            meta: meta.format_version,
            header: header.format_version,
        });
    }
    if meta.schema_version != header.schema_version {
        return Err(StageLoadError::MetaHeaderMismatch {
            field: "schema_version",
            meta: meta.schema_version,
            header: header.schema_version,
        });
    }
    Ok(meta)
}

pub(super) fn decode_stage(payload: &[u8]) -> Result<StageDefinition, StageLoadError> {
    decode_structured(
        payload,
        SectionType::STAG,
        "STAG does not match Stage Schema v1 layout",
    )
}

/// Decodes LOCL: a map from locale to the raw FTL bytes.
///
/// The compiler emits FTL payloads as MessagePack binary values
/// (`use_bin_type=True`), so they are read with `serde_bytes` to keep the
/// exact bytes. The result is sorted by locale.
pub(super) fn decode_localization(payload: &[u8]) -> Result<Vec<Localization>, StageLoadError> {
    let by_locale: BTreeMap<String, ByteBuf> =
        rmp_serde::from_slice(payload).map_err(|err| invalid_msgpack(SectionType::LOCL, err))?;
    Ok(by_locale
        .into_iter()
        .map(|(locale, bytes)| Localization {
            locale,
            raw: bytes.into_vec(),
        })
        .collect())
}

#[derive(Deserialize)]
struct DocumentsPayload {
    documents: Vec<Document>,
}

pub(super) fn decode_documents(payload: &[u8]) -> Result<Vec<Document>, StageLoadError> {
    let payload: DocumentsPayload = decode_structured(
        payload,
        SectionType::DOCS,
        "DOCS payload does not contain a valid 'documents' array",
    )?;
    Ok(payload.documents)
}
