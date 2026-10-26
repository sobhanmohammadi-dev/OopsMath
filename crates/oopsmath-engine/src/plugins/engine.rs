use bevy::prelude::*;

use crate::i18n::I18nPlugin;
use crate::{
    app::{
        boot::boot_system,
        loading::load_selected_stage,
        stages::{LoadedStage, StageCatalogResource, StageLoadFailure, StageSelection},
        states::AppState,
    },
    blocks::{BlockAssetsPlugin, BlockPlacementPlugin, BlockPreviewPlugin, BlocksPlugin},
    blueprint::BlueprintPlugin,
    challenges::ChallengesPlugin,
    items::ItemsPlugin,
    objectives::ObjectivesPlugin,
    scene::StageScenePlugin,
    stage::localization::StageLocale,
    structure::StructurePlugin,
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
            .add_plugins(I18nPlugin)
            .add_plugins(OopsMathUiPlugin)
            .add_plugins((BlocksPlugin, BlockAssetsPlugin, BlockPreviewPlugin))
            .add_plugins((
                StageScenePlugin,
                BlockPlacementPlugin,
                ItemsPlugin,
                ChallengesPlugin,
                ObjectivesPlugin,
                StructurePlugin,
                BlueprintPlugin,
            ))
            .add_systems(OnEnter(AppState::Boot), boot_system)
            .add_systems(OnEnter(AppState::LoadingStage), load_selected_stage);
    }
}
