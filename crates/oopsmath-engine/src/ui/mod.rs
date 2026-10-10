pub mod common;
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
            .add_systems(OnEnter(AppState::MainMenu), menu::main_menu::spawn)
            .add_systems(
                Update,
                menu::main_menu::handle_buttons.run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(OnEnter(AppState::StageBrowser), menu::stage_browser::spawn)
            .add_systems(
                Update,
                (
                    menu::stage_browser::handle_buttons,
                    menu::stage_browser::rebuild_list,
                    menu::stage_browser::update_details,
                    menu::stage_browser::update_status,
                    menu::stage_browser::scroll_list,
                )
                    .chain()
                    .run_if(in_state(AppState::StageBrowser)),
            )
            .add_systems(OnEnter(AppState::InGame), hud::gameplay::spawn)
            .add_systems(Update, common_button_interaction);
    }
}

fn common_button_interaction(
    mut query: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Button>)>,
    theme: Res<theme::UiTheme>,
) {
    for (interaction, mut background) in &mut query {
        *background = match *interaction {
            Interaction::Pressed => theme.button_pressed.into(),
            Interaction::Hovered => theme.button_hover.into(),
            Interaction::None => theme.button.into(),
        };
    }
}
