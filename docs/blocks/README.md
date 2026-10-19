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

## Stage-aware HUD and block selection

The gameplay HUD (`ui/hud/gameplay.rs`, data mapping in `ui/hud/model.rs`)
shows the loaded stage's localized title and description and the blocks listed
in `construction.allowed_blocks`. Each row shows the registry display name and
price (stage `cost` if present, otherwise the default) plus `min_count` /
`max_count` only when the stage declares them.

* Clicking a row stores the id in the `SelectedBlock` resource
  (`blocks/selection.rs`). Only blocks that are both listed by the stage **and**
  registered are selectable; the selected row is marked with `>`.
* Stage ids missing from the registry (the sample stage lists `brick`) are shown
  as "unavailable: not a registered block". The stage loader does not validate
  block ids, so the HUD is where the mismatch is reported. A malformed list shows
  an "invalid" message; an empty list shows "no selectable blocks".
* When the loaded stage changes, a selection the new stage does not allow is
  cleared. Nothing is auto-selected.
* Not implemented: placement, previews, inventory, purchasing, balance,
  objective progress. Tools (`allowed_tools`, e.g. `hammer`) are not displayed.

## Grid block placement (in game)

Select a block in the HUD, aim with the mouse, **left-click** to place it.
Camera: right-drag orbit, middle-drag pan, wheel zoom (unchanged).

* **Targeting** (`blocks/targeting.rs`): the cursor ray is intersected with the
  stage floor (cell layer y = 0, origin = stage `world.bounds.origin`, 1 m cells,
  Y up) and with the boxes of occupied cells; the nearest hit wins. A floor hit
  targets the cell it lands in (`GridSpec::world_to_cell`, floor); a hit on a
  block targets the neighbouring cell on the hit face, so blocks stack/extend.
  The conversion for preview, validation and placement is the same
  (`StageScene::grid_spec`).
* **Rules** (`blocks/occupancy.rs::evaluate_placement`, pure): something
  selected -> allowed by the stage (HUD allow-list) -> registered -> inside the
  bounds and rotation allowed -> every footprint cell free. Each refusal has a
  `PlacementRejection` with a player-facing message (shown bottom-left).
* **State**: `PlacedWorld` is the authoritative occupancy and records
  (`BlockId`, cell, rotation, entity). Footprints larger than 1 cell occupy all
  their cells (rotation is always `Deg0` here; there is no rotation control).
* **Preview**: one ghost model entity for the selected block, moved and
  shown/hidden (respawned only when the selection changes), plus a green/red cell
  outline. It is not a `PlacedBlock` and is never in `PlacedWorld`.
* **Atomicity**: placement re-validates, spawns, then records; the record cannot
  fail after validation, and on any failure nothing is recorded.
* **Lifecycle**: occupancy, target and feedback reset on entering and leaving
  the game; placed and preview entities despawn with the game state. Clicks over
  HUD panels/buttons never place.
* Not implemented: removal, rotation, inventory/cost, undo, save/load. The ghost
  is not tinted (it looks like a placed block; the outline shows validity).
  Targeting scans occupied cells linearly, fine for small builds.
