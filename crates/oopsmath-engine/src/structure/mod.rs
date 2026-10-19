//! Structural stability and collapse: MVP.
//!
//! # The rule
//! * The **ground** (the grid floor layer, `y == floor_y`) is the only
//!   foundation.
//! * A placed block is one structural object, however many cells it covers
//!   (a rotated or multi-cell block is one node using its real cells).
//! * Blocks are connected when cells share a **face**. Diagonal or edge contact
//!   is not a connection.
//! * A block is **supported** if it touches the ground or is connected, through
//!   other blocks, to one that does. Everything else is **unsupported**.
//!
//! This is connectivity only: no weight, strength, overhang limit or physics.
//!
//! # Placement
//! A block can only be placed (and the preview only shows green) where it would
//! be supported: on the ground or face-to-face with another block.
//!
//! # Collapse = landing, never deleting
//! Whenever `PlacedWorld` changes, [`settle`] looks for unsupported blocks.
//! Touching unsupported blocks form one rigid group; a group falls straight down
//! by the smallest distance any member has before meeting the ground or a block
//! that is not part of the group, and **stays there**. Groups are handled
//! lowest first, and the search repeats until everything is supported, so a
//! group lands on whatever the groups below it have become.
//!
//! The *logical* move is immediate and atomic ([`PlacedWorld::shift_down`]), so
//! cells, placed-block records, placement limits and objective counts are always
//! consistent. The entity then plays a short fall animation to its new position
//! ([`FallAnimations`]; purely visual). Nothing is despawned, refunded or
//! charged.
//!
//! # Extending later
//! Support rule changes live in [`unsupported_blocks`] / [`placement_supported`]
//! (e.g. overhang limits, anchored blocks, per-block roles); landing behaviour in
//! [`settle`] (e.g. breaking blocks on hard landings); the animation is separate.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use crate::app::states::AppState;
use crate::blocks::PlacementFeedback;
use crate::blocks::definition::BlockDefinition;
use crate::blocks::grid::{GridCoord, GridRotation, GridSpec, rotated_cells};
use crate::blocks::occupancy::{PlacedWorld, PlacementRejection};
use crate::scene::StageScene;

/// Acceleration of the visual fall, in world units per second squared.
pub const FALL_ACCEL: f32 = 12.0;
/// Shortest fall animation.
pub const MIN_FALL_SECS: f32 = 0.25;
/// Safety bound on settle rounds (each round lands at least one group).
const MAX_SETTLE_ROUNDS: usize = 256;

const NEIGHBOURS: [IVec3; 6] = [
    IVec3::X,
    IVec3::NEG_X,
    IVec3::Y,
    IVec3::NEG_Y,
    IVec3::Z,
    IVec3::NEG_Z,
];

/// The floor layer of a grid.
pub fn floor_y(grid: &GridSpec) -> i32 {
    grid.bounds.map_or(0, |b| b.min.y)
}

// ------------------------------------------------------------------ support

/// Placed blocks (sorted) with no support path to the ground.
pub fn unsupported_blocks(world: &PlacedWorld, floor_y: i32) -> Vec<Entity> {
    let mut cells_of: HashMap<Entity, Vec<GridCoord>> = HashMap::new();
    for (cell, entity) in world.cells() {
        cells_of.entry(entity).or_default().push(cell);
    }

    let mut supported: HashSet<Entity> = HashSet::new();
    let mut queue: VecDeque<Entity> = VecDeque::new();
    for (entity, cells) in &cells_of {
        if cells.iter().any(|c| c.0.y == floor_y) {
            supported.insert(*entity);
            queue.push_back(*entity);
        }
    }
    while let Some(entity) = queue.pop_front() {
        for cell in &cells_of[&entity] {
            for step in NEIGHBOURS {
                if let Some(other) = world.occupant(GridCoord(cell.0 + step))
                    && supported.insert(other)
                {
                    queue.push_back(other);
                }
            }
        }
    }

    let mut unsupported: Vec<Entity> = cells_of
        .keys()
        .filter(|e| !supported.contains(*e))
        .copied()
        .collect();
    unsupported.sort();
    unsupported
}

/// Whether a block covering `cells` would be supported: it touches the ground
/// or shares a face with a placed block (every placed block is supported).
/// Read-only.
pub fn placement_supported(world: &PlacedWorld, floor_y: i32, cells: &[GridCoord]) -> bool {
    cells.iter().any(|cell| {
        cell.0.y == floor_y
            || NEIGHBOURS.iter().any(|step| {
                let neighbour = GridCoord(cell.0 + *step);
                !cells.contains(&neighbour) && world.occupant(neighbour).is_some()
            })
    })
}

/// Support check for a candidate placement, as a placement verdict.
pub fn check_placement_support(
    world: &PlacedWorld,
    grid: &GridSpec,
    def: &BlockDefinition,
    cell: GridCoord,
    rotation: GridRotation,
) -> Result<(), PlacementRejection> {
    let cells: Vec<GridCoord> =
        rotated_cells(cell, def.dimensions.footprint_cells, rotation).collect();
    if placement_supported(world, floor_y(grid), &cells) {
        Ok(())
    } else {
        Err(PlacementRejection::Unsupported {
            block: def.id.clone(),
        })
    }
}

// ------------------------------------------------------------------ landing

/// Splits blocks into groups of face-touching blocks (each sorted; groups
/// ordered by their lowest cell, then first entity).
fn groups_of(world: &PlacedWorld, blocks: &[Entity]) -> Vec<Vec<Entity>> {
    let wanted: HashSet<Entity> = blocks.iter().copied().collect();
    let mut seen: HashSet<Entity> = HashSet::new();
    let mut groups: Vec<Vec<Entity>> = Vec::new();
    for start in blocks {
        if !seen.insert(*start) {
            continue;
        }
        let mut members = vec![*start];
        let mut next = 0;
        while next < members.len() {
            let current = members[next];
            next += 1;
            for cell in world.cells_of(current) {
                for step in NEIGHBOURS {
                    if let Some(other) = world.occupant(GridCoord(cell.0 + step))
                        && wanted.contains(&other)
                        && seen.insert(other)
                    {
                        members.push(other);
                    }
                }
            }
        }
        members.sort();
        groups.push(members);
    }
    let lowest = |group: &Vec<Entity>| {
        group
            .iter()
            .flat_map(|e| world.cells_of(*e))
            .map(|c| c.0.y)
            .min()
            .unwrap_or(0)
    };
    groups.sort_by_key(|g| (lowest(g), g[0]));
    groups
}

/// How many cells a rigid group can fall before meeting the ground or a block
/// outside the group.
pub fn landing_drop(world: &PlacedWorld, floor_y: i32, group: &[Entity]) -> u32 {
    let members: HashSet<Entity> = group.iter().copied().collect();
    let mut drop = u32::MAX;
    for entity in group {
        for cell in world.cells_of(*entity) {
            let mut steps: i32 = 0;
            loop {
                let below = GridCoord(cell.0 - IVec3::Y * (steps + 1));
                if below.0.y < floor_y {
                    break;
                }
                if let Some(other) = world.occupant(below)
                    && !members.contains(&other)
                {
                    break;
                }
                steps += 1;
            }
            drop = drop.min(steps as u32);
        }
    }
    if drop == u32::MAX { 0 } else { drop }
}

/// One group that fell and now rests at its new cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landing {
    pub blocks: Vec<Entity>,
    /// Cells fallen.
    pub drop: u32,
}

/// Makes every block supported by letting unsupported groups fall until they
/// rest. Mutates only the placed-block records and occupancy (nothing is
/// removed) and returns what moved. Calling it again returns nothing.
pub fn settle(world: &mut PlacedWorld, floor_y: i32) -> Vec<Landing> {
    let mut landings = Vec::new();
    for _ in 0..MAX_SETTLE_ROUNDS {
        let unsupported = unsupported_blocks(world, floor_y);
        if unsupported.is_empty() {
            break;
        }
        let mut moved_any = false;
        for group in groups_of(world, &unsupported) {
            // Recomputed per group: groups below have already landed.
            let drop = landing_drop(world, floor_y, &group);
            if world.shift_down(&group, drop) {
                moved_any = true;
                landings.push(Landing {
                    blocks: group,
                    drop,
                });
            }
        }
        if !moved_any {
            break; // cannot happen with the rule above; never loop forever
        }
    }
    landings
}

// ---------------------------------------------------------------- animation

/// Visual fall of one entity towards its (already updated) cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fall {
    /// World units still to fall in total (adds up if it falls again).
    pub drop: f32,
    pub elapsed: f32,
    pub duration: f32,
    /// Height when the animation first moved the entity.
    pub from_y: Option<f32>,
}

fn duration_for(drop: f32) -> f32 {
    (2.0 * drop / FALL_ACCEL).sqrt().max(MIN_FALL_SECS)
}

/// Distance dropped after `elapsed` seconds, never more than `drop`.
pub fn fall_offset(elapsed: f32, drop: f32) -> f32 {
    (0.5 * FALL_ACCEL * elapsed * elapsed).min(drop)
}

/// Entities currently animating a fall. Purely visual; cleared with the stage.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct FallAnimations {
    active: HashMap<Entity, Fall>,
}

impl FallAnimations {
    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    pub fn get(&self, entity: Entity) -> Option<&Fall> {
        self.active.get(&entity)
    }

    pub fn clear(&mut self) {
        self.active.clear();
    }

    /// Starts (or extends) the fall of `entity` by `drop` world units.
    pub fn begin(&mut self, entity: Entity, drop: f32) {
        let fall = self.active.entry(entity).or_insert(Fall {
            drop: 0.0,
            elapsed: 0.0,
            duration: MIN_FALL_SECS,
            from_y: None,
        });
        fall.drop += drop;
        fall.duration = duration_for(fall.drop);
    }

    pub fn advance(&mut self, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        for fall in self.active.values_mut() {
            fall.elapsed += dt;
        }
    }

    /// Forgets animations that have played out; returns how many.
    pub fn prune_finished(&mut self) -> usize {
        let before = self.active.len();
        self.active.retain(|_, f| f.elapsed < f.duration);
        before - self.active.len()
    }
}

// ------------------------------------------------------------------ systems

pub struct StructurePlugin;

impl Plugin for StructurePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FallAnimations>()
            .add_systems(OnEnter(AppState::InGame), reset_structure)
            .add_systems(OnExit(AppState::InGame), reset_structure)
            .add_systems(
                Update,
                (recheck_stability, animate_falls)
                    .chain()
                    .run_if(in_state(AppState::InGame).and_then(resource_exists::<StageScene>)),
            );
    }
}

fn reset_structure(mut animations: ResMut<FallAnimations>) {
    animations.clear();
}

/// When the placed blocks changed, lets unsupported groups fall and land.
fn recheck_stability(
    mut world: ResMut<PlacedWorld>,
    scene: Res<StageScene>,
    mut animations: ResMut<FallAnimations>,
    mut feedback: ResMut<PlacementFeedback>,
) {
    if !world.is_changed() {
        return;
    }
    let grid = scene.grid_spec();
    let floor = floor_y(&grid);
    if unsupported_blocks(&world, floor).is_empty() {
        return; // read-only path: nothing is marked as changed
    }
    let landings = settle(&mut world, floor);
    let mut fallen = 0;
    for landing in &landings {
        for entity in &landing.blocks {
            animations.begin(*entity, landing.drop as f32 * grid.cell_size.y);
            fallen += 1;
        }
    }
    if fallen > 0 {
        feedback.0 = Some(format!("The structure collapsed: {fallen} block(s) fell."));
    }
}

/// Plays the fall of entities whose logical cells have already moved.
fn animate_falls(
    time: Res<Time>,
    mut animations: ResMut<FallAnimations>,
    mut transforms: Query<&mut Transform>,
) {
    if animations.is_empty() {
        return;
    }
    animations.advance(time.delta_secs());
    for (entity, fall) in animations.active.iter_mut() {
        match transforms.get_mut(*entity) {
            Ok(mut transform) => {
                let base = *fall.from_y.get_or_insert(transform.translation.y);
                transform.translation.y = base - fall_offset(fall.elapsed, fall.drop);
            }
            // Removed meanwhile (e.g. by the hammer): nothing left to animate.
            Err(_) => fall.elapsed = fall.duration,
        }
    }
    animations.prune_finished();
}

#[cfg(test)]
mod tests;
