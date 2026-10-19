//! Math challenge button and popup.
//!
//! Presentation and input only. Validation and state transitions live in
//! [`crate::challenges`] (`ChallengePanel::submit` -> `submit`). There is no
//! text-input widget in the project, so the answer field is fed from keyboard
//! messages while the popup is open: digits, `.`, `+`, `-`, Backspace, Enter to
//! submit, Escape to close. The popup is a full-screen overlay with an
//! `Interaction`, so it blocks the HUD and world; placement, rotation and the
//! hammer do not run while it is open. Opening, closing and solving touch no
//! inventory, money, blocks or selection.

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::ui::GlobalZIndex;

use crate::app::states::AppState;
use crate::challenges::{ChallengePanel, ChallengeTracker, StageChallenges};
use crate::items::shop::ShopState;
use crate::ui::theme::UiTheme;

#[derive(Component)]
pub struct ChallengeButton;
#[derive(Component)]
pub struct ChallengePopup;
#[derive(Component)]
pub struct ChallengeCloseButton;
#[derive(Component)]
pub struct ChallengeSubmitButton;
#[derive(Component)]
pub struct ChallengeNavButton {
    delta: i32,
}

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// The opener (bottom-left, above Shop). Hidden while the stage has no challenges.
pub fn spawn_challenge_button(mut commands: Commands, theme: Res<UiTheme>) {
    commands.spawn((
        ChallengeButton,
        Button,
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            bottom: px(112),
            left: px(16),
            padding: UiRect::axes(px(18), px(10)),
            ..default()
        },
        BackgroundColor(theme.button),
        Text::new("Math Challenge"),
        font(18.0),
        TextColor(theme.text),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Shows the opener only when the active stage has challenges.
pub fn sync_challenge_button(
    challenges: Res<StageChallenges>,
    mut buttons: Query<&mut Node, With<ChallengeButton>>,
) {
    let display = if challenges.challenges.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    for mut node in &mut buttons {
        if node.display != display {
            node.display = display;
        }
    }
}

/// Buttons, typing, Enter and Escape. All state changes go through
/// [`ChallengePanel`] and [`crate::challenges::submit`].
#[allow(clippy::too_many_arguments)]
pub fn handle_challenge_input(
    mut keyboard: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    open_button: Query<&Interaction, (Changed<Interaction>, With<ChallengeButton>)>,
    close_button: Query<&Interaction, (Changed<Interaction>, With<ChallengeCloseButton>)>,
    submit_button: Query<&Interaction, (Changed<Interaction>, With<ChallengeSubmitButton>)>,
    nav_buttons: Query<(&Interaction, &ChallengeNavButton), Changed<Interaction>>,
    shop: Res<ShopState>,
    challenges: Res<StageChallenges>,
    mut tracker: ResMut<ChallengeTracker>,
    mut panel: ResMut<ChallengePanel>,
) {
    // Always drain the messages so nothing typed earlier is replayed on open.
    let typed: Vec<KeyboardInput> = keyboard.read().cloned().collect();

    if open_button.iter().any(|i| *i == Interaction::Pressed) {
        if panel.open {
            panel.close();
        } else if !shop.open {
            panel.open_for(&challenges, &tracker);
        }
        return;
    }
    if !panel.open {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) || close_button.iter().any(|i| *i == Interaction::Pressed)
    {
        panel.close();
        return;
    }
    for (interaction, button) in &nav_buttons {
        if *interaction == Interaction::Pressed {
            panel.step(button.delta, challenges.challenges.len());
        }
    }
    let mut submit_requested = submit_button.iter().any(|i| *i == Interaction::Pressed);
    for event in typed.iter().filter(|e| e.state == ButtonState::Pressed) {
        match event.key_code {
            KeyCode::Backspace => panel.backspace(),
            KeyCode::Enter | KeyCode::NumpadEnter => submit_requested |= !event.repeat,
            _ => {
                if let Some(text) = &event.text {
                    text.chars().for_each(|c| panel.push_char(c));
                }
            }
        }
    }
    if submit_requested {
        panel.submit(&challenges, &mut tracker);
    }
}

fn small_button(
    parent: &mut ChildSpawnerCommands,
    theme: &UiTheme,
    label: &str,
    marker: impl Bundle,
) {
    parent.spawn((
        Button,
        marker,
        Node {
            padding: UiRect::axes(px(14), px(8)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(theme.button),
        Text::new(label),
        font(17.0),
        TextColor(theme.text),
    ));
}

/// Rebuilds the popup when the panel, solved state or challenges change.
pub fn refresh_challenge_popup(
    mut commands: Commands,
    panel: Res<ChallengePanel>,
    tracker: Res<ChallengeTracker>,
    challenges: Res<StageChallenges>,
    theme: Res<UiTheme>,
    existing: Query<Entity, With<ChallengePopup>>,
) {
    if !(panel.is_changed() || tracker.is_changed() || challenges.is_changed()) {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let Some(challenge) = challenges
        .challenges
        .get(panel.index)
        .filter(|_| panel.open)
    else {
        return;
    };
    let solved = tracker.is_solved_at(panel.index);
    let count = challenges.challenges.len();

    commands
        .spawn((
            ChallengePopup,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            // Blocks the HUD and world behind the popup.
            Interaction::default(),
            GlobalZIndex(100),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: px(480),
                        padding: UiRect::all(px(16)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(12),
                        ..default()
                    },
                    BackgroundColor(theme.panel),
                    Interaction::default(),
                ))
                .with_children(|box_| {
                    box_.spawn(Node {
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|header| {
                        header.spawn((
                            Text::new("MATH CHALLENGE"),
                            font(24.0),
                            TextColor(theme.text),
                        ));
                        small_button(header, &theme, "Close", ChallengeCloseButton);
                    });
                    if count > 1 {
                        box_.spawn(Node {
                            align_items: AlignItems::Center,
                            column_gap: px(12),
                            ..default()
                        })
                        .with_children(|row| {
                            small_button(row, &theme, "<", ChallengeNavButton { delta: -1 });
                            row.spawn((
                                Text::new(format!("Challenge {} / {count}", panel.index + 1)),
                                font(17.0),
                                TextColor(theme.secondary_text),
                            ));
                            small_button(row, &theme, ">", ChallengeNavButton { delta: 1 });
                        });
                    }
                    box_.spawn((
                        Text::new(challenge.prompt.clone()),
                        font(20.0),
                        TextColor(theme.text),
                    ));
                    if solved {
                        box_.spawn((Text::new("Solved!"), font(22.0), TextColor(theme.accent)));
                    } else {
                        box_.spawn((
                            Text::new(format!("Answer: {}_", panel.input)),
                            font(22.0),
                            TextColor(theme.accent),
                        ));
                        small_button(box_, &theme, "Submit", ChallengeSubmitButton);
                    }
                    if let Some(feedback) = &panel.feedback {
                        box_.spawn((
                            Text::new(feedback.clone()),
                            font(16.0),
                            TextColor(theme.secondary_text),
                        ));
                    }
                });
        });
}
