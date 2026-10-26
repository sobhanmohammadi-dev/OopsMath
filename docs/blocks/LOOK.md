# Block material and placement ghost

Code: `crates/oopsmath-engine/src/blocks/look.rs` (`BlockLookPlugin`, registered
by `BlockPlacementPlugin`).

## Why a code-side material

`assets/log.glb` has two materials with no base colour, no textures and no
vertex colours, so glTF defaults render it white. The asset is not edited: three
shared `StandardMaterial`s (`BlockLook`) are created once at startup and their
*handles* are assigned to the mesh primitives of spawned block scenes in
`PostUpdate` (after the scene spawner, before rendering), so a block is never
drawn white for a frame.

| Role | Detected by | Material |
|---|---|---|
| Placed block | scene root has `PlacedBlock` | `wood`: base colour (0.60, 0.40, 0.22), roughness 0.85, low reflectance, opaque, double sided like the original |
| Placement ghost | scene root has `GhostVisual` (added in `sync_preview`) | `ghost_valid` (green, alpha 0.45) or `ghost_invalid` (red, alpha 0.45), `AlphaMode::Blend`, no shadow casting |

Every primitive of the model is treated the same way (all material slots).
This is a flat colour, not wood grain.

## Isolation

* Only handles on mesh entities are swapped. No material asset is mutated, so
  re-tinting the ghost cannot change a placed block, and the imported glTF
  materials stay as loaded (the development preview scene keeps its look).
* The ghost uses its own materials and the `GhostPart` marker; `retint_ghost`
  only touches `GhostPart` meshes, when the `PlacementTarget` changes. The
  preview is not respawned to change colour.
* The systems read `PlacementTarget` (verdict) only. They do not access the
  grid (`PlacedWorld`), inventory, wallet, objectives or stability, and the
  ghost has no `PlacedBlock`.
* Position, rotation and footprint of the preview are unchanged (`sync_preview`).
  The green/red wireframe outline is unchanged.

## Limitations

* A flat colour: no grain, no texture. A textured model is future asset work.
* The ghost is alpha-blended; if the model's two primitives overlap internally,
  overlapping areas may look slightly darker. Needs an in-game check.
* The ghost does not cast shadows but can receive them.
