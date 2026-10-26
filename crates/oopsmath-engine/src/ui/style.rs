//! Small shared UI style layer: sizes, a font helper, modal building blocks,
//! primary buttons and the expiry of the transient feedback line.
//!
//! Colours live in [`UiTheme`]. Nothing here changes what a button does: the
//! helpers only build nodes, and the systems only recolour primary buttons and
//! clear an old placement message.

use bevy::prelude::*;
use bevy::ui::GlobalZIndex;

use crate::app::states::AppState;
use crate::blocks::PlacementFeedback;
use crate::i18n::{FontRole, LocalizedText};

use super::theme::UiTheme;

/// Text sizes (logical px).
pub mod text {
    pub const TITLE: f32 = 26.0;
    pub const HEADING: f32 = 20.0;
    pub const BODY: f32 = 16.0;
    pub const BUTTON: f32 = 18.0;
    pub const HELPER: f32 = 14.0;
}

/// Spacing (logical px).
pub mod space {
    pub const PANEL_PADDING: f32 = 18.0;
    pub const GAP: f32 = 10.0;
    pub const BUTTON_PAD_X: f32 = 18.0;
    pub const BUTTON_PAD_Y: f32 = 10.0;
}

/// Stacking order of modal overlays (above the HUD and the other popups).
pub const MODAL_Z: i32 = 110;

/// Seconds a placement / hammer / collapse message stays on screen.
pub const FEEDBACK_SECONDS: f32 = 6.0;

pub fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
}

/// Marks a primary button. The generic button system recolours every `Button`
/// with the secondary colours on interaction; the systems below then put the
/// primary colours back.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct PrimaryButton;

/// Full-screen dimming layer for a modal. It carries an `Interaction`, so
/// clicks never reach the HUD or the world behind it.
pub fn modal_overlay(theme: &UiTheme) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(theme.overlay),
        Interaction::default(),
        GlobalZIndex(MODAL_Z),
        DespawnOnExit(AppState::InGame),
    )
}

/// The panel inside a modal overlay.
pub fn modal_panel(theme: &UiTheme, width: f32) -> impl Bundle {
    (
        Node {
            width: px(width),
            max_width: percent(95),
            padding: UiRect::all(px(space::PANEL_PADDING)),
            flex_direction: FlexDirection::Column,
            row_gap: px(space::GAP),
            ..default()
        },
        BackgroundColor(theme.panel),
        Interaction::default(),
    )
}

/// A button whose label is the localized message `key`, with the shared size
/// and text style. The label follows the active language and font.
pub fn dialog_button(
    parent: &mut ChildSpawnerCommands,
    theme: &UiTheme,
    key: &str,
    kind: ButtonKind,
    marker: impl Bundle,
) {
    let base = match kind {
        ButtonKind::Primary => theme.primary,
        ButtonKind::Secondary => theme.button,
    };
    let mut entity = parent.spawn((
        Button,
        marker,
        Node {
            padding: UiRect::axes(px(space::BUTTON_PAD_X), px(space::BUTTON_PAD_Y)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(base),
        Text::new(""),
        LocalizedText::new(key),
        FontRole::Button,
        font(text::BUTTON),
        TextColor(theme.text),
    ));
    if kind == ButtonKind::Primary {
        entity.insert(PrimaryButton);
    }
}

/// Colours a freshly spawned primary button (in `PostUpdate`, before
/// rendering, so it is never drawn with the secondary colour).
fn paint_new_primary_buttons(
    theme: Res<UiTheme>,
    mut buttons: Query<&mut BackgroundColor, Added<PrimaryButton>>,
) {
    for mut background in &mut buttons {
        *background = theme.primary.into();
    }
}

/// Hover / pressed colours of primary buttons. Runs after the generic button
/// system so it has the last word on the same frame.
fn primary_button_interaction(
    theme: Res<UiTheme>,
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<PrimaryButton>),
    >,
) {
    for (interaction, mut background) in &mut buttons {
        *background = match *interaction {
            Interaction::Pressed => theme.primary_pressed.into(),
            Interaction::Hovered => theme.primary_hover.into(),
            Interaction::None => theme.primary.into(),
        };
    }
}

/// When the current feedback message appeared. A new message restarts the
/// timer, so an older message's expiry can never clear a newer one: expiry is
/// always measured from the latest change.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct FeedbackClock {
    since: f32,
}

impl FeedbackClock {
    pub fn restart(&mut self, now: f32) {
        self.since = now;
    }

    pub fn expired(&self, now: f32, has_message: bool) -> bool {
        has_message && now - self.since >= FEEDBACK_SECONDS
    }
}

fn expire_feedback(
    time: Res<Time>,
    mut clock: ResMut<FeedbackClock>,
    mut feedback: ResMut<PlacementFeedback>,
) {
    let now = time.elapsed_secs();
    if feedback.is_changed() {
        clock.restart(now);
    }
    if clock.expired(now, feedback.0.is_some()) {
        feedback.0 = None;
    }
}

pub struct StylePlugin;

impl Plugin for StylePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FeedbackClock>()
            .add_systems(PostUpdate, paint_new_primary_buttons)
            .add_systems(
                Update,
                (
                    primary_button_interaction.after(super::common_button_interaction),
                    expire_feedback,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_expires_only_after_the_configured_time() {
        let mut clock = FeedbackClock::default();
        clock.restart(10.0);
        assert!(!clock.expired(10.0, true));
        assert!(!clock.expired(10.0 + FEEDBACK_SECONDS - 0.01, true));
        assert!(clock.expired(10.0 + FEEDBACK_SECONDS, true));
    }

    #[test]
    fn nothing_expires_when_there_is_no_message() {
        let mut clock = FeedbackClock::default();
        clock.restart(0.0);
        assert!(!clock.expired(1000.0, false));
    }

    #[test]
    fn a_newer_message_is_not_cleared_by_an_older_timer() {
        let mut clock = FeedbackClock::default();
        clock.restart(0.0); // message A
        clock.restart(FEEDBACK_SECONDS - 1.0); // message B just before A expires
        // A's deadline has passed, B's has not.
        assert!(!clock.expired(FEEDBACK_SECONDS + 0.5, true));
        assert!(clock.expired(2.0 * FEEDBACK_SECONDS - 1.0, true));
    }

    #[test]
    fn text_sizes_form_a_descending_hierarchy() {
        assert!(text::TITLE > text::HEADING);
        assert!(text::HEADING > text::BODY);
        assert!(text::BODY > text::HELPER);
        assert!(text::BUTTON >= text::BODY);
    }
}
