//! Immutable description of a registered block *type*.
//!
//! Individual blocks in the world are [`super::placement::PlacedBlock`]s that
//! refer to a definition by [`BlockId`]; they never copy a definition.
//!
//! Units: world sizes are metres; prices are whole in-game coins (the same
//! unit as the stage `rewards.money` / `economy.budget` fields).

use std::fmt;

use bevy::math::{UVec3, Vec3};

use super::error::BlockError;
use super::grid::{DEFAULT_CELL_SIZE, GridRotation};

/// Largest footprint, per axis, in cells.
pub const MAX_FOOTPRINT_CELLS: u32 = 64;
/// Sanity cap for a unit price (coins).
pub const MAX_UNIT_PRICE: u32 = 1_000_000;
/// Tolerance when comparing world size against footprint size.
const SIZE_EPSILON: f32 = 1e-3;

/// Stable identity of a block type, e.g. `block.wood_log`.
///
/// Rules: at least two dot-separated segments; each segment is lowercase ASCII
/// `[a-z][a-z0-9_]*`. Ids are persisted in stages, so never rename one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(String);

impl BlockId {
    /// Wraps a string without checking it; validation happens at registration.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), BlockError> {
        let fail = |reason| BlockError::InvalidId {
            id: self.0.clone(),
            reason,
        };
        if self.0.is_empty() {
            return Err(fail("id is empty"));
        }
        let mut segments = 0;
        for segment in self.0.split('.') {
            if !is_valid_segment(segment) {
                return Err(fail(
                    "segments must match [a-z][a-z0-9_]* and be separated by single dots",
                ));
            }
            segments += 1;
        }
        if segments < 2 {
            return Err(fail("id needs a namespace, e.g. 'block.wood_log'"));
        }
        Ok(())
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub(crate) fn is_valid_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// ---------------------------------------------------------------- model

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelFormat {
    Glb,
    Gltf,
}

impl ModelFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Glb => "glb",
            Self::Gltf => "gltf",
        }
    }
}

/// Reference to the model asset of a block.
///
/// `path` is relative to the application's asset root (the repository-level
/// `assets/` folder), e.g. `"log.glb"`. Textures are expected to be embedded in
/// the GLB; a `.gltf` with external files must keep them next to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockModel {
    pub path: String,
    pub format: ModelFormat,
    /// glTF scene index to instantiate.
    pub scene_index: usize,
}

impl BlockModel {
    pub fn glb(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            format: ModelFormat::Glb,
            scene_index: 0,
        }
    }

    pub fn with_scene_index(mut self, scene_index: usize) -> Self {
        self.scene_index = scene_index;
        self
    }

    fn validate(&self) -> Result<(), String> {
        let p = &self.path;
        if p.is_empty() {
            return Err("model path is empty".into());
        }
        if p.starts_with('/') || p.contains('\\') || p.contains(':') {
            return Err(format!(
                "model path '{p}' must be relative to the asset root and use '/'"
            ));
        }
        if p.split('/').any(|s| s.is_empty() || s == "..") {
            return Err(format!("model path '{p}' has an empty or '..' segment"));
        }
        let ext = self.format.extension();
        if !p.to_ascii_lowercase().ends_with(&format!(".{ext}")) {
            return Err(format!("model path '{p}' does not end with .{ext}"));
        }
        Ok(())
    }
}

// ----------------------------------------------------------- dimensions

/// Logical footprint (cells) and physical size (metres) of a block.
///
/// These are independent of the GLB's own scale; see [`BlockVisualConfig`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockDimensions {
    pub footprint_cells: UVec3,
    /// Real-world size in metres. Must fit inside the footprint.
    pub world_size: Vec3,
}

impl BlockDimensions {
    pub fn new(footprint_cells: UVec3, world_size: Vec3) -> Self {
        Self {
            footprint_cells,
            world_size,
        }
    }
}

// ----------------------------------------------------------------- grid

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlacementMode {
    /// Placed on integer grid cells (the only mode implemented).
    #[default]
    Grid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AllowedRotations {
    /// Only [`GridRotation::Deg0`].
    None,
    /// All four quarter turns about Y.
    #[default]
    QuarterTurns,
}

impl AllowedRotations {
    pub fn allows(self, rotation: GridRotation) -> bool {
        match self {
            Self::None => rotation == GridRotation::Deg0,
            Self::QuarterTurns => true,
        }
    }

    /// Whether the player may rotate this block at all.
    pub fn is_rotatable(self) -> bool {
        self != Self::None
    }

    /// One quarter turn from `current`; a non-rotatable block stays at `Deg0`.
    pub fn step(self, current: GridRotation) -> GridRotation {
        match self {
            Self::None => GridRotation::Deg0,
            Self::QuarterTurns => current.next(),
        }
    }

    /// The orientation actually used for a requested one: the request if this
    /// block allows it, otherwise `Deg0`.
    pub fn effective(self, requested: GridRotation) -> GridRotation {
        if self.allows(requested) {
            requested
        } else {
            GridRotation::Deg0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridConfig {
    pub placement_mode: PlacementMode,
    pub rotations: AllowedRotations,
    /// World-space offset added to the footprint centre (unrotated).
    pub placement_offset: Vec3,
}

impl GridConfig {
    /// Grid placement, all quarter turns, no offset.
    pub fn default_block() -> Self {
        Self {
            placement_mode: PlacementMode::Grid,
            rotations: AllowedRotations::QuarterTurns,
            placement_offset: Vec3::ZERO,
        }
    }

    pub fn with_rotations(mut self, rotations: AllowedRotations) -> Self {
        self.rotations = rotations;
        self
    }
}

// ------------------------------------------------------------- economy

/// Global default price. A stage may override it; see `stage_rules`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockEconomyConfig {
    /// Price of one block in whole coins.
    pub default_unit_price: u32,
}

impl BlockEconomyConfig {
    pub const fn with_default_price(default_unit_price: u32) -> Self {
        Self { default_unit_price }
    }
}

// -------------------------------------------------------------- visuals

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaterialPolicy {
    /// Keep the materials and textures exactly as authored in the model
    /// (the only policy implemented).
    #[default]
    PreserveOriginal,
}

/// How the model is fitted to the block's logical box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockVisualConfig {
    /// Scale applied to the model so its authored size matches `world_size`.
    pub scale: Vec3,
    /// Offset (metres, block-local, rotates with the block) from the footprint
    /// centre to the model's pivot. Zero for models authored centred.
    pub model_offset: Vec3,
    pub material_policy: MaterialPolicy,
}

impl Default for BlockVisualConfig {
    fn default() -> Self {
        Self {
            scale: Vec3::ONE,
            model_offset: Vec3::ZERO,
            material_policy: MaterialPolicy::PreserveOriginal,
        }
    }
}

impl BlockVisualConfig {
    pub fn with_uniform_scale(mut self, scale: f32) -> Self {
        self.scale = Vec3::splat(scale);
        self
    }
}

// -------------------------------------------------------------- physics

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollisionPolicy {
    /// No collision.
    None,
    /// Collides as the footprint box. Declarative only: no collision system
    /// consumes it yet.
    #[default]
    FootprintBox,
}

/// Gameplay/structural properties, deliberately separate from geometry.
/// Nothing simulates these yet; they are data for future systems.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockPhysicsConfig {
    /// Mass of one block in kilograms (informational for now).
    pub mass_kg: f32,
    pub collision: CollisionPolicy,
    /// Capability tags, each `[a-z][a-z0-9_]*`, e.g. `wood`.
    pub tags: Vec<String>,
}

impl Default for BlockPhysicsConfig {
    fn default() -> Self {
        Self {
            mass_kg: 1.0,
            collision: CollisionPolicy::FootprintBox,
            tags: Vec::new(),
        }
    }
}

// ----------------------------------------------------------- definition

#[derive(Clone, Debug, PartialEq)]
pub struct BlockDefinition {
    pub id: BlockId,
    pub display_name: String,
    pub description: Option<String>,
    pub model: BlockModel,
    pub dimensions: BlockDimensions,
    pub grid: GridConfig,
    pub economy: BlockEconomyConfig,
    pub visuals: BlockVisualConfig,
    pub physics: BlockPhysicsConfig,
}

impl BlockDefinition {
    /// Required parts are explicit; grid, visuals and physics start from their
    /// documented defaults and can be changed with the `with_*` methods.
    pub fn new(
        id: BlockId,
        display_name: impl Into<String>,
        model: BlockModel,
        dimensions: BlockDimensions,
        economy: BlockEconomyConfig,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            description: None,
            model,
            dimensions,
            grid: GridConfig::default_block(),
            economy,
            visuals: BlockVisualConfig::default(),
            physics: BlockPhysicsConfig::default(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_grid(mut self, grid: GridConfig) -> Self {
        self.grid = grid;
        self
    }

    pub fn with_visuals(mut self, visuals: BlockVisualConfig) -> Self {
        self.visuals = visuals;
        self
    }

    pub fn with_physics(mut self, physics: BlockPhysicsConfig) -> Self {
        self.physics = physics;
        self
    }

    pub fn with_economy(mut self, economy: BlockEconomyConfig) -> Self {
        self.economy = economy;
        self
    }

    /// A copy under a new id, to build a variant without restating every field.
    /// Chain `with_*` calls to change what differs.
    pub fn variant(&self, id: BlockId, display_name: impl Into<String>) -> Self {
        let mut copy = self.clone();
        copy.id = id;
        copy.display_name = display_name.into();
        copy
    }

    /// Checks every invariant; called by `BlockRegistry::register`.
    pub fn validate(&self) -> Result<(), BlockError> {
        self.id.validate()?;
        let bad = |reason: String| BlockError::InvalidDefinition {
            id: self.id.clone(),
            reason,
        };

        if self.display_name.trim().is_empty() {
            return Err(bad("display name is empty".into()));
        }
        self.model.validate().map_err(bad)?;

        let f = self.dimensions.footprint_cells;
        if f.min_element() == 0 || f.max_element() > MAX_FOOTPRINT_CELLS {
            return Err(bad(format!(
                "footprint {f} must be 1..={MAX_FOOTPRINT_CELLS} cells per axis"
            )));
        }
        let w = self.dimensions.world_size;
        if !w.is_finite() || w.min_element() <= 0.0 {
            return Err(bad(format!("world size {w} must be finite and positive")));
        }
        let max = f.as_vec3() * DEFAULT_CELL_SIZE + Vec3::splat(SIZE_EPSILON);
        if w.cmpgt(max).any() {
            return Err(bad(format!(
                "world size {w} exceeds the footprint {f} at the default cell size"
            )));
        }

        let v = &self.visuals;
        if !v.scale.is_finite() || v.scale.min_element() <= 0.0 {
            return Err(bad("visual scale must be finite and positive".into()));
        }
        if !v.model_offset.is_finite() || !self.grid.placement_offset.is_finite() {
            return Err(bad("offsets must be finite".into()));
        }

        if self.economy.default_unit_price > MAX_UNIT_PRICE {
            return Err(bad(format!(
                "default price {} exceeds the maximum {MAX_UNIT_PRICE}",
                self.economy.default_unit_price
            )));
        }

        if !self.physics.mass_kg.is_finite() || self.physics.mass_kg < 0.0 {
            return Err(bad("mass must be finite and non-negative".into()));
        }
        if let Some(tag) = self.physics.tags.iter().find(|t| !is_valid_segment(t)) {
            return Err(bad(format!("tag '{tag}' must match [a-z][a-z0-9_]*")));
        }
        Ok(())
    }
}
