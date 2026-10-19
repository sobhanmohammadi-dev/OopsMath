# Construction objectives (v1)

Code: `objectives/mod.rs` (model, parsing, evaluation, plugin) and
`ui/hud/objectives.rs` (HUD rows).

## Stage data (additive schema change in Compiler 1.1; DAT unchanged)

Objectives reuse the existing `objectives.tasks` list. A task of
`type: construction` whose free-form `data` object has a `kind` is a tracked
objective (`data` was free-form already; Compiler 1.1 added a typed shape for it to
the embedded schema and `stage.schema.yaml`, `schema_version` stays 1):

```yaml
objectives:
  tasks:
    - id: t_logs
      type: construction
      data:
        kind: place_block          # the only kind so far
        block: block.wood_log      # canonical block id
        count: 12                  # integer >= 1
        title: stage.my_stage.logs # optional localization key
```

* `title` falls back to the block's display name.
* Tasks without `data.kind` (math tasks, region-target construction tasks such as
  stage 001's `t_build`) are ignored by this system, so existing stages load
  unchanged. No tracked tasks = no objectives, and no completion message.
* Validation: the Python compiler (`validation/references.py`) checks kind,
  block id (registry), that the block is in `allowed_blocks`, `count >= 1` and
  unique task ids. The runtime re-checks (registered, allowed, duplicates,
  fields); an invalid list yields no objectives and a diagnostic in the HUD.
  Compiler tests: `tools/oopsmath-stage-compiler/tests/test_objectives_challenges.py`.

## Progress

Derived from `PlacedWorld` (the authoritative placed blocks): the number of
placed instances of the block, capped at `count`. Rotated multi-cell blocks count
once. Not counted: previews, inventory, purchases, grid helpers and stage-authored
content (none is placed in `PlacedWorld`). It is recomputed only when the placed
blocks or the objective list change.

## Completion

Progress follows the live count until the objective completes (removing a counted
block lowers it). Completion is **latched** for the stage session: it stays
complete (`count / count`) and is reported once (`evaluate` returns each index at
most once; logged with `info!`). The aggregate state (`ObjectiveTracker::all_complete`)
needs at least one objective, so an empty list is never "complete". Entering or
leaving a stage rebuilds/clears everything. No rewards, unlocks or stage
transitions yet.

## HUD

An OBJECTIVES block inside the top-left stage panel: `Wood Log: 7 / 12`, accent
colour and `(done)` when complete, and `All objectives complete!` when all are.
Hidden when the stage has none. Rows have no `Interaction`, so they never block
clicks.

## Adding another objective kind

Add a variant to `ObjectiveKind`, parse it in `StageObjectives::parse`, and handle
it in `Objective::required` / `live_progress` (and the compiler check).
