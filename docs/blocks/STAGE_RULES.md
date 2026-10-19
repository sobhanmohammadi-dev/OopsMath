# Stage block rules and inventory-connected placement

No schema or DAT change: the existing `construction.allowed_blocks` entries and
`construction.starting_inventory` are used.

```yaml
construction:
  allowed_blocks:
    - {id: block.wood_log, max_count: 15}   # allowed, at most 15 placed at once
  allowed_tools:
    - {id: hammer}
  starting_inventory:
    blocks: {block.wood_log: 10}            # what the player owns at the start
    tools: {hammer: 1}
```

* **Allowlist**: a block can be placed only if it has an `allowed_blocks`
  entry. No entries (incl. legacy stages) means nothing is placeable; absence is
  never "unrestricted". Ids must be registered block ids (`block.wood_log`).
* **`max_count`** (existing field) = maximum number of placed **instances** of
  that block alive at once (not cells, not the starting inventory). Omitted = no
  instance cap. Negative / non-integer values make the whole list invalid:
  nothing is placeable and the HUD shows the diagnostic.
* **`starting_inventory.blocks`** = the quantity owned. It is separate from
  `max_count`. A stage that allows a block but grants none cannot place it.
* **Placement** (`occupancy::evaluate_commit`, shared by the preview and the
  click): selected block -> allowed -> registered -> bounds/rotation/occupancy
  -> instance limit -> at least one in inventory. On success one unit is consumed
  together with the placed-block record (`commit_placement`); any failure
  changes neither. Selecting a tool never places.
* **HUD**: the selection panel shows `In inventory: N` and `Placed: X / max`;
  rejections appear as feedback text. A block with none left is deselected.

Migration: stages created before this change that list blocks but no
`starting_inventory.blocks` must add it (the sample `001_first_wall` lists
`brick`, which is not a registered block, so it is unplaceable by design).
Recompile with the stage compiler after editing the YAML.

## Rotation

Press **R** in game to turn the selected block a quarter turn (0, 90, 180, 270
degrees, counter-clockwise seen from above; wraps after four). The preview,
the green/red outline, validation and the placed block all use that orientation
(`PlacementTarget.rotation`).

* **Capability**: `GridConfig.rotations` (`AllowedRotations::QuarterTurns` is the
  default, so existing blocks such as `block.wood_log` rotate;
  `AllowedRotations::None` makes a block fixed at 0). No stage or DAT field.
* **Footprint and anchor**: the unrotated footprint extends from the aimed
  (anchor) cell towards +X/+Y/+Z. Rotation turns it **about the anchor cell**, so
  the aimed cell is always occupied and a 2-cell block points +X, -Z, -X, +Z for
  0/90/180/270. Odd turns swap the X and Z cell counts; height is unchanged. The
  model is rotated about the rotated footprint's centre, so it fills exactly the
  cells it occupies (`footprint_box`, `rotated_cells`, `GridSpec::footprint_center`
  in `grid.rs`).
* Rotation is only a request (`PlacementRotation`); it is resolved against the
  selected block's capability, and R does nothing with no block or a tool
  selected. It never changes occupancy or inventory. There are no text fields
  in game, so R is read whenever the game state is active.

## Hammer

Select the hammer (inventory panel), then left-click a placed block to remove it.

* **Targeting**: the cursor ray is tested against the cells of placed blocks
  only (`ray_pick_occupied`); the nearest hit wins, so an occluded block is never
  picked. The floor, grid helpers, the preview ghost and HUD are not candidates,
  and over the HUD there is no target. The target is an entity that is checked
  against `PlacedWorld` again at click time. A miss does nothing.
* **Removal** (`occupancy::remove_block`): releases exactly the cells recorded
  for that block (its real, rotated footprint) and nothing owned by another
  block; the placed-instance count (derived from the records) drops, so the
  stage limit frees a slot. The entity is despawned after the logical removal.
* **Refund policy**: exactly one unit of the removed block returns to the
  inventory, only when a recorded placed block is removed. A repeated click, a
  preview or any non-block entity gives no refund. If the refund cannot be stored
  (quantity at the `u32` maximum) nothing is removed. Tools, currency and the
  budget are never refunded.
* **One click, one action**: `click_action` maps the selection to placement or
  hammer, never both. Other tools do nothing on click.
* Stage exit clears `PlacedWorld`, the target and despawns the blocks.
* Not implemented: damage, effects, removing stage-authored terrain.
