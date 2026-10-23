//! Pure plan calculation for a blueprint (no ECS, no mutation).
//!
//! Everything is derived from the canonical sources: block footprints and
//! rotation from `BlockDefinition` (via `rotated_cells`), names and prices from
//! the registry (`BlockEconomyConfig`, read through the shop's
//! `purchasable_price`), permissions and limits from `StagePlacementRules`, and
//! owned quantities from the `Inventory`. Nothing is charged or consumed.
//!
//! Three separate quantities are reported: **geometry** (bounds and size),
//! **materials** (counts per block id) and **money** (estimated coins).

use std::collections::BTreeMap;

use bevy::prelude::*;
use thiserror::Error;

use super::Blueprint;
use crate::blocks::grid::rotated_cells;
use crate::blocks::stage_rules::StagePlacementRules;
use crate::blocks::{BlockId, BlockRegistry, PlacedWorld};
use crate::items::ItemRef;
use crate::items::inventory::Inventory;
use crate::items::shop::purchasable_price;

/// One material of a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialLine {
    pub block: BlockId,
    pub name: String,
    /// Planned instances of this block.
    pub required: u32,
    /// Units currently in the inventory.
    pub available: u32,
    /// Shop price of one unit; `None` when the block has no valid price.
    pub unit_price: Option<u32>,
    /// `unit_price * required`; `None` without a price or on overflow.
    pub line_cost: Option<u32>,
}

impl MaterialLine {
    /// Units still to get (0 when the inventory already covers the plan).
    pub fn missing(&self) -> u32 {
        self.required.saturating_sub(self.available)
    }
}

/// Something worth telling the player about a plan.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanIssue {
    #[error("'{0}' is not a registered block.")]
    Unregistered(BlockId),
    #[error("'{0}' is not allowed in this stage.")]
    NotAllowed(BlockId),
    #[error("The plan needs {required} of '{block}' but this stage allows at most {max}.")]
    ExceedsStageLimit {
        block: BlockId,
        required: u32,
        max: u32,
    },
    #[error("'{0}' has no valid price, so the cost cannot be estimated.")]
    NoPrice(BlockId),
    #[error("The estimated cost is too large to calculate.")]
    CostOverflow,
}

impl PlanIssue {
    /// Blocking issues make the plan impossible to build in this stage; the
    /// others are informational.
    pub fn is_blocking(&self) -> bool {
        matches!(
            self,
            Self::Unregistered(_) | Self::NotAllowed(_) | Self::ExceedsStageLimit { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Lowest occupied cell of the plan.
    pub min: IVec3,
    /// Extent in cells: x = width, y = height, z = depth (grid axes).
    pub size: UVec3,
    pub total_blocks: u32,
    /// Sorted by block id.
    pub lines: Vec<MaterialLine>,
    /// Cost of buying every planned block at shop prices; `None` if any block
    /// has no price or the sum overflows (see `issues`).
    pub total_cost: Option<u32>,
    /// Cost of the units not yet in the inventory (same `None` rule).
    pub missing_cost: Option<u32>,
    pub issues: Vec<PlanIssue>,
}

impl Plan {
    pub fn compute(
        blueprint: &Blueprint,
        registry: &BlockRegistry,
        rules: &StagePlacementRules,
        inventory: &Inventory,
    ) -> Self {
        let mut counts: BTreeMap<BlockId, u32> = BTreeMap::new();
        let (mut lo, mut hi) = (IVec3::MAX, IVec3::MIN);
        for planned in &blueprint.blocks {
            *counts.entry(planned.block.clone()).or_insert(0) += 1;
            if let Some(def) = registry.get(&planned.block) {
                for cell in rotated_cells(
                    planned.cell,
                    def.dimensions.footprint_cells,
                    planned.rotation,
                ) {
                    lo = lo.min(cell.0);
                    hi = hi.max(cell.0);
                }
            }
        }
        let (min, size) = if lo.x > hi.x {
            (IVec3::ZERO, UVec3::ZERO)
        } else {
            (lo, (hi - lo + IVec3::ONE).as_uvec3())
        };

        let mut issues = Vec::new();
        let mut lines = Vec::with_capacity(counts.len());
        for (block, required) in counts {
            let def = registry.get(&block);
            if def.is_none() {
                issues.push(PlanIssue::Unregistered(block.clone()));
            }
            if !rules.is_allowed(&block) {
                issues.push(PlanIssue::NotAllowed(block.clone()));
            } else if let Some(max) = rules.max_placed(&block)
                && required > max
            {
                issues.push(PlanIssue::ExceedsStageLimit {
                    block: block.clone(),
                    required,
                    max,
                });
            }
            let unit_price = def.and_then(purchasable_price);
            let line_cost = unit_price.and_then(|price| price.checked_mul(required));
            match (unit_price, line_cost) {
                (None, _) if def.is_some() => issues.push(PlanIssue::NoPrice(block.clone())),
                (Some(_), None) => issues.push(PlanIssue::CostOverflow),
                _ => {}
            }
            lines.push(MaterialLine {
                name: def.map_or_else(|| block.to_string(), |d| d.display_name.clone()),
                available: inventory.quantity(&ItemRef::Block(block.clone())),
                block,
                required,
                unit_price,
                line_cost,
            });
        }

        let total_cost = sum_costs(lines.iter().map(|l| l.line_cost));
        let missing_cost = sum_costs(lines.iter().map(|l| {
            l.unit_price
                .and_then(|price| price.checked_mul(l.missing()))
        }));
        // Every line is priced but the total overflowed.
        if total_cost.is_none()
            && lines.iter().all(|l| l.line_cost.is_some())
            && !issues.contains(&PlanIssue::CostOverflow)
        {
            issues.push(PlanIssue::CostOverflow);
        }
        issues.dedup();

        Self {
            min,
            size,
            total_blocks: u32::try_from(blueprint.blocks.len()).unwrap_or(u32::MAX),
            lines,
            total_cost,
            missing_cost,
            issues,
        }
    }

    /// False when a blocking issue makes the plan impossible in this stage.
    pub fn is_buildable(&self) -> bool {
        !self.issues.iter().any(PlanIssue::is_blocking)
    }

    /// True when the inventory does not cover every planned block yet.
    pub fn has_shortage(&self) -> bool {
        self.lines.iter().any(|l| l.missing() > 0)
    }
}

/// Checked sum; `None` if any term is missing or the sum overflows.
fn sum_costs(mut costs: impl Iterator<Item = Option<u32>>) -> Option<u32> {
    costs.try_fold(0u32, |sum, cost| sum.checked_add(cost?))
}

// ------------------------------------------------------------- progress

/// How a planned block relates to the real (authoritative) construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    /// Nothing is there yet.
    Pending,
    /// A placed block with the same id, anchor cell and rotation exists.
    Built,
    /// The cells are used by something else (another block or another orientation).
    Obstructed,
}

/// Compares the plan with the placed blocks. Evaluated live, so a block that is
/// later removed or moved by a collapse stops counting. Previews never take
/// part: only `PlacedWorld` is read.
pub fn plan_statuses(
    blueprint: &Blueprint,
    registry: &BlockRegistry,
    world: &PlacedWorld,
) -> Vec<PlanStatus> {
    blueprint
        .blocks
        .iter()
        .map(|planned| {
            let matches = world
                .occupant(planned.cell)
                .and_then(|entity| world.record(entity))
                .is_some_and(|record| {
                    record.block == planned.block
                        && record.cell == planned.cell
                        && record.rotation == planned.rotation
                });
            if matches {
                return PlanStatus::Built;
            }
            let blocked = registry.get(&planned.block).is_some_and(|def| {
                rotated_cells(
                    planned.cell,
                    def.dimensions.footprint_cells,
                    planned.rotation,
                )
                .any(|cell| world.is_occupied(cell))
            });
            if blocked {
                PlanStatus::Obstructed
            } else {
                PlanStatus::Pending
            }
        })
        .collect()
}

/// Number of planned blocks that are built.
pub fn built_count(statuses: &[PlanStatus]) -> usize {
    statuses.iter().filter(|s| **s == PlanStatus::Built).count()
}
