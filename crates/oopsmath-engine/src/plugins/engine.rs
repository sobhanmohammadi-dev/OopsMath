use bevy::prelude::*;

use crate::{
    app::{
        boot::boot_system,
        loading::load_selected_stage,
        stages::{LoadedStage, StageCatalogResource, StageLoadFailure, StageSelection},
        states::AppState,
    },
    blocks::{BlockAssetsPlugin, BlockPreviewPlugin, BlocksPlugin},
    scene::StageScenePlugin,
    stage::localization::StageLocale,
    ui::OopsMathUiPlugin,
};

pub struct OopsMathEnginePlugin;

impl Plugin for OopsMathEnginePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_resource::<StageCatalogResource>()
            .init_resource::<StageSelection>()
            .init_resource::<StageLoadFailure>()
            .init_resource::<LoadedStage>()
            .init_resource::<StageLocale>()
            .add_plugins(OopsMathUiPlugin)
            .add_plugins((BlocksPlugin, BlockAssetsPlugin, BlockPreviewPlugin))
            .add_plugins(StageScenePlugin)
            .add_systems(OnEnter(AppState::Boot), boot_system)
            .add_systems(OnEnter(AppState::LoadingStage), load_selected_stage);
    }
}
