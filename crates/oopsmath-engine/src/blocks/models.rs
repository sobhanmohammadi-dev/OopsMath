//! Built-in block registrations. Adding a standard block means adding one
//! function here and one `registry.register(..)` line; no renderer, camera or
//! plugin code needs to change.

use bevy::math::{UVec3, Vec3};

use super::definition::{
    BlockDefinition, BlockDimensions, BlockEconomyConfig, BlockId, BlockModel, BlockPhysicsConfig,
    BlockVisualConfig, CollisionPolicy,
};
use super::error::BlockError;
use super::registry::BlockRegistry;

pub const WOOD_LOG_ID: &str = "block.wood_log";

pub fn wood_log_id() -> BlockId {
    BlockId::new(WOOD_LOG_ID)
}

/// Registers every built-in block, in a fixed order.
pub fn register_builtin_blocks(registry: &mut BlockRegistry) -> Result<(), BlockError> {
    registry.register(wood_log())?;
    Ok(())
}

/// Minecraft-style wooden log.
///
/// Measured from `assets/log.glb` (accessor min/max): the mesh spans about
/// 2.02 x 2.00 x 2.03 model units, centred on the origin. Scaling by 0.5 makes
/// it fill one 1 m cell (it overhangs by ~0.005 m on X/Z). The file has no
/// embedded images, only two untextured PBR materials, which are preserved.
fn wood_log() -> BlockDefinition {
    BlockDefinition::new(
        wood_log_id(),
        "Wood Log",
        BlockModel::glb("log.glb"),
        BlockDimensions::new(UVec3::new(1, 1, 2), Vec3::new(1.0, 1.0, 2.0)),
        BlockEconomyConfig::with_default_price(3),
    )
    .with_description("A solid wooden log for basic construction.")
    .with_visuals(BlockVisualConfig {
        scale: Vec3::new(0.5, 0.5, 1.0),
        ..Default::default()
    })
    .with_physics(BlockPhysicsConfig {
        mass_kg: 1200.0,
        collision: CollisionPolicy::FootprintBox,
        tags: vec!["wood".into(), "structural".into()],
    })
}
