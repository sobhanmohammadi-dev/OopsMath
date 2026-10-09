//! MessagePack decoding helpers shared by the section decoders.

use std::fmt::Display;

use serde::de::DeserializeOwned;

use crate::stage::dat::error::DecodeError;
use crate::stage::dat::section_type::SectionType;
use crate::stage::error::StageLoadError;

/// Error for a payload that is not valid MessagePack.
pub(super) fn invalid_msgpack(section: SectionType, err: impl Display) -> StageLoadError {
    DecodeError::InvalidMsgpack {
        section,
        reason: err.to_string(),
    }
    .into()
}

/// Error for valid MessagePack whose structure is not what the section needs.
pub(super) fn invalid_structure(section: SectionType, reason: impl Into<String>) -> StageLoadError {
    DecodeError::InvalidMsgpackStructure {
        section,
        reason: reason.into(),
    }
    .into()
}

/// Decodes a MessagePack payload into `T`.
///
/// The payload goes through `serde_json::Value` first so that schema objects
/// kept as [`serde_json::Value`] and `#[serde(flatten)]` maps behave the same
/// as they would for JSON. `expectation` describes the layout in the error
/// message when the structure does not match.
pub(super) fn decode_structured<T: DeserializeOwned>(
    payload: &[u8],
    section: SectionType,
    expectation: &str,
) -> Result<T, StageLoadError> {
    let value: serde_json::Value =
        rmp_serde::from_slice(payload).map_err(|err| invalid_msgpack(section, err))?;
    serde_json::from_value(value)
        .map_err(|err| invalid_structure(section, format!("{expectation}: {err}")))
}
