//! Typed errors for the Blocks subsystem.
//!
//! Registry/definition/placement problems ([`BlockError`]) are deliberately
//! kept apart from asynchronous asset problems ([`BlockAssetError`]): the first
//! are programmer or data errors detected synchronously, the second can happen
//! at runtime when a file is missing or corrupt.

use bevy::math::IVec3;
use thiserror::Error;

use super::definition::BlockId;
use super::grid::GridRotation;

/// Invalid ids, definitions, grids, placements and stage rules.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum BlockError {
    #[error("invalid block id '{id}': {reason}")]
    InvalidId { id: String, reason: &'static str },

    #[error("block id '{0}' is already registered")]
    DuplicateId(BlockId),

    #[error("invalid definition for block '{id}': {reason}")]
    InvalidDefinition { id: BlockId, reason: String },

    #[error("block '{0}' is not registered")]
    NotRegistered(BlockId),

    #[error("block '{id}' does not allow rotation {rotation:?}")]
    RotationNotAllowed { id: BlockId, rotation: GridRotation },

    #[error("block '{id}' placed at cell {cell} would leave the grid bounds")]
    OutOfBounds { id: BlockId, cell: IVec3 },

    #[error("invalid grid configuration: {0}")]
    InvalidGrid(String),

    #[error("invalid stage block rule: {0}")]
    InvalidStageRule(String),
}

/// Model loading problems, reported by the asset layer.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum BlockAssetError {
    #[error("no model asset has been requested for block '{0}'")]
    NotRequested(BlockId),

    #[error("model '{path}' for block '{id}' is still loading")]
    Pending { id: BlockId, path: String },

    #[error("failed to load model '{path}' for block '{id}': {reason}")]
    LoadFailed {
        id: BlockId,
        path: String,
        reason: String,
    },
}

/// Failure of [`super::rendering::spawn_block`].
#[derive(Debug, Clone, PartialEq, Error)]
pub enum BlockSpawnError {
    #[error(transparent)]
    Block(#[from] BlockError),
    #[error(transparent)]
    Asset(#[from] BlockAssetError),
}
