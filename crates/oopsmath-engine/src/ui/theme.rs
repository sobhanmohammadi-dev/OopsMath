use bevy::prelude::*;

/// Colours shared by every panel. Existing fields keep their meaning; the
/// semantic colours below were added for HUD consistency (see
/// `docs/blocks/UI_STYLE.md`).
#[derive(Resource)]
pub struct UiTheme {
    pub background: Color,

    pub panel: Color,
    pub panel_hover: Color,
    /// Background of the currently selected entry in a list.
    pub selected: Color,

    /// Secondary buttons.
    pub button: Color,
    pub button_hover: Color,
    pub button_pressed: Color,

    /// Primary actions (Submit, Buy, Resume, Replay Stage).
    pub primary: Color,
    pub primary_hover: Color,
    pub primary_pressed: Color,

    pub text: Color,
    pub secondary_text: Color,
    /// Text of something that cannot be used right now.
    pub disabled_text: Color,
    /// Emphasis color for the selected list entry's title.
    pub accent: Color,

    pub success: Color,
    pub warning: Color,
    pub error: Color,

    /// Dimming layer behind modal popups.
    pub overlay: Color,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            background: Color::srgb(0.03, 0.03, 0.04),

            // Slightly see-through so the 3D scene is not completely hidden
            // behind the in-game panels.
            panel: Color::srgba(0.08, 0.08, 0.10, 0.94),
            panel_hover: Color::srgb(0.12, 0.12, 0.15),
            selected: Color::srgb(0.16, 0.22, 0.34),

            button: Color::srgb(0.10, 0.10, 0.13),
            button_hover: Color::srgb(0.16, 0.16, 0.20),
            button_pressed: Color::srgb(0.20, 0.20, 0.24),

            primary: Color::srgb(0.17, 0.38, 0.62),
            primary_hover: Color::srgb(0.22, 0.47, 0.76),
            primary_pressed: Color::srgb(0.13, 0.30, 0.50),

            text: Color::WHITE,
            secondary_text: Color::srgb(0.7, 0.7, 0.7),
            disabled_text: Color::srgb(0.48, 0.48, 0.50),
            accent: Color::srgb(0.45, 0.75, 1.0),

            success: Color::srgb(0.45, 0.85, 0.50),
            warning: Color::srgb(0.95, 0.75, 0.30),
            error: Color::srgb(1.0, 0.45, 0.45),

            overlay: Color::srgba(0.0, 0.0, 0.0, 0.6),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color) -> f32 {
        let c = color.to_linear();
        0.2126 * c.red + 0.7152 * c.green + 0.0722 * c.blue
    }

    fn contrast(a: Color, b: Color) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn text_is_readable_on_panels_and_buttons() {
        let t = UiTheme::default();
        for background in [t.panel, t.button, t.selected, t.primary] {
            assert!(
                contrast(t.text, background) >= 4.5,
                "text on {background:?}"
            );
        }
        for color in [t.secondary_text, t.accent] {
            assert!(contrast(color, t.panel) >= 4.5, "{color:?} on the panel");
        }
        for color in [t.success, t.warning, t.error] {
            assert!(contrast(color, t.panel) >= 4.5, "{color:?} on the panel");
        }
    }

    #[test]
    fn primary_and_secondary_buttons_differ_in_every_state() {
        let t = UiTheme::default();
        assert_ne!(t.primary, t.button);
        assert_ne!(t.primary_hover, t.button_hover);
        assert_ne!(t.primary_pressed, t.button_pressed);
        assert_ne!(t.primary, t.primary_hover);
        assert_ne!(t.primary, t.primary_pressed);
    }

    #[test]
    fn disabled_text_is_dimmer_than_normal_text() {
        let t = UiTheme::default();
        assert!(luminance(t.disabled_text) < luminance(t.secondary_text));
    }

    #[test]
    fn the_panel_is_translucent_but_mostly_opaque() {
        let a = UiTheme::default().panel.alpha();
        assert!((0.85..1.0).contains(&a));
    }
}
