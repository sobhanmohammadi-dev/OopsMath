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
use bevy::ui::GlobalZIndex;

use crate::app::session::{SessionAction, SessionOverlay, SessionUi};
use crate::app::states::AppState;
use crate::blueprint::BlueprintMode;
use crate::challenges::ChallengePanel;
use crate::items::shop::ShopState;
use crate::objectives::ObjectiveTracker;
use crate::ui::theme::UiTheme;

use super::{blueprint, challenge, shop};

/// Controls shown in the help panel. Every line was checked against the
/// input code (see `docs/blocks/SESSION.md`).
pub const CONTROLS: &[(&str, &str)] = &[
    (
        "Choose what to use",
        "Click a block in BLOCKS (top right) or an item in INVENTORY (bottom right).",
    ),
    (
        "Place a block",
        "Left click on the ground or on the face of a placed block. A ghost shows where it will go.",
    ),
    (
        "Rotate a block",
        "R turns the selected block a quarter turn (only blocks that can rotate).",
    ),
    (
        "Remove a block",
        "Select the Hammer in INVENTORY, then left click a placed block. You get the block back.",
    ),
    (
        "Camera",
        "Hold the right mouse button and drag to orbit, the middle button to pan, the wheel to zoom.",
    ),
    (
        "Shop",
        "Shop button (bottom left): buy blocks with your coins.",
    ),
    (
        "Math Challenge",
        "Math Challenge button (bottom left, if the stage has one): type the answer, Enter or Submit.",
    ),
    (
        "Plan",
        "Plan button (bottom left, if the stage has one): shows what a blueprint needs. Nothing is built for you.",
    ),
    (
        "Objectives",
        "The stage panel (top left) lists what you must do. Finish them all to complete the stage.",
    ),
    (
        "Stability",
        "Blocks must touch the ground or another block. If their support is removed they fall.",
    ),
    (
        "Menu and windows",
        "Esc closes an open window; otherwise it opens this menu. Back to Stages leaves the stage.",
    ),
];

#[derive(Component)]
pub struct SessionMenuButton;
#[derive(Component)]
pub struct SessionOverlayRoot;
#[derive(Component)]
pub struct SessionActionButton(pub SessionAction);

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

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
        Text::new("Menu (Esc)"),
        font(18.0),
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

fn button(parent: &mut ChildSpawnerCommands, theme: &UiTheme, label: &str, action: SessionAction) {
    parent.spawn((
        Button,
        SessionActionButton(action),
        Node {
            padding: UiRect::axes(px(14), px(10)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(theme.button),
        Text::new(label),
        font(18.0),
        TextColor(theme.text),
    ));
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
        .spawn((
            SessionOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            Interaction::default(),
            GlobalZIndex(110),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(width),
                    padding: UiRect::all(px(18)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
                    ..default()
                },
                BackgroundColor(theme.panel),
                Interaction::default(),
            ))
            .with_children(|panel| match overlay {
                SessionOverlay::Menu => {
                    panel.spawn((Text::new("MENU"), font(26.0), TextColor(theme.text)));
                    button(panel, &theme, "Resume", SessionAction::Resume);
                    button(panel, &theme, "Controls", SessionAction::ShowControls);
                    button(panel, &theme, "Restart Stage", SessionAction::Restart);
                    button(panel, &theme, "Back to Stages", SessionAction::BackToStages);
                    panel.spawn((
                        Text::new(
                            "Restart and Back to Stages discard your progress in this stage.",
                        ),
                        font(14.0),
                        TextColor(theme.secondary_text),
                    ));
                }
                SessionOverlay::Help => {
                    panel.spawn((Text::new("CONTROLS"), font(26.0), TextColor(theme.text)));
                    for (name, text) in CONTROLS {
                        panel
                            .spawn(Node {
                                flex_direction: FlexDirection::Column,
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((Text::new(*name), font(16.0), TextColor(theme.accent)));
                                row.spawn((Text::new(*text), font(15.0), TextColor(theme.text)));
                            });
                    }
                    button(panel, &theme, "Back", SessionAction::BackToMenu);
                }
                SessionOverlay::Complete => {
                    panel.spawn((
                        Text::new("STAGE COMPLETE!"),
                        font(28.0),
                        TextColor(theme.accent),
                    ));
                    panel.spawn((
                        Text::new("You finished every objective of this stage."),
                        font(17.0),
                        TextColor(theme.text),
                    ));
                    button(panel, &theme, "Replay Stage", SessionAction::Restart);
                    button(panel, &theme, "Back to Stages", SessionAction::BackToStages);
                    button(panel, &theme, "Keep Building", SessionAction::KeepBuilding);
                }
                SessionOverlay::None => {}
            });
        });
}
