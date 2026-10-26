//! Plan button and popup.
//!
//! Presentation and input only. Numbers come from `blueprint::plan::Plan`
//! (read-only), progress from `plan_statuses`. Opening, closing or starting the
//! guide never places, charges or consumes anything. The popup is a full-screen
//! overlay with an `Interaction`, so it blocks the HUD and world; placement,
//! rotation and the hammer do not run while it is open. Escape closes it.

use bevy::prelude::*;
use bevy::ui::GlobalZIndex;

use crate::app::states::AppState;
use crate::blocks::stage_rules::StagePlacementRules;
use crate::blocks::{BlockRegistry, PlacedWorld};
use crate::blueprint::{
    BlueprintMode, BlueprintProgress, Plan, StageBlueprints, built_count, plan_statuses,
};
use crate::challenges::ChallengePanel;
use crate::items::inventory::Inventory;
use crate::items::shop::ShopState;
use crate::ui::theme::UiTheme;

#[derive(Component)]
pub struct BlueprintButton;
#[derive(Component)]
pub struct BlueprintPopup;
#[derive(Component)]
pub struct BlueprintCloseButton;
#[derive(Component)]
pub struct BlueprintGuideButton;
#[derive(Component)]
pub struct BlueprintNavButton {
    delta: i32,
}

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// The opener (bottom-left, above Math Challenge). Hidden without blueprints.
pub fn spawn_blueprint_button(mut commands: Commands, theme: Res<UiTheme>) {
    commands.spawn((
        BlueprintButton,
        Button,
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            bottom: px(160),
            left: px(16),
            padding: UiRect::axes(px(18), px(10)),
            ..default()
        },
        BackgroundColor(theme.button),
        Text::new("Plan"),
        font(18.0),
        TextColor(theme.text),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Shows the button when the stage has blueprints; while a guide is active the
/// label shows its progress.
pub fn sync_blueprint_button(
    blueprints: Res<StageBlueprints>,
    mode: Res<BlueprintMode>,
    progress: Res<BlueprintProgress>,
    mut buttons: Query<(&mut Node, &mut Text), With<BlueprintButton>>,
) {
    if !(blueprints.is_changed() || mode.is_changed() || progress.is_changed()) {
        return;
    }
    let display = if blueprints.is_available() {
        Display::Flex
    } else {
        Display::None
    };
    let label = if mode.guide && !progress.statuses.is_empty() {
        format!(
            "Plan: {} / {} built",
            progress.built(),
            progress.statuses.len()
        )
    } else {
        "Plan".to_string()
    };
    for (mut node, mut text) in &mut buttons {
        if node.display != display {
            node.display = display;
        }
        if text.0 != label {
            *text = Text::new(label.clone());
        }
    }
}

/// Buttons and Escape. Only [`BlueprintMode`] changes here.
#[allow(clippy::too_many_arguments)]
pub fn handle_blueprint_input(
    keys: Res<ButtonInput<KeyCode>>,
    open_button: Query<&Interaction, (Changed<Interaction>, With<BlueprintButton>)>,
    close_button: Query<&Interaction, (Changed<Interaction>, With<BlueprintCloseButton>)>,
    guide_button: Query<&Interaction, (Changed<Interaction>, With<BlueprintGuideButton>)>,
    nav_buttons: Query<(&Interaction, &BlueprintNavButton), Changed<Interaction>>,
    shop: Res<ShopState>,
    challenge: Res<ChallengePanel>,
    blueprints: Res<StageBlueprints>,
    mut mode: ResMut<BlueprintMode>,
) {
    if open_button.iter().any(|i| *i == Interaction::Pressed) {
        if mode.panel_open {
            mode.close();
        } else if !shop.open && !challenge.open && blueprints.is_available() {
            mode.open();
        }
        return;
    }
    if !mode.panel_open {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) || close_button.iter().any(|i| *i == Interaction::Pressed)
    {
        mode.close();
        return;
    }
    for (interaction, button) in &nav_buttons {
        if *interaction == Interaction::Pressed {
            mode.step(button.delta, blueprints.blueprints.len());
        }
    }
    if guide_button.iter().any(|i| *i == Interaction::Pressed) {
        if mode.guide {
            mode.stop_guide();
        } else if !blueprints.blueprints.is_empty() {
            mode.start_building();
        }
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

/// Rebuilds the popup when something it shows changed (rare), never per frame.
#[allow(clippy::too_many_arguments)]
pub fn refresh_blueprint_popup(
    mut commands: Commands,
    mode: Res<BlueprintMode>,
    blueprints: Res<StageBlueprints>,
    registry: Res<BlockRegistry>,
    rules: Res<StagePlacementRules>,
    inventory: Res<Inventory>,
    world: Res<PlacedWorld>,
    theme: Res<UiTheme>,
    existing: Query<Entity, With<BlueprintPopup>>,
) {
    if !(mode.is_changed()
        || blueprints.is_changed()
        || inventory.is_changed()
        || rules.is_changed()
        || (mode.panel_open && world.is_changed()))
    {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    if !mode.panel_open {
        return;
    }
    let count = blueprints.blueprints.len();
    let selected = blueprints.get(mode.selected);
    let plan = selected.map(|b| Plan::compute(b, &registry, &rules, &inventory));
    let statuses = selected.map(|b| plan_statuses(b, &registry, &world));

    commands
        .spawn((
            BlueprintPopup,
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
            GlobalZIndex(100),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: px(560),
                        padding: UiRect::all(px(16)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        ..default()
                    },
                    BackgroundColor(theme.panel),
                    Interaction::default(),
                ))
                .with_children(|panel| {
                    let text = |panel: &mut ChildSpawnerCommands, s: String, size: f32, color: Color| {
                        panel.spawn((Text::new(s), font(size), TextColor(color)));
                    };
                    panel
                        .spawn(Node {
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|header| {
                            header.spawn((Text::new("CONSTRUCTION PLAN"), font(24.0), TextColor(theme.text)));
                            small_button(header, &theme, "Close", BlueprintCloseButton);
                        });

                    if let Some(error) = &blueprints.error {
                        text(panel, format!("This stage's blueprints are invalid: {error}"), 16.0, theme.secondary_text);
                        return;
                    }
                    let (Some(blueprint), Some(plan), Some(statuses)) = (selected, &plan, &statuses) else {
                        text(panel, "This stage has no blueprints.".into(), 16.0, theme.secondary_text);
                        return;
                    };

                    if count > 1 {
                        panel
                            .spawn(Node { align_items: AlignItems::Center, column_gap: px(12), ..default() })
                            .with_children(|row| {
                                small_button(row, &theme, "<", BlueprintNavButton { delta: -1 });
                                row.spawn((
                                    Text::new(format!("Plan {} / {count}", mode.selected + 1)),
                                    font(17.0),
                                    TextColor(theme.secondary_text),
                                ));
                                small_button(row, &theme, ">", BlueprintNavButton { delta: 1 });
                            });
                    }
                    text(panel, blueprint.title.clone(), 20.0, theme.accent);
                    text(
                        panel,
                        format!(
                            "Size: {} wide x {} high x {} deep (cells)    Blocks: {}    Built: {} / {}",
                            plan.size.x, plan.size.y, plan.size.z, plan.total_blocks,
                            built_count(statuses), statuses.len()
                        ),
                        16.0,
                        theme.text,
                    );
                    text(panel, "Materials".into(), 17.0, theme.secondary_text);
                    for line in &plan.lines {
                        let price = match (line.unit_price, line.line_cost) {
                            (Some(unit), Some(cost)) => format!("{unit} coins each = {cost} coins"),
                            (Some(unit), None) => format!("{unit} coins each (total too large)"),
                            _ => "no price".to_string(),
                        };
                        let short = if line.missing() > 0 {
                            format!(", {} more needed", line.missing())
                        } else {
                            String::new()
                        };
                        text(
                            panel,
                            format!("{}: need {}, have {}{short} ({price})", line.name, line.required, line.available),
                            16.0,
                            theme.text,
                        );
                    }
                    text(
                        panel,
                        match plan.total_cost {
                            Some(total) => format!("Estimated cost for everything: {total} coins"),
                            None => "Estimated cost: unavailable".to_string(),
                        },
                        17.0,
                        theme.text,
                    );
                    if let Some(missing) = plan.missing_cost {
                        text(panel, format!("Still to buy (what you do not own yet): {missing} coins"), 16.0, theme.secondary_text);
                    }
                    for issue in &plan.issues {
                        let prefix = if issue.is_blocking() { "Cannot build: " } else { "Note: " };
                        text(panel, format!("{prefix}{issue}"), 15.0, theme.secondary_text);
                    }
                    text(panel, "Nothing is charged or placed by this plan; you build it block by block.".into(), 14.0, theme.secondary_text);
                    if plan.is_buildable() {
                        let label = if mode.guide { "Stop guide" } else { "Start building (show guide)" };
                        small_button(panel, &theme, label, BlueprintGuideButton);
                    } else {
                        text(panel, "This plan cannot be built in this stage.".into(), 16.0, theme.accent);
                    }
                });
        });
}
