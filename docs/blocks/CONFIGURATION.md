# Configuration reference

**Units**: 1 world unit = 1 metre, Y up. Prices are whole coins (`u32`), the
same unit as stage `rewards.money` and `economy.budget`.

## `BlockDefinition`

| Field | Meaning |
|---|---|
| `id: BlockId` | Stable id, `[a-z][a-z0-9_]*` segments, >= 2 segments, e.g. `block.wood_log`. Persisted in stages: never rename. |
| `display_name`, `description` | Developer-facing text (no localization wiring yet). |
| `model: BlockModel` | `path` relative to the asset root (`"log.glb"`), `format` (`Glb`/`Gltf`), `scene_index` (default 0). |
| `dimensions: BlockDimensions` | `footprint_cells: UVec3` (logical, 1..=64) and `world_size: Vec3` (metres, must fit in the footprint). Independent of the GLB's own scale. |
| `grid: GridConfig` | `placement_mode` (`Grid`), `rotations` (`None` or `QuarterTurns`), `placement_offset` (world offset added to the footprint centre). |
| `economy: BlockEconomyConfig` | `default_unit_price` in coins. Global default. |
| `visuals: BlockVisualConfig` | `scale` (fits model to `world_size`), `model_offset` (block-local metres from footprint centre to model pivot), `material_policy` (`PreserveOriginal`). |
| `physics: BlockPhysicsConfig` | `mass_kg`, `collision` (`None`/`FootprintBox`), `tags`. Data only. |

## Choosing dimensions

* Footprint = how many cells it blocks. World size = how big it really is.
* Scale = `world_size / model_size`. `block.wood_log`: the model spans ~2.0
  units, so scale 0.5 gives 1 m.
* Model pivot should be the centre of the model's bounds; otherwise set
  `model_offset` to the vector from the footprint centre to the pivot.

## Grid conventions (`grid.rs`)

* `GridCoord` is integer; floats are never used for cell identity.
* A block's cell is the **minimum corner** of its footprint.
* Rotation is a yaw about +Y in quarter turns, counter-clockwise seen from
  above. Footprints rotate: 90/270 swap X and Z, anchored at the same minimum
  corner. Visual centre = rotated footprint centre.
* `world_to_cell` uses floor, so snapping is deterministic.
* `GridSpec { origin, cell_size, bounds }`; `bounds` is optional.
  `validate_placement` checks existence, allowed rotation and bounds.
* Registration checks `world_size` against the **default** 1 m cell; a stage
  with larger cells is not re-validated per definition.

## Stage overrides (`stage_rules.rs`)

Global defaults live in the definition. A stage may specify, per block:

```yaml
construction:
  allowed_blocks:
    - id: block.wood_log
      cost: 3        # optional: replaces default_unit_price
      min_count: 0
      max_count: 20
```

Precedence: `cost` if present, else `economy.default_unit_price`. Use:

```rust
let rules = parse_allowed_blocks(&construction_json)?;   // serde_json::Value
let price = rules[0].effective_price(rules[0].definition(&registry)?);
```

The existing schema already has `id`, `min_count`, `max_count`; `cost` is not in
the current compiler schema or fixtures, so no stage was changed. Adding
`cost` to the compiler is a separate, deliberate step. Nothing enforces these
rules at runtime yet.

## `block.wood_log` as shipped

id `block.wood_log`, model `log.glb`, footprint 1x1x1, world size 1x1x1 m,
scale 0.5, all four rotations, price 3 coins, mass 600 kg (estimate), tags
`wood`, `structural`.
