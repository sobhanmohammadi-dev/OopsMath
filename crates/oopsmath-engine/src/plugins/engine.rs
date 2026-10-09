use bevy::prelude::*;

use crate::{
    app::{
        boot::boot_system,
        loading::{LoadedStage, SelectedStage, load_test_stage},
        states::AppState,
    },
    ui::OopsMathUiPlugin,
};

pub struct OopsMathEnginePlugin;

impl Plugin for OopsMathEnginePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_resource::<SelectedStage>()
            .init_resource::<LoadedStage>()
            .add_plugins(OopsMathUiPlugin)
            .add_systems(OnEnter(AppState::Boot), boot_system)
            .add_systems(OnEnter(AppState::LoadingStage), load_test_stage);
    }
}
