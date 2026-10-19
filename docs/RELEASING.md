# Running and releasing the game

## Folder layout

The game expects two folders **next to the executable**:

```
oopsmath/
  oopsmath(.exe)
  assets/      models and other assets (e.g. log.glb)
  stages/      compiled stage packages (*.dat)
```

* **Assets**: Bevy loads them from `assets/` next to the executable
  (`crates/oopsmath-game/src/main.rs`).
* **Stages**: the stage browser scans `stages/` next to the executable
  (`stage::catalog::default_stages_dir`). If the folder is missing the browser
  shows "stage directory does not exist; a packaged game needs a `stages` folder
  next to the executable" together with the expected path.

Resolution order for the stage folder:

1. `OOPSMATH_STAGES_DIR` (environment variable) if set;
2. development runs (`cargo run`, `cargo test`): the repository's `stages/`;
3. `stages/` next to the executable.

The old fallbacks (`levels/`, the current working directory, and a path baked in at
build time) are gone, so a release never depends on the machine it was built on.

Build stage files with the stage compiler (`tools/oopsmath-stage-compiler`, e.g.
`python omsc.py build tools/oopsmath-stage-compiler/examples/001_first_wall -o stages`)
and commit the resulting `.dat` files in `stages/`.

## Publishing a release (GitHub Actions)

`.github/workflows/release.yml`:

1. Push a version tag: `git tag v0.1.0 && git push origin v0.1.0`.
2. The workflow runs `cargo test --workspace`, then builds `oopsmath` in release
   mode on Windows, Linux and macOS (Apple silicon), checks that `assets/` and
   `stages/*.dat` are in the repository, and packages each platform as
   `oopsmath-<tag>-<platform>.zip` containing the executable, `assets/`, `stages/`
   and `LICENSE`.
3. It creates the GitHub Release for the tag, attaches the three zips and generates
   release notes.

Run it by hand (Actions -> Release -> Run workflow) to get the zips as workflow
artifacts without publishing.

Requirements: `assets/` and `stages/` (with at least one `.dat`) committed;
`Cargo.lock` committed (the build uses `--locked`); repository setting
*Settings -> Actions -> General -> Workflow permissions* must allow the workflow to
create releases (the workflow requests `contents: write` for the publish job only).

Notes: Linux builds need the usual Bevy system libraries (installed by the
workflow). The executables are not code-signed, so Windows SmartScreen and macOS
Gatekeeper may warn on first launch.
