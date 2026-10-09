use bevy::prelude::*;
use oopsmath_engine::OopsMathEnginePlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(OopsMathEnginePlugin)
        .run();
}
