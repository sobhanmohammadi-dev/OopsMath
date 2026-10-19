//! Integer grid coordinates, rotation rules and grid <-> world conversion.
//!
//! Conventions (also documented in `docs/blocks/CONFIGURATION.md`):
//! * Right-handed, Y up. One world unit is one metre.
//! * Logical identity is always an integer [`GridCoord`]; floats are only used
//!   for rendering and for converting a world position into a cell.
//! * A block's `origin` cell is the *minimum corner* of its footprint.
//! * Rotation is a yaw about +Y in quarter turns (counter-clockwise seen from
//!   above). A footprint rotates with the block: odd quarter turns swap X and Z.
//!   The rotated footprint stays anchored at the origin cell's minimum corner.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use super::error::BlockError;

/// Edge length of one grid cell in world units (metres) unless a [`GridSpec`]
/// says otherwise.
pub const DEFAULT_CELL_SIZE: f32 = 1.0;

/// Integer cell coordinate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GridCoord(pub IVec3);

impl GridCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self(IVec3::new(x, y, z))
    }
}

/// Discrete yaw rotation of a placed block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GridRotation {
    #[default]
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl GridRotation {
    pub const ALL: [GridRotation; 4] = [Self::Deg0, Self::Deg90, Self::Deg180, Self::Deg270];

    pub const fn quarter_turns(self) -> u32 {
        match self {
            Self::Deg0 => 0,
            Self::Deg90 => 1,
            Self::Deg180 => 2,
            Self::Deg270 => 3,
        }
    }

    pub const fn degrees(self) -> u32 {
        self.quarter_turns() * 90
    }

    /// The next rotation, counter-clockwise seen from above.
    pub const fn next(self) -> Self {
        match self {
            Self::Deg0 => Self::Deg90,
            Self::Deg90 => Self::Deg180,
            Self::Deg180 => Self::Deg270,
            Self::Deg270 => Self::Deg0,
        }
    }

    pub fn to_quat(self) -> Quat {
        Quat::from_rotation_y(self.quarter_turns() as f32 * FRAC_PI_2)
    }

    /// Rotates a horizontal cell offset (x, z) about the anchor cell. This is
    /// the same mapping as [`to_quat`](Self::to_quat) (+X goes to -Z for one
    /// quarter turn), so cells and the rotated model always agree.
    pub const fn rotate_offset(self, x: i32, z: i32) -> (i32, i32) {
        match self.quarter_turns() {
            0 => (x, z),
            1 => (z, -x),
            2 => (-x, -z),
            _ => (-z, x),
        }
    }

    /// Footprint after rotation: odd quarter turns swap X and Z.
    pub const fn rotate_footprint(self, footprint: UVec3) -> UVec3 {
        if self.quarter_turns() % 2 == 1 {
            UVec3::new(footprint.z, footprint.y, footprint.x)
        } else {
            footprint
        }
    }
}

/// Optional inclusive-exclusive cell range a grid is restricted to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridBounds {
    pub min: IVec3,
    pub size: UVec3,
}

impl GridBounds {
    /// True when the box `[cell, cell + size)` lies fully inside the bounds.
    pub fn contains_box(&self, cell: GridCoord, size: UVec3) -> bool {
        let lo = self.min;
        let hi = self.min + self.size.as_ivec3();
        let box_hi = cell.0 + size.as_ivec3();
        cell.0.cmpge(lo).all() && box_hi.cmple(hi).all()
    }
}

/// Describes one grid in the world. A stage (or the preview) owns one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridSpec {
    /// World position of the minimum corner of cell (0, 0, 0).
    pub origin: Vec3,
    /// World size of a cell per axis.
    pub cell_size: Vec3,
    pub bounds: Option<GridBounds>,
}

impl Default for GridSpec {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            cell_size: Vec3::splat(DEFAULT_CELL_SIZE),
            bounds: None,
        }
    }
}

impl GridSpec {
    pub fn new(
        origin: Vec3,
        cell_size: Vec3,
        bounds: Option<GridBounds>,
    ) -> Result<Self, BlockError> {
        if !origin.is_finite() {
            return Err(BlockError::InvalidGrid("origin must be finite".into()));
        }
        if !cell_size.is_finite() || cell_size.min_element() <= 0.0 {
            return Err(BlockError::InvalidGrid(
                "cell size must be finite and positive on every axis".into(),
            ));
        }
        if let Some(b) = bounds
            && b.size.min_element() == 0
        {
            return Err(BlockError::InvalidGrid(
                "bounds must be at least one cell on every axis".into(),
            ));
        }
        Ok(Self {
            origin,
            cell_size,
            bounds,
        })
    }

    /// World position of a cell's minimum corner.
    pub fn cell_min_corner(&self, cell: GridCoord) -> Vec3 {
        self.origin + cell.0.as_vec3() * self.cell_size
    }

    /// World position of a cell's centre.
    pub fn cell_center(&self, cell: GridCoord) -> Vec3 {
        self.box_center(cell, UVec3::ONE)
    }

    /// World position of the centre of a `size`-cell box whose minimum cell is `cell`.
    pub fn box_center(&self, cell: GridCoord, size: UVec3) -> Vec3 {
        self.cell_min_corner(cell) + size.as_vec3() * self.cell_size * 0.5
    }

    /// World centre of a footprint anchored at `anchor` with `rotation`: the
    /// anchor cell's centre plus the footprint's own centre offset, turned by
    /// the same mapping as the cells.
    pub fn footprint_center(&self, anchor: GridCoord, size: UVec3, rotation: GridRotation) -> Vec3 {
        let (lx, lz) = (size.x as f32 - 1.0, size.z as f32 - 1.0);
        let (rx, rz) = match rotation.quarter_turns() {
            0 => (lx, lz),
            1 => (lz, -lx),
            2 => (-lx, -lz),
            _ => (-lz, lx),
        };
        self.cell_min_corner(anchor)
            + self.cell_size * 0.5
            + Vec3::new(
                rx * 0.5 * self.cell_size.x,
                (size.y as f32 - 1.0) * 0.5 * self.cell_size.y,
                rz * 0.5 * self.cell_size.z,
            )
    }

    /// The cell containing a world position (floor semantics, deterministic).
    pub fn world_to_cell(&self, position: Vec3) -> GridCoord {
        GridCoord(
            ((position - self.origin) / self.cell_size)
                .floor()
                .as_ivec3(),
        )
    }

    /// Snaps a world position to the centre of the cell that contains it.
    pub fn snap_to_cell_center(&self, position: Vec3) -> Vec3 {
        self.cell_center(self.world_to_cell(position))
    }
}

/// Minimum cell and size of the box covered by `size` cells placed at
/// `anchor` with `rotation`.
///
/// Anchor convention: the unrotated footprint extends from the anchor cell
/// towards +X, +Y, +Z. Rotation turns it about the **anchor cell**, so the cell
/// the player aims at is always occupied and a multi-cell block points in four
/// different directions from it.
pub fn footprint_box(anchor: GridCoord, size: UVec3, rotation: GridRotation) -> (GridCoord, UVec3) {
    let (far_x, far_z) = rotation.rotate_offset(size.x as i32 - 1, size.z as i32 - 1);
    let min = IVec3::new(
        anchor.0.x + far_x.min(0),
        anchor.0.y,
        anchor.0.z + far_z.min(0),
    );
    (GridCoord(min), rotation.rotate_footprint(size))
}

/// Cells covered by a block anchored at `anchor` (see [`footprint_box`]).
pub fn rotated_cells(
    anchor: GridCoord,
    size: UVec3,
    rotation: GridRotation,
) -> impl Iterator<Item = GridCoord> {
    let (min, rotated) = footprint_box(anchor, size, rotation);
    occupied_cells(min, rotated)
}

/// All cells of the box `[origin, origin + size)`, in Y-then-Z-then-X order.
pub fn occupied_cells(origin: GridCoord, size: UVec3) -> impl Iterator<Item = GridCoord> {
    (0..size.y).flat_map(move |y| {
        (0..size.z).flat_map(move |z| {
            (0..size.x).map(move |x| GridCoord(origin.0 + UVec3::new(x, y, z).as_ivec3()))
        })
    })
}
