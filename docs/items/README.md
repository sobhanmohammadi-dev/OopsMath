# Items and inventory (foundation)

Code: `crates/oopsmath-engine/src/items/` and `ui/hud/hotbar.rs`.

## Model

* `ItemRef::Block(BlockId)` / `ItemRef::Tool(ToolId)`. Blocks and tools are
  separate id namespaces (they mirror the stage's `blocks` / `tools` maps), so a
  tool can never be resolved as a placeable block (`ItemRef::resolve_block`).
* Block data stays in `BlockRegistry` (model, dimensions, price...). Nothing is
  duplicated in the inventory.
* `ToolRegistry` / `ToolDefinition { id, display_name, kind }`. `ToolKind` is the
  extension point; only `Hammer` exists and **using it is not implemented**.
  Tool ids use the stage convention (`hammer`); block ids still need a namespace
  (`block.wood_log`).

## Inventory rules (`items/inventory.rs`)

`u32` counts; adding/removing 0 is an error; overflow and underflow are errors
and change nothing; an entry reaching zero is removed (zero = absent); iteration
is sorted. The inventory holds ids and counts only and is not consumed by
placement yet.

## Starting resources (no schema change)

The existing stage schema already has `construction.starting_inventory.blocks`,
`.tools` (id -> integer >= 0), `construction.allowed_tools` and
`economy.budget`; the DAT format and compiler are untouched.

* On entering the game the inventory is rebuilt from the loaded stage
  (`StartingResources::parse` -> `Inventory::from_starting`). Zero entries are
  ignored.
* Legacy stages without `starting_inventory` get an **empty** inventory; nothing
  is granted implicitly.
* `economy.budget` (whole non-negative number) is parsed into
  `StageResources.starting.currency` for the future economy; there is no balance
  and the HUD does not show it.
* Invalid data leaves the inventory empty, logs an error and shows the reason in
  the INVENTORY panel.

## Selection

`SelectedItem` is the single selected item; `SelectedBlock` (used by placement)
is derived from it. Rules (`ui/hud/hotbar.rs`):

* Tool: allowed by the stage, registered, and in the inventory.
* Block: allowed by the stage and registered. Inventory presence is **not**
  required yet (transitional: placement does not consume, and the sample stage
  grants no blocks). Tighten this in the inventory-connected-building milestone.
* Stage change / inventory change: a no-longer-valid selection is cleared;
  nothing is auto-selected. Selecting a tool clears `SelectedBlock`, and left
  click never places while a tool is selected.

## HUD

BLOCKS panel (allowed blocks, unchanged), INVENTORY panel (rows
`[Block|Tool] Name xQty`, selected marked `>`, unavailable entries explained,
"Inventory is empty." state) and a `Selected:` line. All values come from the
resources above.

## Roadmap

1. Inventory-connected building: after a committed placement call
   `Inventory::remove`; require inventory for blocks in `is_item_selectable`.
2. Rotation: add a rotation to the placement target; it is already part of
   `PlacedBlock`/`evaluate_placement`.
3. Hammer: a tool-use system matching `ToolKind::Hammer`, targeting
   `PlacedWorld`, releasing cells; decide the refund policy first.
4. Economy/shop: a wallet resource seeded from `StartingResources.currency`;
   purchases that change wallet and inventory in one step.
