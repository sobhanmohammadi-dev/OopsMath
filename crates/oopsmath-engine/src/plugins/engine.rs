use bevy::prelude::*;

use crate::{
    app::{
        boot::boot_system,
        loading::load_test_stage,
        states::AppState,
    },
    ui::OopsMathUiPlugin,
};

pub struct OopsMathEnginePlugin;

impl Plugin for OopsMathEnginePlugin {
    fn build(&self, app: &mut App) {
        app
            .init_state::<AppState>()
            .add_plugins(OopsMathUiPlugin)
            .add_systems(
                OnEnter(AppState::Boot),
                boot_system,
            )
            .add_systems(
                OnEnter(AppState::LoadingStage),
                load_test_stage,
            );
    }
}