use bevy::prelude::*;

use crate::app::states::AppState;
use crate::ui::{
    components::PlayButton,
    theme::UiTheme,
};

pub fn spawn(
    mut commands: Commands,
    theme: Res<UiTheme>,
) {
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

            parent.spawn((
                Button,
                PlayButton,
                Node {
                    width: px(220),
                    height: px(70),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(theme.panel),
                children![
                    (
                        Text::new("PLAY"),
                        TextFont {
                            font_size: FontSize::Px(28.0),
                            ..default()
                        },
                        TextColor(theme.text),
                    )
                ],
            ));
        });
}

pub fn handle_play_button(
    mut interactions: Query<
        &Interaction,
        (Changed<Interaction>, With<PlayButton>),
    >,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in &mut interactions {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::LoadingStage);
        }
    }
}