use bevy::prelude::*;
use oopsmath_engine::OopsMathEnginePlugin;

/// Asset directory, relative to Bevy's base path.
///
/// Under `cargo run` the base path is this crate's manifest dir, while the
/// shared assets live in the workspace-level `assets/` folder. A deployed build
/// (no `CARGO_MANIFEST_DIR`) or an explicit `BEVY_ASSET_ROOT` uses `assets/`.
fn asset_dir() -> String {
    let cargo_run = std::env::var_os("CARGO_MANIFEST_DIR").is_some()
        && std::env::var_os("BEVY_ASSET_ROOT").is_none();
    if cargo_run {
        "../../assets".to_string()
    } else {
        "assets".to_string()
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            file_path: asset_dir(),
            ..default()
        }))
        .add_plugins(OopsMathEnginePlugin)
        .run();
}
