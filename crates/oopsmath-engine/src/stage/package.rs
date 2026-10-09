//! The `StagePackage` produced by `stage::loader::load`.
//!
//! Every field comes straight from a checksum-verified DAT v1 section. Parts
//! of the stage schema the game has not modelled yet are kept as raw
//! [`MsgValue`]s instead of being dropped, so runtime systems can adopt them
//! later without a format change.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::stage::world::VoxelWorld;

/// Schema objects carried without a dedicated Rust binding (yet).
pub type MsgValue = serde_json::Value;

/// `META` section: package metadata reported for selection and hot-loading.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PackageMeta {
    /// Always "OOPSMDAT"; verified against the DAT magic.
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
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub difficulty: Option<String>,
    #[serde(default)]
    pub level: Option<i64>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// `STAG` section: the Stage Schema v1 definition, with the localization and
/// markdown/asset indirections already resolved by the compiler.
///
/// Fields are grouped by top-level schema key and kept as [`MsgValue`] so the
/// engine can start consuming gameplay data before every schema object has a
/// dedicated struct.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StageDefinition {
    /// Value of the `schema_version` key (always 1 for DAT v1).
    pub schema_version: u16,
    /// `stage` object (id, title, difficulty, level...).
    pub stage: MsgValue,
    /// `learning` object (question, curriculum, lesson, solution).
    pub learning: MsgValue,
    /// `world` object (bounds, environment, terrain...), without the voxel
    /// payload; voxel data lives in `StagePackage::world`.
    pub world: MsgValue,
    /// `construction` object.
    pub construction: MsgValue,
    /// `objectives` object.
    pub objectives: MsgValue,
    /// `rewards` object.
    pub rewards: MsgValue,
    #[serde(default)]
    pub story: Option<MsgValue>,
    #[serde(default)]
    pub player: Option<MsgValue>,
    #[serde(default)]
    pub camera: Option<MsgValue>,
    #[serde(default)]
    pub economy: Option<MsgValue>,
    #[serde(default)]
    pub physics: Option<MsgValue>,
    #[serde(default)]
    pub events: Option<MsgValue>,
    #[serde(default)]
    pub audio: Option<MsgValue>,
    #[serde(default)]
    pub localization: Option<MsgValue>,
    /// Any other top-level keys (e.g. future `extensions`), preserved verbatim.
    #[serde(flatten)]
    pub extra: BTreeMap<String, MsgValue>,
}

/// A localized FTL file keyed by locale (`fa`, `en-US`, ...).
///
/// The raw bytes are preserved exactly as compiled so Fluent loading sees
/// identical byte content across locales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Localization {
    pub locale: String,
    /// Raw FTL bytes from `LOCL`.
    pub raw: Vec<u8>,
}

/// A markdown or other text document from the `DOCS` section.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
    /// Byte offset of the asset inside `ASDT`.
    pub offset: u64,
    /// Byte length of the asset (equals `data.len()`).
    pub size: u64,
    /// SHA-256 declared by the compiler (lowercase hex); empty when the
    /// compiler declared none.
    pub sha256_hex: String,
    /// Exact asset bytes sliced from `ASDT`.
    pub data: Vec<u8>,
}

/// Everything the engine needs after a successful `stage::loader::load`.
#[derive(Debug, Clone, PartialEq)]
pub struct StagePackage {
    pub meta: PackageMeta,
    pub stage: StageDefinition,
    /// The voxel world; `None` when the stage ships no `WRLD` section.
    pub world: Option<VoxelWorld>,
    /// Stage-local localization, sorted by locale; empty when none shipped.
    pub localization: Vec<Localization>,
    /// Embedded documents in `DOCS` order.
    pub documents: Vec<Document>,
    /// Custom assets in `ASIX` order; empty when none shipped.
    pub custom_assets: Vec<CustomAsset>,
}

impl StagePackage {
    /// The document with the given id (e.g. `"lesson.md"`).
    pub fn document(&self, id: &str) -> Option<&Document> {
        self.documents.iter().find(|document| document.id == id)
    }

    /// The localization for the given locale (e.g. `"en-US"`).
    pub fn localization_for(&self, locale: &str) -> Option<&Localization> {
        self.localization
            .iter()
            .find(|localization| localization.locale == locale)
    }

    /// The custom asset with the given id.
    pub fn custom_asset(&self, id: &str) -> Option<&CustomAsset> {
        self.custom_assets.iter().find(|asset| asset.id == id)
    }
}
