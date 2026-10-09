//! The decoded voxel world carried by a stage package.

use std::collections::HashMap;

/// A decoded voxel world: its size in voxels, the block-name palette and the
/// sparse voxel map.
///
/// Palette index 0 is air; air cells are not stored in [`VoxelWorld::voxels`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoxelWorld {
    /// World size in voxels along x, y and z.
    pub size: [u32; 3],
    /// Block names; `palette[index]` names the block with that palette index.
    pub palette: Vec<String>,
    /// Sparse map from voxel position to palette index (never 0).
    pub voxels: HashMap<[u32; 3], u16>,
}

impl VoxelWorld {
    /// Palette index of the block at `position`, or `None` for air and for
    /// positions outside the world.
    pub fn block_index(&self, position: [u32; 3]) -> Option<u16> {
        self.voxels.get(&position).copied()
    }

    /// Name of the block at `position`, or `None` for air and for positions
    /// outside the world.
    pub fn block_name(&self, position: [u32; 3]) -> Option<&str> {
        let index = self.block_index(position)?;
        self.palette.get(usize::from(index)).map(String::as_str)
    }

    /// Number of non-air voxels.
    pub fn solid_count(&self) -> usize {
        self.voxels.len()
    }
}
