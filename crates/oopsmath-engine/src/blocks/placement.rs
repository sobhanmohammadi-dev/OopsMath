//! Placed block instances: a registered [`BlockId`] plus per-instance grid
//! state. The immutable definition is always resolved through the registry.

use bevy::prelude::*;

use super::definition::{BlockDefinition, BlockId};
use super::error::BlockError;
use super::grid::{GridCoord, GridRotation, GridSpec, footprint_box, rotated_cells};
use super::registry::BlockRegistry;

/// Runtime instance of a block. Placing or previewing one never charges the
/// player; purchasing and inventory live outside this module.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PlacedBlock {
    pub block: BlockId,
    /// Minimum corner cell of the (rotated) footprint.
    pub cell: GridCoord,
    pub rotation: GridRotation,
}

/// Checks that `block` exists, allows `rotation`, and fits the grid bounds.
pub fn validate_placement<'r>(
    registry: &'r BlockRegistry,
    grid: &GridSpec,
    block: &BlockId,
    cell: GridCoord,
    rotation: GridRotation,
) -> Result<&'r BlockDefinition, BlockError> {
    let def = registry.require(block)?;
    if !def.grid.rotations.allows(rotation) {
        return Err(BlockError::RotationNotAllowed {
            id: block.clone(),
            rotation,
        });
    }
    if let Some(bounds) = grid.bounds {
        let (min, size) = footprint_box(cell, def.dimensions.footprint_cells, rotation);
        if !bounds.contains_box(min, size) {
            return Err(BlockError::OutOfBounds {
                id: block.clone(),
                cell: cell.0,
            });
        }
    }
    Ok(def)
}

impl PlacedBlock {
    /// Validated constructor.
    pub fn new(
        registry: &BlockRegistry,
        grid: &GridSpec,
        block: BlockId,
        cell: GridCoord,
        rotation: GridRotation,
    ) -> Result<Self, BlockError> {
        validate_placement(registry, grid, &block, cell, rotation)?;
        Ok(Self {
            block,
            cell,
            rotation,
        })
    }

    /// Cells covered by this block, taking rotation into account.
    pub fn occupied_cells(&self, def: &BlockDefinition) -> impl Iterator<Item = GridCoord> {
        rotated_cells(self.cell, def.dimensions.footprint_cells, self.rotation)
    }

    /// World transform of the model: footprint centre + offsets, yaw, and the
    /// definition's visual scale.
    pub fn transform(&self, def: &BlockDefinition, grid: &GridSpec) -> Transform {
        let rotation = self.rotation.to_quat();
        let translation =
            grid.footprint_center(self.cell, def.dimensions.footprint_cells, self.rotation)
                + def.grid.placement_offset
                + rotation * def.visuals.model_offset;
        Transform {
            translation,
            rotation,
            scale: def.visuals.scale,
        }
    }
}
