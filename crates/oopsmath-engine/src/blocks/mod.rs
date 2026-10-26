//! Blocks subsystem: block definitions, a central registry, grid math, model
//! loading, spawning, and an opt-in development preview.
//!
//! * [`BlocksPlugin`] - registry only; usable in headless tests and any scene.
//! * [`BlockAssetsPlugin`] - loads each registered model once (needs `AssetPlugin`).
//! * [`BlockPreviewPlugin`] - opt-in preview scene (`OOPSMATH_BLOCK_PREVIEW=1`).
//!
//! Not in scope: inventory/purchasing, structural simulation, an editor.

pub mod assets;
pub mod camera;
pub mod debug_scene;
pub mod definition;
pub mod error;
pub mod grid;
pub mod interaction;
pub mod look;
pub mod models;
pub mod occupancy;
pub mod placement;
pub mod registry;
pub mod rendering;
pub mod selection;
pub mod stage_rules;
pub mod targeting;

#[cfg(test)]
mod tests;

use bevy::prelude::*;

pub use assets::{BlockAssetState, BlockAssets, BlockAssetsPlugin};
pub use debug_scene::BlockPreviewPlugin;
pub use definition::{BlockDefinition, BlockId};
pub use error::{BlockAssetError, BlockError, BlockSpawnError};
pub use grid::{GridCoord, GridRotation, GridSpec};
pub use interaction::{
    BlockPlacementPlugin, HammerTarget, PlacementFeedback, PlacementRotation, PlacementTarget,
};
pub use occupancy::{PlacedWorld, PlacementRejection};
pub use placement::PlacedBlock;
pub use registry::BlockRegistry;
pub use rendering::spawn_block;
pub use selection::SelectedBlock;

/// Builds the [`BlockRegistry`] with the built-in blocks during `build`, so it
/// exists before any system (including `Startup`) runs.
///
/// A failing built-in registration is a programming error: it is logged with
/// `error!` and the registry keeps whatever was registered before the failure.
pub struct BlocksPlugin;

impl Plugin for BlocksPlugin {
    fn build(&self, app: &mut App) {
        let mut registry = BlockRegistry::new();
        if let Err(err) = models::register_builtin_blocks(&mut registry) {
            error!("Built-in block registration failed: {err}");
        }
        app.insert_resource(registry)
            .init_resource::<SelectedBlock>();
    }
}
