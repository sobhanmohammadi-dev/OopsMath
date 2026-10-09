use bevy::prelude::*;

use crate::app::states::AppState;
use crate::ui::components::{GameplayHud, MaterialText, MoneyText, ObjectiveText, ToolText};
use crate::ui::theme::UiTheme;

pub fn spawn(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((
            GameplayHud,
            DespawnOnExit(AppState::InGame),
            Node {
                width: percent(100),
                height: percent(100),
                position_type: PositionType::Absolute,
                ..default()
            },
        ))
        .with_children(|parent| {
            // Top-left resource panel
            parent
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(16),
                        left: px(16),
                        padding: UiRect::all(px(10)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    },
                    BackgroundColor(theme.panel),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("$ 1000"),
                        MoneyText,
                        TextFont {
                            font_size: FontSize::Px(22.0),
                            ..default()
                        },
                        TextColor(theme.text),
                    ));

                    panel.spawn((
                        Text::new("Brick * 50"),
                        MaterialText,
                        TextFont {
                            font_size: FontSize::Px(20.0),
                            ..default()
                        },
                        TextColor(theme.text),
                    ));

                    panel.spawn((
                        Text::new("Hammer"),
                        ToolText,
                        TextFont {
                            font_size: FontSize::Px(20.0),
                            ..default()
                        },
                        TextColor(theme.text),
                    ));
                });

            // Objective panel
            parent
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(16),
                        bottom: px(16),
                        padding: UiRect::all(px(10)),
                        ..default()
                    },
                    BackgroundColor(theme.panel),
                ))
                .with_child((
                    Text::new("OBJECTIVE\nBuild Wall #04"),
                    ObjectiveText,
                    TextFont {
                        font_size: FontSize::Px(20.0),
                        ..default()
                    },
                    TextColor(theme.text),
                ));
        });
}
