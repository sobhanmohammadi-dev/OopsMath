# Structural stability and collapse (MVP)

Code: `structure/mod.rs` (rule, landing, animation, systems) and
`structure/tests.rs`. Hooks: `blocks/interaction.rs` (placement check),
`blocks/occupancy.rs` (`PlacedWorld::shift_down`).

## The rule

* The **ground** (grid floor layer, `y == floor_y`) is the only foundation.
* A placed block is one structural object however many cells it covers (rotated
  and multi-cell blocks use their real cells).
* Blocks are connected through shared **faces**; diagonal and edge contact do not
  count.
* A block is **supported** if it touches the ground or is connected, through other
  blocks, to one that does. Everything else is **unsupported**.

Connectivity only: there is no weight, strength, overhang limit or physics.

## Placement

A block can only be placed (and the preview only turns green) where it would be
supported: on the ground or face-to-face with another block
(`check_placement_support`, read-only, rotated footprint). Otherwise: "No
support: build on the ground or against another block." and nothing is consumed.

## Collapse: blocks land, they are never deleted

Whenever `PlacedWorld` changes (placement, hammer removal, a landing), `settle`
looks for unsupported blocks:

1. Touching unsupported blocks form one **rigid group** (a column stays a column).
2. The group falls straight down by the smallest distance any member has before
   meeting the ground or a block outside the group, and **stays at its new cells**
   (`PlacedWorld::shift_down`: atomic, keeps shape and rotation, refuses overlaps).
3. Groups are processed lowest first and the search repeats until everything is
   supported, so a group lands on what the groups below it have become.

The *logical* move is immediate, so cells, records, placement limits and objective
counts are always consistent. The entity then plays a short fall animation
(`FallAnimations`, `sqrt(2 * drop / 12)` s, at least 0.25 s; purely visual) to its new
position. Nothing is despawned, refunded or charged; objectives, math challenges,
inventory and money are untouched. The player sees "The structure collapsed: N
block(s) fell." Entering or leaving a stage clears the animations.

Example (the reported case): stack two blocks, hammer the bottom one: the top block
falls one cell and now rests on the ground.

## Known MVP limits / ideas for later

* A block placed into the cells a falling block just left (during its ~0.3 s
  animation) is allowed; the animation does not wait.
* Landing never breaks anything and nothing is damaged by height.
* Ideas: overhang limit (a reach budget in `unsupported_blocks`), per-block roles
  (anchored / decorative), stage switches (`construction.constraints` exists in
  the schema but is not read yet), blocks that break on hard landings.
