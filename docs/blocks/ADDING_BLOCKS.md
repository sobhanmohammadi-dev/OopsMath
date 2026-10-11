# Adding a block

1. **Put the model in `assets/`** (repository root), e.g. `assets/stone.glb`.
   Prefer GLB with embedded textures. See `ASSETS_AND_RENDERING.md`.
2. **Measure it.** Note the model's size in model units (Blender dimensions, or
   the POSITION accessor `min`/`max` in the GLB JSON) and where its pivot is.
3. **Choose dimensions.** Footprint in cells, size in metres (must fit in the
   footprint at 1 m cells), and a visual scale so the model matches that size.
4. **Add a function in `blocks/models.rs`** and register it.
5. **Preview it**: change `BlockPreview::default().block` (or set the resource)
   to your id and run with `OOPSMATH_BLOCK_PREVIEW=1`.
6. **Add a test** that registers it and, if useful, compares the GLB bounds
   (see `log_glb_matches_declared_dimensions` in `blocks/tests.rs`).

## Example (documentation only: `stone.glb` is not in the repository)

```rust
// blocks/models.rs
pub const STONE_ID: &str = "block.stone";

pub fn register_builtin_blocks(registry: &mut BlockRegistry) -> Result<(), BlockError> {
    registry.register(wood_log())?;
    registry.register(stone())?;      // <- new line
    Ok(())
}

fn stone() -> BlockDefinition {
    BlockDefinition::new(
        BlockId::new(STONE_ID),
        "Stone",
        BlockModel::glb("stone.glb"),
        BlockDimensions::new(UVec3::ONE, Vec3::ONE),
        BlockEconomyConfig::with_default_price(8),
    )
    .with_visuals(BlockVisualConfig::default().with_uniform_scale(1.0))
    .with_grid(GridConfig::default_block().with_rotations(AllowedRotations::None))
    .with_physics(BlockPhysicsConfig {
        mass_kg: 2400.0,
        collision: CollisionPolicy::FootprintBox,
        tags: vec!["stone".into(), "structural".into()],
    })
}
```

No renderer, camera or plugin change is needed: `BlockAssetsPlugin` loads a
scene for every registered definition.

## Variants without copying

```rust
let cheap = stone().variant(BlockId::new("block.stone_cheap"), "Cheap Stone")
    .with_economy(BlockEconomyConfig::with_default_price(5));
registry.register(cheap)?;
```

A variant shares the model (and so the loaded handle's file, though each id
gets its own handle entry) and changes only what you override.

## Rules enforced at registration

Unique, well-formed id; non-empty name; relative model path with the right
extension; footprint 1..=64 per axis; finite positive world size that fits the
footprint; finite positive scale; price <= 1,000,000; finite non-negative mass;
valid tags. Violations return `BlockError`.
