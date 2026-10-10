use bevy::prelude::*;

#[derive(Resource)]
pub struct UiTheme {
    pub background: Color,

    pub panel: Color,
    pub panel_hover: Color,
    /// Background of the currently selected entry in a list.
    pub selected: Color,

    pub button: Color,
    pub button_hover: Color,
    pub button_pressed: Color,

    pub text: Color,
    pub secondary_text: Color,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            background: Color::srgb(0.03, 0.03, 0.04),

            panel: Color::srgb(0.08, 0.08, 0.10),
            panel_hover: Color::srgb(0.12, 0.12, 0.15),
            selected: Color::srgb(0.16, 0.22, 0.34),

            button: Color::srgb(0.10, 0.10, 0.13),
            button_hover: Color::srgb(0.16, 0.16, 0.20),
            button_pressed: Color::srgb(0.20, 0.20, 0.24),

            text: Color::WHITE,
            secondary_text: Color::srgb(0.7, 0.7, 0.7),
        }
    }
}
