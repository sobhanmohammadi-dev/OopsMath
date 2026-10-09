//! The `StagePackage` produced by `stage::loader::load`.
//!
//! Every field comes straight from a checksum-verified DAT v1 section; fields
//! that the game has not yet implemented (asset behaviours, etc.) are
//! preserved verbatim instead of being dropped so runtime systems can adopt
//! them later without a format change.

use crate::stage::dat::world::VoxelWorld;
use serde::Deserialize;
use std::collections::BTreeMap;

/// `META` section: package metadata reported for selection/hot-loading.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PackageMeta {
    /// Always "OOPSMDAT"; verified against the DAT magic.
    #[serde(rename = "format")]
    pub format: String,
    pub format_version: u16,
    pub schema_version: u16,
    /// e.g. "oopsmath_stage/1.0.0"
    pub compiler: String,
    pub stage_id: String,
    /// Localization key for the stage title.
    pub title: String,
    pub grade_min: i64,
    pub grade_max: i64,
    pub topics: Vec<String>,
    pub locales: Vec<String>,
    /// Section types included in the package, starting with "META".
    pub sections: Vec<String>,
    /// Lowercase hex SHA-256 of every section payload in order.
    pub content_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// `STAG` section: the complete Stage Schema v1 definition, with the
/// localization and markdown/asset indirections resolved by the compiler.
/// Kept as `serde_json::Value`-free typed fields grouped by top-level schema
/// key so the engine can start consuming gameplay data before the full
/// schema structs exist.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StageDefinition {
    /// Value of the `schema_version` key (always 1 for DAT v1).
    pub schema_version: u16,
    /// `stage` object (id, title, difficulty, level...).
    pub stage: MsgValue,
    /// `learning` object (question, curriculum, lesson, solution).
    pub learning: MsgValue,
    /// `world` object (bounds, environment, terrain...), without the voxel
    /// payload; voxel data lives in `world_voxels`.
    pub world: MsgValue,
    /// `construction` object.
    pub construction: MsgValue,
    /// `objectives` object.
    pub objectives: MsgValue,
    /// `rewards` object.
    pub rewards: MsgValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story: Option<MsgValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<MsgValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<MsgValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub economy: Option<MsgValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physics: Option<MsgValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<MsgValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<MsgValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub localization: Option<MsgValue>,
    /// Any other top-level keys (e.g. future `extensions`), preserved verbatim.
    #[serde(flatten)]
    pub extra: BTreeMap<String, MsgValue>,
}

/// MessagePack value placeholder: forward-compatible alias so the package can
/// carry arbitrary schema objects without hand-writing every binding.
pub type MsgValue = serde_json::Value;

/// A localized FTL file keyed by locale (`fa`, `en-US`, ...).
/// The raw bytes are preserved exactly as compiled so Fluent loading in the
/// engine sees identical byte content across locales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Localization {
    pub locale: String,
    /// Raw FTL bytes from `LOCL` (msgpack binary values preserved).
    pub raw: Vec<u8>,
}

/// A markdown or other text document from the `DOCS` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub id: String,
    pub mime: String,
    pub text: String,
}

/// Custom asset metadata from `ASIX` plus the exact GLB bytes from `ASDT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomAsset {
    pub id: String,
    pub asset_type: String,
    pub mime: String,
    pub offset: u64,
    pub size: u64,
    /// Lowercase hex SHA-256 declared by the compiler.
    pub sha256_hex: String,
    /// Exact GLB bytes sliced from `ASDT`.
    pub data: Vec<u8>,
}

/// Everything the engine needs after a successful `stage::loader::load`.
#[derive(Debug, Clone)]
pub struct StagePackage {
    pub meta: PackageMeta,
    pub stage: StageDefinition,
    pub world: Option<VoxelWorld>,
    /// Stage-local localization; empty when the stage ships none.
    pub localization: Vec<Localization>,
    /// Embedded documents in `DOCS` order.
    pub documents: Vec<Document>,
    /// Custom assets, when `ASIX`/`ASDT` exist; empty otherwise.
    pub custom_assets: Vec<CustomAsset>,
}
