# Blocks subsystem

`crates/oopsmath-engine/src/blocks/` defines **what a block is** (definitions +
registry), **where it can go** (grid + placement), and **how it is shown**
(GLB loading, spawning, an opt-in preview scene).

## Responsibilities

Does: stable block ids, typed definitions with validation, a deterministic
registry, grid math, one shared GLB scene handle per block, spawning placed
blocks, stage-rule *resolution* helpers, a development preview.

Does **not**: charge money, track inventory, check objectives, simulate
structures or collisions, edit stages, or touch the DAT format. Those are future
systems; `physics`/`economy` fields are data only.

## Files

| File | Responsibility |
|---|---|
| `mod.rs` | Exports and `BlocksPlugin` (registry only). |
| `definition.rs` | `BlockId`, `BlockDefinition` and its config structs, validation. |
| `registry.rs` | `BlockRegistry` resource: register, get/require, iterate. |
| `models.rs` | Built-in registrations (`block.wood_log`). |
| `grid.rs` | `GridCoord`, `GridRotation`, `GridSpec`, bounds, cell <-> world. |
| `placement.rs` | `PlacedBlock` component, `validate_placement`, transforms. |
| `stage_rules.rs` | `StageBlockRule`, price/count overrides, parsing `allowed_blocks`. |
| `assets.rs` | `BlockAssetsPlugin`, `BlockAssets` (load state per block). |
| `rendering.rs` | `spawn_block`. |
| `camera.rs` | `OrbitCamera` + system (RMB orbit, MMB pan, wheel zoom). |
| `debug_scene.rs` | `BlockPreviewPlugin`: preview camera, lights, grid, status text. |
| `error.rs` | `BlockError`, `BlockAssetError`, `BlockSpawnError`. |
| `tests.rs` | Unit tests. Integration tests: `crates/oopsmath-engine/tests/blocks_plugin.rs`. |

## Quick start

```rust
use oopsmath_engine::blocks::{models::wood_log_id, BlockRegistry};

fn show(registry: Res<BlockRegistry>) {
    let log = registry.require(&wood_log_id()).unwrap();
    info!("{} costs {} coins", log.display_name, log.economy.default_unit_price);
}
```

Plugins (all added by `OopsMathEnginePlugin`): `BlocksPlugin`,
`BlockAssetsPlugin` (needs `AssetPlugin`), `BlockPreviewPlugin` (inert unless
activated).

## Running the preview

```
# PowerShell
$env:OOPSMATH_BLOCK_PREVIEW = "1"; cargo run
```

On reaching the main menu the app switches to `AppState::BlockPreview`. You get
the real `log.glb` on a 16 x 8 x 16 grid (same bounds as stage 001).

| Input | Action |
|---|---|
| Right mouse drag | orbit |
| Middle mouse drag | pan |
| Mouse wheel | zoom (clamped) |
| Arrow keys | move the block one cell (stays inside the grid) |
| R | rotate a quarter turn |

Escape is not used. Close the window to exit. Without the variable the normal
menu flow is unchanged.

## Known limitations

* Grid blocks only; no non-grid parts or connectors.
* Stage rules are parsed/resolved but nothing enforces counts or charges prices.
* `CollisionPolicy`, `mass_kg` and `tags` are declarative; nothing consumes them.
* `MaterialPolicy` has one policy (preserve). No tint/override yet.
* Stage block ids (e.g. `brick` in stage 001) are not registered blocks yet.
* The preview needs a GPU/window; it was **not** run when this was written.
