use bevy::prelude::*;

#[derive(Resource)]
pub struct UiTheme {
    pub background: Color,
    pub panel: Color,
    pub text: Color,
    pub accent: Color,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            background: Color::srgb(0.03, 0.03, 0.04),
            panel: Color::srgb(0.08, 0.08, 0.10),
            text: Color::WHITE,
            accent: Color::srgb(0.2, 0.7, 1.0),
        }
    }
}