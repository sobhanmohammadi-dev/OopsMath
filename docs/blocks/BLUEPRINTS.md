# Blueprints (construction plans) v1

Code: `blueprint/mod.rs` (data, parsing, mode, guide drawing), `blueprint/plan.rs`
(pure plan calculation and progress), `ui/hud/blueprint.rs` (button and popup).
Compiler: `construction.blueprints` (Compiler 1.2.0).

## What the player gets

A **Plan** button (bottom-left, above Math Challenge; hidden when the stage has no
blueprints) opens a popup with, for the selected blueprint (`<` `>` to switch):

* title, **size** (width x height x depth in cells) and number of blocks;
* **materials** per block: needed, owned, still missing, unit price and line cost;
* **estimated cost** of buying everything at shop prices, and the cost of what is
  not owned yet;
* issues: "Cannot build: ..." (stage restrictions) and "Note: ..." (e.g. no price);
* **Start building (show guide)** / **Stop guide**, and Close (or Escape).

Starting the guide closes the popup and draws the plan on the grid; the button then
shows `Plan: 3 / 8 built`. **Nothing is placed, charged or consumed by the plan**:
you build block by block through the normal placement rules (inventory, stage
limits, rotation, stability), and the shop is used as usual.

## Data

```yaml
construction:
  blueprints:
    - id: bp_wall_a
      title: stage.001_first_wall.blueprint.wall   # optional localization key
      blocks:
        - {block: block.wood_log, position: [0, 0, 0]}
        - {block: block.wood_log, position: [1, 0, 0], rotation: 90}
```

* `position`: the block's **anchor cell**, the cell you aim at when placing it
  (relative to the world origin). `rotation`: yaw in degrees, 0/90/180/270, the same
  quarter turns and anchor rule as normal placement (see `STRUCTURE.md`/rotation).
* No block data is stored: footprints, rotation rules, names and prices come from the
  block registry; permissions and limits from `allowed_blocks`.
* Compatibility: additive; no DAT or schema-version change; stages without
  `blueprints` are unaffected. Compiler checks and game checks are listed in the
  compiler README; the game additionally rejects overlapping footprints, positions or
  rotations the block cannot use and anything outside the stage grid. An invalid list
  shows its reason in the popup and the guide is unavailable.

## Calculations (`Plan::compute`, read-only)

* **Geometry**: bounds of all real occupied cells (rotated footprints).
* **Materials**: counts per block id (a multi-cell block counts once).
* **Cost**: `unit price x quantity` with checked arithmetic. A price of 0 means "no
  valid price" (as in the shop): the cost shows as unavailable with a note, never an
  invented number. Overflow is reported ("too large to calculate").
* **Restrictions**: block not allowed in the stage, or needing more than the stage's
  `max_count`, blocks the plan ("cannot be built in this stage"). Missing materials
  do not block: you can buy them.

## Progress

A planned block counts as **built** when `PlacedWorld` holds a block with the same id,
the same anchor cell and the same rotation (`plan_statuses`). It is evaluated live:
removing a block, or a collapse moving it, un-builds it. Cells used by anything else
show as obstructed. Previews are never consulted. Objectives are unchanged; a plan
does not complete anything.

## Preview

Wireframe boxes (footprint of the rotated block) with a small arrow for the block's
forward direction, drawn with gizmos: cyan = to build, green = built, orange =
obstructed. They are **not entities**, so they cannot occupy the grid, count for
objectives, take part in stability or outlive the stage; there is nothing to rebuild per
frame. Limitation: block models are not ghosted.

## Not in v1

Automatic building, a blueprint editor, ghost models, blueprints that depend on
objectives, per-blueprint prices.
