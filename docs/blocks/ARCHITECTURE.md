# Architecture

## Runtime flow

1. `OopsMathEnginePlugin` adds `BlocksPlugin`, `BlockAssetsPlugin`,
   `BlockPreviewPlugin`.
2. `BlocksPlugin::build` creates the registry, registers built-ins, inserts it
   as a resource. It therefore exists before any system runs.
3. `Startup`: `request_block_assets` loads one scene handle per definition.
4. `Update` (only while loading): `poll_block_assets` updates `BlockAssets`.
5. Gameplay or preview code calls `spawn_block`, which validates the placement,
   requires a ready model and spawns `PlacedBlock + Transform + WorldAssetRoot`.
6. Preview only: with `OOPSMATH_BLOCK_PREVIEW`, `OnEnter(MainMenu)` switches to
   `AppState::BlockPreview` (the only change to existing states: one new variant).

## Decisions

* **Definition vs instance**: `BlockDefinition` is immutable registry data;
  `PlacedBlock` holds `BlockId + GridCoord + GridRotation` only.
* **Logical vs visual size**: `dimensions` (cells, metres) are separate from
  `visuals` (scale/offset). Physical size is never inferred from model scale.
* **Defaults vs stage rules**: definition holds defaults; `StageBlockRule` holds
  per-stage values and a documented precedence.
* **Errors**: registry/placement errors (`BlockError`) are distinct from asset
  errors (`BlockAssetError`); neither panics at runtime.
* **Plugins split** so the registry works headless (tests, future scenes) with
  no asset or debug code.
* **No new dependencies**; `serde_json` (already present) is used for stage
  rule parsing.
* **Not touched**: DAT format, stage loader/compiler, menus, localization.

## Extension points (not implemented)

* Non-grid parts/connectors: add a new `PlacementMode` variant and a sibling to
  `PlacedBlock`; the registry, ids, assets and spawn path are reusable.
* Material overrides: add `MaterialPolicy` variants and apply them in
  `rendering.rs` after the scene spawns.
* Stage integration: add `cost` to the stage schema/compiler, call
  `parse_allowed_blocks` when a stage is loaded, enforce counts in construction
  gameplay.
* Collision/structure: consume `physics` in a future system.

## Testing

`cargo test -p oopsmath-engine` runs `blocks/tests.rs` (registry, validation,
grid, placement, stage rules, GLB bounds) and `tests/blocks_plugin.rs`
(headless plugin/spawn behaviour). Visual behaviour needs a manual run.
