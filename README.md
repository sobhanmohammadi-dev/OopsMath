# OopsMath

OopsMath is an educational voxel-style construction game. You solve math tasks
and build structures on a grid: choose blocks, place and rotate them, buy more
in the shop, follow the stage objectives and finish the stage. It is written in
Rust with the [Bevy](https://bevyengine.org/) engine (0.19) and a Python tool
that compiles stage source files into `.dat` packages.

## Requirements

* A Rust toolchain that supports edition 2024 (`rustup` stable).
* On Linux, the packages used by the CI workflow
  (`.github/workflows/release.yml`): `pkg-config libasound2-dev libudev-dev
  libwayland-dev libxkbcommon-dev libx11-dev libxcursor-dev libxi-dev
  libxrandr-dev`.
* Python 3 with `PyYAML jsonschema msgpack zstandard` only if you want to build
  stages (see below). `pytest` is needed to run the compiler tests.

## Run the game

From the repository root:

```
cargo run
```

`cargo run` starts the `oopsmath` game crate (the workspace default member).
During development it loads models from the repository's `assets/` folder and
stages from the repository's `stages/` folder. Choose **PLAY**, pick a stage in
the browser and press **Play Stage**.

A packaged game needs `assets/` and `stages/` next to the executable; see
[docs/RELEASING.md](docs/RELEASING.md). `OOPSMATH_STAGES_DIR` overrides the stage
folder.

## Controls

| Action | Control |
|---|---|
| Choose a block or tool | Click it in BLOCKS (top right) or INVENTORY (bottom right) |
| Place a block | Left click on the ground or a block face |
| Rotate the selected block | `R` |
| Remove a block | Select the Hammer in INVENTORY, left click a block |
| Camera | Right mouse drag = orbit, middle drag = pan, wheel = zoom |
| Shop / Math Challenge / Plan | Buttons at the bottom left (Math Challenge and Plan appear only when the stage has them) |
| Answer a math challenge | Type the answer, `Enter` or Submit |
| Menu, controls help, restart, back to stages | `Esc` or the "Menu (Esc)" button |
| Close an open window | `Esc` |

Blocks must touch the ground or another block; if their support is removed they
fall. When every objective is complete a "Stage complete" panel offers Replay
Stage, Back to Stages and Keep Building.

## Build stages

Stage sources live under `tools/oopsmath-stage-compiler` (see its README). For
example:

```
cd tools/oopsmath-stage-compiler
pip install PyYAML jsonschema msgpack zstandard pytest
python omsc.py validate examples/001_first_wall
python omsc.py build examples/001_first_wall -o ../../stages
```

## Tests

```
cargo fmt --check
cargo check --workspace
cargo test --workspace
cd tools/oopsmath-stage-compiler && python -m pytest -q tests && python omsc.py self-test
```

## Documentation

Feature notes are in [docs/blocks](docs/blocks/README.md) (blocks, shop,
objectives, math challenges, structure, blueprints, stage session) and
[docs/items](docs/items/README.md).

## License

Apache-2.0, see [LICENSE](LICENSE).
