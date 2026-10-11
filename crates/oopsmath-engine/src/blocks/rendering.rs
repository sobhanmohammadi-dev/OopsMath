//! Spawning placed blocks into the world.
//!
//! Only visuals and placement are handled here; purchasing, inventory and
//! objectives belong to gameplay code.

use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

use super::assets::BlockAssets;
use super::definition::BlockId;
use super::error::BlockSpawnError;
use super::grid::{GridCoord, GridRotation, GridSpec};
use super::placement::PlacedBlock;
use super::registry::BlockRegistry;

/// Validates the placement, requires the model to be loaded, and spawns the
/// block's scene with the transform from its definition.
///
/// The spawned entity carries [`PlacedBlock`] (identity + grid state) and a
/// `WorldAssetRoot` that reuses the shared model handle.
pub fn spawn_block(
    commands: &mut Commands,
    registry: &BlockRegistry,
    assets: &BlockAssets,
    grid: &GridSpec,
    block: BlockId,
    cell: GridCoord,
    rotation: GridRotation,
) -> Result<Entity, BlockSpawnError> {
    let placed = PlacedBlock::new(registry, grid, block, cell, rotation)?;
    let def = registry.require(&placed.block)?;
    let handle = assets.ready_handle(&placed.block)?.clone();
    let transform = placed.transform(def, grid);
    let name = Name::new(def.display_name.clone());
    Ok(commands
        .spawn((name, placed, transform, WorldAssetRoot(handle)))
        .id())
}
