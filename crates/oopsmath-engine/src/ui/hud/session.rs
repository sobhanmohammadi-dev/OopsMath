//! Menu button, in-game menu, controls help and stage-completion panel.
//!
//! Presentation and input only; the rules live in `app::session::SessionUi`.
//! The overlays are full-screen nodes with an `Interaction`, like the Shop,
//! Math Challenge and Plan popups, so clicks do not reach the HUD or the world,
//! and the placement systems are gated on `SessionUi::blocks_world`.
//!
//! Escape: closes the Shop / Math Challenge / Plan popup if one is open (their
//! own handlers do that); otherwise it opens or closes this menu. To keep one
//! key press from doing both, `handle_session_input` is ordered *before* those
//! handlers and looks at the popup state as it was at the start of the frame.
//!
//! Leaving never despawns anything by hand. Restart requests
//! `AppState::LoadingStage` and Back to Stages requests
//! `AppState::StageBrowser`; `OnExit(InGame)` cleanup does the rest.

use bevy::prelude::*;

use crate::app::session::{SessionAction, SessionOverlay, SessionUi};
use crate::app::states::AppState;
use crate::blueprint::BlueprintMode;
use crate::challenges::ChallengePanel;
use crate::i18n::{FontRole, LocalizedText};
use crate::items::shop::ShopState;
use crate::objectives::ObjectiveTracker;
use crate::ui::style::{ButtonKind, dialog_button, font, modal_overlay, modal_panel, text};
use crate::ui::theme::UiTheme;

use super::{blueprint, challenge, shop};

/// Message ids of the controls shown in the help panel: (name, description).
/// Every line was checked against the input code (see `docs/blocks/SESSION.md`);
/// the texts are in `assets/i18n/<lang>/core.ftl`.
pub const CONTROLS: &[(&str, &str)] = &[
    ("help-select-name", "help-select-text"),
    ("help-place-name", "help-place-text"),
    ("help-rotate-name", "help-rotate-text"),
    ("help-remove-name", "help-remove-text"),
    ("help-camera-name", "help-camera-text"),
    ("help-shop-name", "help-shop-text"),
    ("help-math-name", "help-math-text"),
    ("help-plan-name", "help-plan-text"),
    ("help-objectives-name", "help-objectives-text"),
    ("help-stability-name", "help-stability-text"),
    ("help-menu-name", "help-menu-text"),
];

#[derive(Component)]
pub struct SessionMenuButton;
#[derive(Component)]
pub struct SessionOverlayRoot;
#[derive(Component)]
pub struct SessionActionButton(pub SessionAction);

pub struct SessionHudPlugin;

impl Plugin for SessionHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SessionUi>()
            .add_systems(
                OnEnter(AppState::InGame),
                (reset_session, spawn_menu_button),
            )
            .add_systems(OnExit(AppState::InGame), reset_session)
            .add_systems(
                Update,
                handle_session_input
                    .before(shop::handle_shop_input)
                    .before(challenge::handle_challenge_input)
                    .before(blueprint::handle_blueprint_input)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (detect_completion, refresh_session_overlay)
                    .chain()
                    .after(handle_session_input)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// A fresh run starts with no overlay and no completion latch; leaving the
/// stage clears them too.
fn reset_session(mut session: ResMut<SessionUi>) {
    *session = SessionUi::default();
}

/// The opener (bottom-left, above Plan).
pub fn spawn_menu_button(mut commands: Commands, theme: Res<UiTheme>) {
    commands.spawn((
        SessionMenuButton,
        Button,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(208),
            left: px(16),
            padding: UiRect::axes(px(18), px(10)),
            ..default()
        },
        BackgroundColor(theme.button),
        Text::new(""),
        LocalizedText::new("hud-menu"),
        FontRole::Button,
        font(text::BUTTON),
        TextColor(theme.text),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Escape, the Menu button and the overlay buttons. Only [`SessionUi`] and the
/// requested next state change here.
#[allow(clippy::too_many_arguments)]
pub fn handle_session_input(
    keys: Res<ButtonInput<KeyCode>>,
    menu_button: Query<&Interaction, (Changed<Interaction>, With<SessionMenuButton>)>,
    action_buttons: Query<(&Interaction, &SessionActionButton), Changed<Interaction>>,
    shop: Res<ShopState>,
    challenge: Res<ChallengePanel>,
    plan: Res<BlueprintMode>,
    mut session: ResMut<SessionUi>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let popup_open = shop.open || challenge.open || plan.panel_open;
    if menu_button.iter().any(|i| *i == Interaction::Pressed) || keys.just_pressed(KeyCode::Escape)
    {
        session.toggle_menu(popup_open);
        return;
    }
    for (interaction, button) in &action_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Some(next) = session.apply(button.0) {
            next_state.set(next);
            return;
        }
    }
}

/// Opens the completion panel once, from the authoritative objective state.
///
/// The last objective is usually finished inside a popup (solving the math
/// challenge), so completion does not wait for popups: it closes the Shop,
/// Math Challenge and Plan popups (their own `close()` methods, no other state
/// changes) and shows the panel.
pub fn detect_completion(
    tracker: Res<ObjectiveTracker>,
    mut shop: ResMut<ShopState>,
    mut challenge: ResMut<ChallengePanel>,
    mut plan: ResMut<BlueprintMode>,
    mut session: ResMut<SessionUi>,
) {
    let all_complete = tracker.all_complete();
    // `wants_completion` takes `&self`, so nothing is marked changed per frame.
    if !session.wants_completion(all_complete) {
        return;
    }
    if session.try_complete(all_complete) {
        info!("Stage complete: showing the completion panel");
        if shop.open {
            shop.close();
        }
        if challenge.open {
            challenge.close();
        }
        if plan.panel_open {
            plan.close();
        }
    }
}

fn button(
    parent: &mut ChildSpawnerCommands,
    theme: &UiTheme,
    key: &str,
    kind: ButtonKind,
    action: SessionAction,
) {
    dialog_button(parent, theme, key, kind, SessionActionButton(action));
}

/// A text that follows the language.
fn label(key: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(""),
        LocalizedText::new(key),
        font(size),
        TextColor(color),
    )
}

/// Rebuilds the overlay when the session changed (rare), never per frame.
pub fn refresh_session_overlay(
    mut commands: Commands,
    session: Res<SessionUi>,
    theme: Res<UiTheme>,
    existing: Query<Entity, With<SessionOverlayRoot>>,
) {
    if !session.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let overlay = session.overlay();
    if overlay == SessionOverlay::None {
        return;
    }
    let width = if overlay == SessionOverlay::Help {
        640.0
    } else {
        420.0
    };
    commands
        .spawn((SessionOverlayRoot, modal_overlay(&theme)))
        .with_children(|root| {
            root.spawn(modal_panel(&theme, width))
                .with_children(|panel| match overlay {
                    SessionOverlay::Menu => {
                        panel.spawn((
                            label("session-menu-title", text::TITLE, theme.text),
                            FontRole::Title,
                        ));
                        button(
                            panel,
                            &theme,
                            "session-resume",
                            ButtonKind::Primary,
                            SessionAction::Resume,
                        );
                        button(
                            panel,
                            &theme,
                            "session-controls",
                            ButtonKind::Secondary,
                            SessionAction::ShowControls,
                        );
                        button(
                            panel,
                            &theme,
                            "session-restart",
                            ButtonKind::Secondary,
                            SessionAction::Restart,
                        );
                        button(
                            panel,
                            &theme,
                            "session-back-to-stages",
                            ButtonKind::Secondary,
                            SessionAction::BackToStages,
                        );
                        panel.spawn(label(
                            "session-discard-warning",
                            text::HELPER,
                            theme.secondary_text,
                        ));
                    }
                    SessionOverlay::Help => {
                        panel.spawn((
                            label("session-controls-title", text::TITLE, theme.text),
                            FontRole::Title,
                        ));
                        for (name, description) in CONTROLS {
                            panel
                                .spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    ..default()
                                })
                                .with_children(|row| {
                                    row.spawn((
                                        label(name, text::BODY, theme.accent),
                                        FontRole::Heading,
                                    ));
                                    row.spawn(label(description, text::BODY - 1.0, theme.text));
                                });
                        }
                        button(
                            panel,
                            &theme,
                            "session-back",
                            ButtonKind::Primary,
                            SessionAction::BackToMenu,
                        );
                    }
                    SessionOverlay::Complete => {
                        panel.spawn((
                            label("session-complete-title", text::TITLE + 2.0, theme.success),
                            FontRole::Title,
                        ));
                        panel.spawn(label("session-complete-body", text::BODY, theme.text));
                        button(
                            panel,
                            &theme,
                            "session-replay",
                            ButtonKind::Primary,
                            SessionAction::Restart,
                        );
                        button(
                            panel,
                            &theme,
                            "session-back-to-stages",
                            ButtonKind::Secondary,
                            SessionAction::BackToStages,
                        );
                        button(
                            panel,
                            &theme,
                            "session-keep-building",
                            ButtonKind::Secondary,
                            SessionAction::KeepBuilding,
                        );
                    }
                    SessionOverlay::None => {}
                });
        });
}
