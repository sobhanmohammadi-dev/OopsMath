//! The logical record of what has been placed, and the rules for placing.
//!
//! [`PlacedWorld`] is the authoritative occupancy: rendered entities never
//! decide whether a cell is free. [`evaluate_placement`] is a pure decision
//! (no rendering, no mutation), used both for the preview and for the final
//! placement so they can never disagree.

use std::collections::HashMap;

use bevy::prelude::*;
use thiserror::Error;

use super::definition::{BlockDefinition, BlockId};
use super::error::BlockError;
use super::grid::{GridCoord, GridRotation, GridSpec, rotated_cells};
use super::placement::{PlacedBlock, validate_placement};
use super::registry::BlockRegistry;
use super::stage_rules::StagePlacementRules;
use crate::items::ItemRef;
use crate::items::inventory::Inventory;

/// Why a placement was refused. Display text is player-facing.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum PlacementRejection {
    #[error("No block is selected.")]
    NothingSelected,
    #[error("'{0}' is not allowed in this stage.")]
    NotAllowed(BlockId),
    #[error("Block '{0}' is not registered.")]
    NotRegistered(BlockId),
    #[error("That position is outside the build area.")]
    OutOfBounds { block: BlockId, cell: IVec3 },
    #[error("That space is already occupied.")]
    Occupied { block: BlockId, cell: IVec3 },
    #[error("No support: build on the ground or against another block.")]
    Unsupported { block: BlockId },
    #[error("You have no '{0}' left in your inventory.")]
    NoInventory(BlockId),
    #[error("Placement limit reached: this stage allows at most {max} of '{block}'.")]
    LimitReached { block: BlockId, max: u32 },
    #[error("Invalid placement: {0}")]
    Invalid(BlockError),
}

/// One placed block: the logical record that pairs with a rendered entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedRecord {
    pub block: BlockId,
    pub cell: GridCoord,
    pub rotation: GridRotation,
    pub entity: Entity,
}

/// Cell occupancy and placed-block records for the active stage.
#[derive(Resource, Default, Debug)]
pub struct PlacedWorld {
    records: HashMap<Entity, PlacedRecord>,
    cells: HashMap<GridCoord, Entity>,
}

impl PlacedWorld {
    pub fn is_occupied(&self, cell: GridCoord) -> bool {
        self.cells.contains_key(&cell)
    }

    pub fn occupant(&self, cell: GridCoord) -> Option<Entity> {
        self.cells.get(&cell).copied()
    }

    pub fn record(&self, entity: Entity) -> Option<&PlacedRecord> {
        self.records.get(&entity)
    }

    /// Number of placed blocks (not cells).
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The cells owned by one placed block.
    pub fn cells_of(&self, entity: Entity) -> Vec<GridCoord> {
        self.cells()
            .filter(|(_, owner)| *owner == entity)
            .map(|(cell, _)| cell)
            .collect()
    }

    /// Moves a group of placed blocks straight down by `drop` cells, keeping
    /// their shape and orientation. All-or-nothing: if any target cell is held
    /// by a block outside the group nothing changes and `false` is returned.
    pub fn shift_down(&mut self, group: &[Entity], drop: u32) -> bool {
        if drop == 0 {
            return false;
        }
        let down = IVec3::new(0, -(drop as i32), 0);
        let moved: Vec<(Entity, Vec<GridCoord>)> =
            group.iter().map(|e| (*e, self.cells_of(*e))).collect();
        if moved
            .iter()
            .any(|(e, cells)| cells.is_empty() || !self.records.contains_key(e))
        {
            return false;
        }
        for (_, cells) in &moved {
            for cell in cells {
                self.cells.remove(cell);
            }
        }
        let free = moved.iter().all(|(_, cells)| {
            cells
                .iter()
                .all(|c| !self.cells.contains_key(&GridCoord(c.0 + down)))
        });
        for (entity, cells) in &moved {
            for cell in cells {
                let target = if free {
                    GridCoord(cell.0 + down)
                } else {
                    *cell
                };
                self.cells.insert(target, *entity);
            }
            if free && let Some(record) = self.records.get_mut(entity) {
                record.cell = GridCoord(record.cell.0 + down);
            }
        }
        free
    }

    /// Every occupied cell with its owning entity, in arbitrary order.
    pub fn cells(&self) -> impl Iterator<Item = (GridCoord, Entity)> + '_ {
        self.cells.iter().map(|(cell, entity)| (*cell, *entity))
    }

    /// Every occupied cell, in arbitrary order.
    pub fn occupied_cells(&self) -> impl Iterator<Item = GridCoord> + '_ {
        self.cells.keys().copied()
    }

    /// Number of placed *instances* of `block` (never occupied cells).
    pub fn count_of(&self, block: &BlockId) -> u32 {
        self.records.values().filter(|r| &r.block == block).count() as u32
    }

    /// Forgets one placed block and frees all its cells.
    pub fn remove(&mut self, entity: Entity) -> Option<PlacedRecord> {
        let record = self.records.remove(&entity)?;
        self.cells.retain(|_, owner| *owner != entity);
        Some(record)
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.cells.clear();
    }

    /// Records `placed` under `entity`. All-or-nothing: if any covered cell is
    /// taken, nothing is changed and `Occupied` is returned.
    pub fn insert(
        &mut self,
        placed: &PlacedBlock,
        def: &BlockDefinition,
        entity: Entity,
    ) -> Result<(), PlacementRejection> {
        if let Some(taken) = placed.occupied_cells(def).find(|c| self.is_occupied(*c)) {
            return Err(PlacementRejection::Occupied {
                block: placed.block.clone(),
                cell: taken.0,
            });
        }
        for cell in placed.occupied_cells(def) {
            self.cells.insert(cell, entity);
        }
        self.records.insert(
            entity,
            PlacedRecord {
                block: placed.block.clone(),
                cell: placed.cell,
                rotation: placed.rotation,
                entity,
            },
        );
        Ok(())
    }
}

/// Decides whether `selected` may be placed at `cell`. Order of checks:
/// selection, stage allow-list, registry, grid bounds / rotation, overlap.
/// Pure: takes the world by shared reference and mutates nothing.
pub fn evaluate_placement<'r>(
    registry: &'r BlockRegistry,
    grid: &GridSpec,
    world: &PlacedWorld,
    is_allowed: impl Fn(&BlockId) -> bool,
    selected: Option<&BlockId>,
    cell: GridCoord,
    rotation: GridRotation,
) -> Result<&'r BlockDefinition, PlacementRejection> {
    let id = selected.ok_or(PlacementRejection::NothingSelected)?;
    if !is_allowed(id) {
        return Err(PlacementRejection::NotAllowed(id.clone()));
    }
    let def = validate_placement(registry, grid, id, cell, rotation).map_err(|err| match err {
        BlockError::NotRegistered(id) => PlacementRejection::NotRegistered(id),
        BlockError::OutOfBounds { id, cell } => PlacementRejection::OutOfBounds { block: id, cell },
        other => PlacementRejection::Invalid(other),
    })?;
    let footprint = def.dimensions.footprint_cells;
    if let Some(taken) = rotated_cells(cell, footprint, rotation).find(|c| world.is_occupied(*c)) {
        return Err(PlacementRejection::Occupied {
            block: id.clone(),
            cell: taken.0,
        });
    }
    Ok(def)
}

/// Full placement decision: geometry and stage permission
/// ([`evaluate_placement`]), then the stage's instance limit, then inventory.
/// Pure, so the preview and the real placement use the same rules.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_commit<'r>(
    registry: &'r BlockRegistry,
    grid: &GridSpec,
    world: &PlacedWorld,
    rules: &StagePlacementRules,
    inventory: &Inventory,
    selected: Option<&BlockId>,
    cell: GridCoord,
    rotation: GridRotation,
) -> Result<&'r BlockDefinition, PlacementRejection> {
    let def = evaluate_placement(
        registry,
        grid,
        world,
        |id| rules.is_allowed(id),
        selected,
        cell,
        rotation,
    )?;
    if let Some(max) = rules.max_placed(&def.id)
        && world.count_of(&def.id) >= max
    {
        return Err(PlacementRejection::LimitReached {
            block: def.id.clone(),
            max,
        });
    }
    if !inventory.has(&ItemRef::Block(def.id.clone()), 1) {
        return Err(PlacementRejection::NoInventory(def.id.clone()));
    }
    Ok(def)
}

/// Records the placement and consumes exactly one inventory unit, all or
/// nothing: if either step fails, neither the world nor the inventory changed.
pub fn commit_placement(
    world: &mut PlacedWorld,
    inventory: &mut Inventory,
    placed: &PlacedBlock,
    def: &BlockDefinition,
    entity: Entity,
) -> Result<(), PlacementRejection> {
    world.insert(placed, def, entity)?;
    if inventory
        .remove(&ItemRef::Block(placed.block.clone()), 1)
        .is_err()
    {
        world.remove(entity);
        return Err(PlacementRejection::NoInventory(placed.block.clone()));
    }
    Ok(())
}

/// Why a removal did nothing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemovalError {
    /// The entity is not a placed block (already removed, a preview, or any
    /// unrelated entity). Nothing was changed.
    #[error("There is no placed block to remove.")]
    NotPlaced,
    /// The refund could not be stored (quantity limit); nothing was changed.
    #[error("The block could not be returned to your inventory.")]
    RefundFailed,
}

/// Removes one placed block from the logical state and refunds it.
///
/// Refund policy: exactly one unit of the removed block goes back to the
/// inventory. The refund is applied first and the record removed second, and
/// only when the entity is a recorded placed block, so a repeated call, a
/// preview or an unrelated entity gives `NotPlaced` and never a second refund.
/// If the refund cannot be stored, nothing is removed. Only the cells the
/// record owns (its actual, rotated footprint) are released; the caller
/// despawns the entity afterwards.
pub fn remove_block(
    world: &mut PlacedWorld,
    inventory: &mut Inventory,
    entity: Entity,
) -> Result<PlacedRecord, RemovalError> {
    let block = world
        .record(entity)
        .ok_or(RemovalError::NotPlaced)?
        .block
        .clone();
    let item = ItemRef::Block(block);
    inventory
        .add(item.clone(), 1)
        .map_err(|_| RemovalError::RefundFailed)?;
    match world.remove(entity) {
        Some(record) => Ok(record),
        None => {
            // Cannot happen (the record was just seen); undo the refund anyway.
            let _ = inventory.remove(&item, 1);
            Err(RemovalError::NotPlaced)
        }
    }
}
