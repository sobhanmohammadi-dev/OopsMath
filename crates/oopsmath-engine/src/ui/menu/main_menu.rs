use bevy::prelude::*;

use crate::{
    app::states::AppState,
    ui::{
        common::button,
        components::{UiAction, UiButton},
        theme::UiTheme,
    },
};

pub fn spawn(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((
            DespawnOnExit(AppState::MainMenu),
            Node {
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                row_gap: px(20),
                ..default()
            },
            BackgroundColor(theme.background),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("OOPSMATH"),
                TextFont {
                    font_size: FontSize::Px(48.0),
                    ..default()
                },
                TextColor(theme.text),
            ));

            parent
                .spawn(button(&theme, UiAction::Play, 220.0, 70.0))
                .with_child((
                    Text::new("PLAY"),
                    TextFont {
                        font_size: FontSize::Px(28.0),
                        ..default()
                    },
                    TextColor(theme.text),
                ));
        });
}

pub fn handle_buttons(
    mut interactions: Query<(&Interaction, &UiButton), (Changed<Interaction>, With<Button>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        if let Some(next) = menu_transition(button.action) {
            next_state.set(next);
        }
    }
}

/// Maps a main-menu action to the state it opens, if any.
///
/// Start opens the stage browser; a stage is loaded only once one has been
/// chosen and Play Stage is pressed there.
fn menu_transition(action: UiAction) -> Option<AppState> {
    match action {
        UiAction::Play => Some(AppState::StageBrowser),
        // Other actions belong to the stage browser.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_opens_the_stage_browser() {
        assert_eq!(
            menu_transition(UiAction::Play),
            Some(AppState::StageBrowser)
        );
        assert_eq!(menu_transition(UiAction::Back), None);
        assert_eq!(menu_transition(UiAction::PlaySelected), None);
    }
}
