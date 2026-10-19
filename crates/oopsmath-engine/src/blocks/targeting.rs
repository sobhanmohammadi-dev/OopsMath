//! Turns a camera ray into the grid cell the player is aiming at.
//!
//! Rules: the ray is tested against the floor plane of the grid (cell layer
//! y = 0) and against the boxes of occupied cells; the nearest hit wins. A
//! floor hit targets the cell it lands in; a hit on an occupied cell targets
//! the neighbouring cell on the face that was hit (so blocks can be stacked
//! and extended). Bounds are *not* checked here; `evaluate_placement` does that.
//! Conversion uses [`GridSpec::world_to_cell`] (floor) for the floor hit.

use bevy::prelude::*;

use super::grid::{GridCoord, GridSpec};

/// Entry distance and the axis/sign of the face entered, for a ray vs a box.
fn ray_box(origin: Vec3, dir: Vec3, lo: Vec3, hi: Vec3) -> Option<(f32, usize, f32)> {
    let mut t_enter = f32::NEG_INFINITY;
    let mut t_exit = f32::INFINITY;
    let mut axis = 0;
    let mut sign = 0.0;
    for a in 0..3 {
        let (o, d) = (origin[a], dir[a]);
        if d.abs() < 1e-8 {
            if o < lo[a] || o > hi[a] {
                return None;
            }
            continue;
        }
        let (t0, t1, face_sign) = {
            let ta = (lo[a] - o) / d;
            let tb = (hi[a] - o) / d;
            if ta < tb {
                (ta, tb, -1.0)
            } else {
                (tb, ta, 1.0)
            }
        };
        if t0 > t_enter {
            t_enter = t0;
            axis = a;
            sign = face_sign;
        }
        t_exit = t_exit.min(t1);
        if t_enter > t_exit {
            return None;
        }
    }
    (t_exit >= 0.0 && t_enter >= 0.0).then_some((t_enter, axis, sign))
}

/// The cell targeted by the ray, if any. `occupied` yields every occupied cell.
pub fn ray_cell_target(
    origin: Vec3,
    dir: Vec3,
    grid: &GridSpec,
    occupied: impl Iterator<Item = GridCoord>,
) -> Option<GridCoord> {
    let mut best: Option<(f32, GridCoord)> = None;

    if dir.y < -1e-6 {
        let t = (grid.origin.y - origin.y) / dir.y;
        if t >= 0.0 {
            let hit = origin + dir * t;
            let cell = grid.world_to_cell(hit);
            best = Some((t, GridCoord::new(cell.0.x, 0, cell.0.z)));
        }
    }

    for cell in occupied {
        let lo = grid.cell_min_corner(cell);
        let hi = lo + grid.cell_size;
        if let Some((t, axis, sign)) = ray_box(origin, dir, lo, hi)
            && best.is_none_or(|(best_t, _)| t < best_t)
        {
            let mut target = cell.0;
            target[axis] += sign as i32;
            best = Some((t, GridCoord(target)));
        }
    }
    best.map(|(_, cell)| cell)
}

/// The nearest occupied cell hit by the ray (the cell itself, not a
/// neighbour). The floor is ignored, so only placed blocks can be picked, and a
/// block behind a nearer one is never returned.
pub fn ray_pick_occupied(
    origin: Vec3,
    dir: Vec3,
    grid: &GridSpec,
    occupied: impl Iterator<Item = GridCoord>,
) -> Option<GridCoord> {
    let mut best: Option<(f32, GridCoord)> = None;
    for cell in occupied {
        let lo = grid.cell_min_corner(cell);
        let hi = lo + grid.cell_size;
        if let Some((t, _, _)) = ray_box(origin, dir, lo, hi)
            && best.is_none_or(|(best_t, _)| t < best_t)
        {
            best = Some((t, cell));
        }
    }
    best.map(|(_, cell)| cell)
}
