pub mod components;
pub mod hud;
pub mod menu;
pub mod theme;

use bevy::prelude::*;

use crate::app::states::AppState;

pub struct OopsMathUiPlugin;

impl Plugin for OopsMathUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<theme::UiTheme>()
            .add_systems(
                OnEnter(AppState::MainMenu),
                menu::main_menu::spawn,
            )
            .add_systems(
                Update,
                menu::main_menu::handle_play_button
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                hud::gameplay::spawn,
            );
    }
}