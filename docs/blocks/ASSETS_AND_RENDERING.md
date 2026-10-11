# Assets and rendering

## Asset root

The GLB lives at repository-level `assets/log.glb`, referenced as `"log.glb"`.
Bevy resolves assets relative to the *running crate's* manifest dir under
`cargo run`, so `crates/oopsmath-game/src/main.rs` sets
`AssetPlugin { file_path: "../../assets" }` in that case (and `"assets"` for a
deployed build or when `BEVY_ASSET_ROOT` is set). The DAT stage loader reads
files with `std::fs` and is unaffected.

## Loading

`BlockAssetsPlugin` runs once at `Startup`: for each definition it loads
`GltfAssetLabel::Scene(scene_index)` of the model path and stores the handle in
`BlockAssets`. `poll_block_assets` runs only while something is loading and
moves each entry to `Ready` or `Failed(reason)` (also logged with `error!`).
`spawn_block` requires `Ready` and spawns `WorldAssetRoot(handle.clone())`, so
all instances share one handle.

Bevy 0.19 renamed `Scene`/`SceneRoot` to `WorldAsset`/`WorldAssetRoot`
(`bevy::world_serialization`); the code uses the new names.

## What `assets/log.glb` contains (inspected)

* 1 scene, nodes: `Cube` (mesh with 2 primitives) and an empty
  `ReflectionCubemap` node.
* 2 PBR materials (`KHR_materials_specular`, `KHR_materials_ior`), **no images
  and no textures**. Both use the default white base colour, so the log renders
  as untextured shaded white/grey geometry. That is what the file contains;
  this module preserves it and does not invent textures. If a coloured/bark
  look is expected, re-export the model with its textures embedded.
* Mesh bounds about 2.02 x 2.00 x 2.03, centred on the origin.

## Preparing a new GLB

Export as GLB with textures embedded, +Y up, origin at the centre of the block,
metres or consistent units. Note the bounds and set `visuals.scale` so
`bounds * scale ~ world_size`. `.gltf` with external files works only if the
files sit beside it under `assets/`.

## Preview scene

`debug_scene.rs`: own `Camera3d` (order 1, `IsDefaultUiCamera`), a per-camera
`AmbientLight`, a shadow-casting key light and a fill light, a dark ground
plane, gizmo grid lines (one line per grid line, no per-cell entities), axes
(X red, Y green, Z blue), a yellow box for the selected footprint, and a status
panel showing model state. All entities use `DespawnOnExit(BlockPreview)`; the
existing `Camera2d` is untouched.

## Troubleshooting

| Symptom | Check |
|---|---|
| Status says `FAILED: ...` | Path/extension in the definition; file exists under `assets/`; run with `RUST_LOG=bevy_asset=debug`. |
| Status stays `loading...` | Asset root wrong; check log line "Asset Server using ... as its base path". |
| Nothing visible | Scale (model may be huge/tiny); pivot offset; zoom with the wheel. |
| Wrong size | Recompute `scale` from the GLB bounds. |
| `InvalidId` / `DuplicateId` | Id format; each id registered once. |
| Looks untextured | The GLB has no textures (see above). |
