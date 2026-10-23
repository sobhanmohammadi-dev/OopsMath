//! Blueprints: construction plans the player can inspect, cost out and follow.
//!
//! # Data (stage format, Compiler 1.2, additive)
//! `construction.blueprints` (optional) in the stage:
//!
//! ```yaml
//! construction:
//!   blueprints:
//!     - id: bp_wall_a
//!       title: stage.my_stage.blueprint.wall      # optional localization key
//!       blocks:
//!         - {block: block.wood_log, position: [0, 0, 0]}
//!         - {block: block.wood_log, position: [1, 0, 0], rotation: 90}
//! ```
//!
//! `position` is the block's anchor cell (grid cell relative to the world origin,
//! the same cell the player aims at when placing it); `rotation` is the yaw in
//! degrees (0, 90, 180, 270; default 0), the same quarter turns as normal
//! placement. Footprints, rotation rules and bounds all come from the block
//! registry and the stage grid; nothing about blocks is stored here.
//! Stages without blueprints are unaffected.
//!
//! # What it does
//! * [`plan::Plan`]: bounds/size, material counts, estimated cost, stage
//!   restrictions and shortages: pure, read-only calculation.
//! * [`plan::plan_statuses`]: which planned blocks are really built (same id,
//!   anchor cell and rotation in `PlacedWorld`), evaluated live.
//! * A **guide** drawn with gizmo outlines (see below). The plan never places,
//!   charges or consumes anything: construction stays manual and goes through
//!   the normal placement rules (inventory, limits, stability).
//!
//! # Preview
//! The preview is a set of wireframe boxes (plus a small arrow showing the
//! block's forward direction) drawn every frame from the blueprint. They are
//! not entities, so they cannot occupy the grid, count for objectives, take part
//! in stability, or leave anything behind when the stage or blueprint changes.
//! Block models are not ghosted (that would need material overrides on loaded
//! scenes).

pub mod plan;

use std::collections::HashMap;

use bevy::prelude::*;
use serde_json::Value;
use thiserror::Error;

use crate::app::stages::LoadedStage;
use crate::app::states::AppState;
use crate::blocks::debug_scene::draw_box;
use crate::blocks::error::BlockError;
use crate::blocks::grid::{GridCoord, GridRotation, GridSpec, footprint_box, rotated_cells};
use crate::blocks::placement::validate_placement;
use crate::blocks::{BlockId, BlockRegistry, PlacedWorld};
use crate::scene::{StageScene, stage_scene_from};
use crate::stage::localization::{StageLocale, StageMessages};
pub use plan::{MaterialLine, Plan, PlanIssue, PlanStatus, built_count, plan_statuses};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedBlock {
    pub block: BlockId,
    /// Anchor cell, as in normal placement.
    pub cell: GridCoord,
    pub rotation: GridRotation,
}

/// Immutable definition from stage data. Runtime state lives elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blueprint {
    pub id: String,
    pub title: String,
    pub blocks: Vec<PlannedBlock>,
}

// `BlockError` is PartialEq but not Eq, so this enum is PartialEq only.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum BlueprintError {
    #[error("blueprint id '{0}' is used more than once")]
    DuplicateId(String),
    #[error("blueprint '{id}': {reason}")]
    Invalid { id: String, reason: String },
    #[error("blueprint '{id}', block {index}: '{block}' is not a registered block")]
    UnknownBlock {
        id: String,
        index: usize,
        block: String,
    },
    #[error("blueprint '{id}', block {index}: {cause}")]
    Placement {
        id: String,
        index: usize,
        cause: BlockError,
    },
    #[error("blueprint '{id}': block {second} overlaps block {first} at cell {cell}")]
    Overlap {
        id: String,
        first: usize,
        second: usize,
        cell: IVec3,
    },
}

/// Parses and validates `construction.blueprints` against the registry and the
/// stage grid (existing placement rules: bounds and allowed rotations), and
/// rejects overlapping planned blocks using their real, rotated footprints.
pub fn parse_blueprints(
    construction: &Value,
    registry: &BlockRegistry,
    grid: &GridSpec,
    title_for: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<Blueprint>, BlueprintError> {
    let Some(list) = construction.get("blueprints") else {
        return Ok(Vec::new());
    };
    let invalid = |id: &str, reason: &str| BlueprintError::Invalid {
        id: id.to_string(),
        reason: reason.to_string(),
    };
    let list = list
        .as_array()
        .ok_or_else(|| invalid("?", "blueprints must be an array"))?;

    let mut blueprints: Vec<Blueprint> = Vec::with_capacity(list.len());
    for entry in list {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("?", "a blueprint without a string 'id'"))?
            .to_string();
        if blueprints.iter().any(|b| b.id == id) {
            return Err(BlueprintError::DuplicateId(id));
        }
        let title = entry
            .get("title")
            .and_then(Value::as_str)
            .map(|key| title_for(key).unwrap_or_else(|| key.to_string()))
            .unwrap_or_else(|| id.clone());
        let planned_list = entry
            .get("blocks")
            .and_then(Value::as_array)
            .filter(|blocks| !blocks.is_empty())
            .ok_or_else(|| invalid(&id, "'blocks' must be a non-empty list"))?;

        let mut blocks = Vec::with_capacity(planned_list.len());
        let mut occupied: HashMap<GridCoord, usize> = HashMap::new();
        for (index, planned) in planned_list.iter().enumerate() {
            let block = planned
                .get("block")
                .and_then(Value::as_str)
                .filter(|b| !b.is_empty())
                .ok_or_else(|| invalid(&id, "every block needs a 'block' id"))?;
            let position = planned
                .get("position")
                .and_then(Value::as_array)
                .filter(|p| p.len() == 3)
                .and_then(|p| {
                    let n = |i: usize| p[i].as_i64().and_then(|v| i32::try_from(v).ok());
                    Some(GridCoord::new(n(0)?, n(1)?, n(2)?))
                })
                .ok_or_else(|| invalid(&id, "'position' must be three integers [x, y, z]"))?;
            let rotation = match planned.get("rotation") {
                None | Some(Value::Null) => GridRotation::Deg0,
                Some(value) => match value.as_i64() {
                    Some(0) => GridRotation::Deg0,
                    Some(90) => GridRotation::Deg90,
                    Some(180) => GridRotation::Deg180,
                    Some(270) => GridRotation::Deg270,
                    _ => return Err(invalid(&id, "'rotation' must be 0, 90, 180 or 270")),
                },
            };

            let block_id = BlockId::new(block);
            let def = registry
                .get(&block_id)
                .ok_or_else(|| BlueprintError::UnknownBlock {
                    id: id.clone(),
                    index,
                    block: block.to_string(),
                })?;
            validate_placement(registry, grid, &block_id, position, rotation).map_err(|cause| {
                BlueprintError::Placement {
                    id: id.clone(),
                    index,
                    cause,
                }
            })?;
            for cell in rotated_cells(position, def.dimensions.footprint_cells, rotation) {
                if let Some(first) = occupied.insert(cell, index) {
                    return Err(BlueprintError::Overlap {
                        id,
                        first,
                        second: index,
                        cell: cell.0,
                    });
                }
            }
            blocks.push(PlannedBlock {
                block: block_id,
                cell: position,
                rotation,
            });
        }
        blueprints.push(Blueprint { id, title, blocks });
    }
    Ok(blueprints)
}

/// The active stage's blueprints (immutable once loaded).
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct StageBlueprints {
    pub blueprints: Vec<Blueprint>,
    /// Why the blueprints could not be used; the list is empty in that case.
    pub error: Option<String>,
}

impl StageBlueprints {
    pub fn from_stage(
        construction: &Value,
        registry: &BlockRegistry,
        grid: &GridSpec,
        title_for: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        match parse_blueprints(construction, registry, grid, title_for) {
            Ok(blueprints) => Self {
                blueprints,
                error: None,
            },
            Err(err) => Self {
                blueprints: Vec::new(),
                error: Some(err.to_string()),
            },
        }
    }

    pub fn get(&self, index: usize) -> Option<&Blueprint> {
        self.blueprints.get(index)
    }

    /// Whether the Plan button should be shown.
    pub fn is_available(&self) -> bool {
        !self.blueprints.is_empty() || self.error.is_some()
    }
}

/// Planning UI state: panel and guide. Never touches gameplay state.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct BlueprintMode {
    pub panel_open: bool,
    pub selected: usize,
    /// Draw the selected blueprint on the grid as a construction guide.
    pub guide: bool,
}

impl BlueprintMode {
    pub fn open(&mut self) {
        self.panel_open = true;
    }

    pub fn close(&mut self) {
        self.panel_open = false;
    }

    /// Selects another blueprint (wrapping); the guide follows the selection.
    pub fn step(&mut self, delta: i32, count: usize) {
        if count > 0 {
            self.selected = (self.selected as i32 + delta).rem_euclid(count as i32) as usize;
        }
    }

    /// "Start building": show the guide and return to the game.
    pub fn start_building(&mut self) {
        self.guide = true;
        self.panel_open = false;
    }

    pub fn stop_guide(&mut self) {
        self.guide = false;
    }
}

/// Status of every planned block of the guided blueprint (empty without a guide).
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct BlueprintProgress {
    pub statuses: Vec<PlanStatus>,
}

impl BlueprintProgress {
    pub fn built(&self) -> usize {
        built_count(&self.statuses)
    }
}

pub struct BlueprintPlugin;

impl Plugin for BlueprintPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StageBlueprints>()
            .init_resource::<BlueprintMode>()
            .init_resource::<BlueprintProgress>()
            .add_systems(OnEnter(AppState::InGame), init_blueprints)
            .add_systems(OnExit(AppState::InGame), clear_blueprints)
            .add_systems(
                Update,
                (update_progress, draw_guide)
                    .chain()
                    .run_if(in_state(AppState::InGame).and_then(resource_exists::<StageScene>)),
            );
    }
}

/// Loads the blueprints of the stage that just became active (and resets the UI state).
fn init_blueprints(
    loaded: Res<LoadedStage>,
    registry: Res<BlockRegistry>,
    locale: Res<StageLocale>,
    mut blueprints: ResMut<StageBlueprints>,
    mut mode: ResMut<BlueprintMode>,
    mut progress: ResMut<BlueprintProgress>,
) {
    *mode = BlueprintMode::default();
    *progress = BlueprintProgress::default();
    let Some(package) = loaded.0.as_ref() else {
        *blueprints = StageBlueprints::default();
        return;
    };
    let grid = stage_scene_from(&package.stage).grid_spec();
    let messages = StageMessages::from_localizations(&package.localization);
    let title_for = |key: &str| {
        messages
            .resolve(key, &locale.locale, &locale.fallback)
            .map(str::to_owned)
    };
    *blueprints =
        StageBlueprints::from_stage(&package.stage.construction, &registry, &grid, &title_for);
    if let Some(error) = &blueprints.error {
        error!(
            "Invalid blueprints in stage '{}': {error}",
            package.meta.stage_id
        );
    }
}

fn clear_blueprints(
    mut blueprints: ResMut<StageBlueprints>,
    mut mode: ResMut<BlueprintMode>,
    mut progress: ResMut<BlueprintProgress>,
) {
    *blueprints = StageBlueprints::default();
    *mode = BlueprintMode::default();
    *progress = BlueprintProgress::default();
}

/// Recomputes the guide's progress when the placed blocks, the blueprints or
/// the mode change (never every frame).
fn update_progress(
    world: Res<PlacedWorld>,
    blueprints: Res<StageBlueprints>,
    mode: Res<BlueprintMode>,
    registry: Res<BlockRegistry>,
    mut progress: ResMut<BlueprintProgress>,
) {
    if !(world.is_changed() || blueprints.is_changed() || mode.is_changed()) {
        return;
    }
    let statuses = if mode.guide {
        blueprints
            .get(mode.selected)
            .map(|blueprint| plan_statuses(blueprint, &registry, &world))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if progress.statuses != statuses {
        progress.statuses = statuses;
    }
}

/// Draws the guide: cyan = still to build, green = built, orange = obstructed.
fn draw_guide(
    mut gizmos: Gizmos,
    mode: Res<BlueprintMode>,
    blueprints: Res<StageBlueprints>,
    progress: Res<BlueprintProgress>,
    registry: Res<BlockRegistry>,
    scene: Res<StageScene>,
) {
    if !mode.guide {
        return;
    }
    let Some(blueprint) = blueprints.get(mode.selected) else {
        return;
    };
    let grid = scene.grid_spec();
    for (index, planned) in blueprint.blocks.iter().enumerate() {
        let Some(def) = registry.get(&planned.block) else {
            continue;
        };
        let footprint = def.dimensions.footprint_cells;
        let (min, size) = footprint_box(planned.cell, footprint, planned.rotation);
        let lo = grid.cell_min_corner(min);
        let hi = lo + size.as_vec3() * grid.cell_size;
        let color = match progress.statuses.get(index) {
            Some(PlanStatus::Built) => Color::srgba(0.3, 0.8, 0.4, 0.5),
            Some(PlanStatus::Obstructed) => Color::srgb(1.0, 0.55, 0.1),
            _ => Color::srgb(0.3, 0.8, 1.0),
        };
        draw_box(&mut gizmos, lo, hi, color);
        // Arrow along the block's local +X, so its orientation is visible.
        let center = grid.footprint_center(planned.cell, footprint, planned.rotation);
        let forward = planned.rotation.to_quat() * Vec3::X;
        gizmos.line(center, center + forward * 0.45 * grid.cell_size.x, color);
    }
}

#[cfg(test)]
mod tests;
