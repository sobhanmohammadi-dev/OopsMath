# Environment and lighting

Code: `crates/oopsmath-engine/src/scene/mod.rs` (`build_stage_scene`,
`EnvironmentRig`, `follow_outer_ground`). Visual only: nothing here touches the
grid, occupancy, placement, inventory, objectives or stability, and the ground
meshes have no collision and cast no shadows.

## Ground

* **Build area** - a plane exactly the size of `world.bounds`, unchanged in
  position and colour (`meadow`: 0.30 / 0.52 / 0.25).
* **Meadow** - a second, slightly darker plane (0.25 / 0.45 / 0.22) 2 cm below
  it. It is centred under the *camera target* and follows it every frame
  (`follow_outer_ground`). The orbit camera has no pan limit, so a fixed-size
  ground could always be panned away from; a following plane cannot.
  A flat colour makes the movement invisible.

## Fog and sky

`DistanceFog` (linear) in the sky colour, so the far ground fades into the
clear colour and no horizon line is visible. Distances come from
`EnvironmentRig::new(stage_size, orbit.max_distance)`:

| value | rule | 16x8x16 stage |
|---|---|---|
| `fog_start` | max zoom distance + stage diagonal (capped at 0.75 * `fog_end`) | 120 |
| `fog_end` | start + one zoom range, capped at 900 (camera far plane is 1000) | 216 |
| `ground_half` | `fog_end` + max zoom distance | 312 (624 m square) |

The camera is at most `max_distance` from its target and the meadow follows the
target, so the nearest meadow edge is always at least `fog_end` away, where the
fog already equals the sky. Fog is not applied to gizmos (grid, outlines,
blueprint guide, placement outlines).

## Lighting

Kept: one directional light with shadows, one ambient light. Changed: the sun
is a warm white (1.0 / 0.95 / 0.86) and the ambient fill is sky-tinted
(0.80 / 0.88 / 1.0, brightness 450). Illuminance (10 000) and the sun direction
are unchanged. Values were reviewed in code only.

## Not included

No props, skybox, sky gradient, post-processing or shader. Presets other than
`meadow` keep their dark neutral palette (now with a slightly darker meadow).
