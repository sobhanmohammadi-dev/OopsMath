use bevy::prelude::*;

use super::{
    components::{UiAction, UiButton},
    theme::UiTheme,
};

pub fn panel(theme: &UiTheme) -> impl Bundle {
    (
        Node {
            padding: UiRect::all(px(12)),
            ..default()
        },
        BackgroundColor(theme.panel),
    )
}

pub fn button(theme: &UiTheme, action: UiAction, width: f32, height: f32) -> impl Bundle {
    (
        Button,
        UiButton { action },
        Node {
            width: px(width),
            height: px(height),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(theme.button),
    )
}
