//! Stage-aware gameplay HUD.
//!
//! Presentation only. What is shown is computed by [`super::model`] (stage
//! information and allowed blocks) and [`super::hotbar`] (inventory rows) from
//! authoritative resources: the loaded stage, the block/tool registries and the
//! [`Inventory`]. The HUD owns no inventory values. Interaction is limited to
//! choosing the [`SelectedItem`]; there is no placement, consumption or
//! purchasing here.

use bevy::prelude::*;

use super::hotbar::{
    block_status_lines, build_hotbar, is_item_selectable, reconcile_item, selection_line,
};
use super::model::{BlockList, StageHud, StageHudModel};
use crate::app::stages::LoadedStage;
use crate::app::states::AppState;
use crate::blocks::stage_rules::StagePlacementRules;
use crate::blocks::{
    BlockRegistry, PlacedWorld, PlacementFeedback, PlacementRotation, SelectedBlock,
};
use crate::items::inventory::{Inventory, StageResources};
use crate::items::tools::{ToolKind, ToolRegistry};
use crate::items::{ItemRef, SelectedItem};
use crate::stage::localization::StageLocale;
use crate::ui::components::{
    GameplayHud, HudBlockButton, HudBlockList, HudBlockRow, HudHotbarButton, HudHotbarList,
    HudObjectiveList, HudText,
};
use crate::ui::theme::UiTheme;

fn text_font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// Spawns the static frame; [`refresh`] fills it in.
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
        .with_children(|root| {
            // Stage information (top-left).
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(16),
                    left: px(16),
                    max_width: px(420),
                    padding: UiRect::all(px(10)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
                BackgroundColor(theme.panel),
                Interaction::default(),
            ))
            .with_children(|panel| {
                panel.spawn((
                    HudText::Title,
                    Text::new(""),
                    text_font(24.0),
                    TextColor(theme.text),
                ));
                panel.spawn((
                    HudText::Description,
                    Text::new(""),
                    text_font(16.0),
                    TextColor(theme.secondary_text),
                ));
                panel.spawn((
                    HudObjectiveList,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        ..default()
                    },
                ));
            });

            // Allowed blocks (top-right).
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(16),
                    right: px(16),
                    width: px(320),
                    padding: UiRect::all(px(10)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
                BackgroundColor(theme.panel),
                Interaction::default(),
            ))
            .with_children(|panel| {
                panel.spawn((Text::new("BLOCKS"), text_font(18.0), TextColor(theme.text)));
                panel.spawn((
                    HudBlockList,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    },
                ));
            });

            // Inventory hotbar and current selection (bottom-right).
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(16),
                    right: px(16),
                    width: px(320),
                    padding: UiRect::all(px(10)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
                BackgroundColor(theme.panel),
                Interaction::default(),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("INVENTORY"),
                    text_font(18.0),
                    TextColor(theme.text),
                ));
                panel.spawn((
                    HudHotbarList,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    },
                ));
                panel.spawn((
                    HudText::Selection,
                    Text::new(""),
                    text_font(16.0),
                    TextColor(theme.accent),
                ));
            });

            // Placement feedback (bottom-left), no panel so it never blocks clicks.
            root.spawn((
                HudText::Feedback,
                Text::new(""),
                text_font(18.0),
                TextColor(theme.text),
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(16),
                    left: px(16),
                    ..default()
                },
            ));
        });
}

/// Marks the model for rebuilding on entering the game.
pub fn mark_stale(mut hud: ResMut<StageHud>) {
    hud.stale = true;
}

/// Drops HUD state when leaving the game (items reset in `ItemsPlugin`).
pub fn clear(mut hud: ResMut<StageHud>, mut selected: ResMut<SelectedBlock>) {
    *hud = StageHud::default();
    selected.0 = None;
}

/// Rebuilds the model when the loaded stage, registries or locale changed (or
/// on entry), then drops a selection the new stage no longer allows. A real
/// rebuild happens only on change, never per frame.
#[allow(clippy::too_many_arguments)]
pub fn sync_model(
    loaded: Res<LoadedStage>,
    registry: Res<BlockRegistry>,
    tools: Res<ToolRegistry>,
    locale: Res<StageLocale>,
    inventory: Res<Inventory>,
    mut hud: ResMut<StageHud>,
    mut selected: ResMut<SelectedItem>,
) {
    if hud.stale
        || loaded.is_changed()
        || registry.is_changed()
        || tools.is_changed()
        || locale.is_changed()
    {
        hud.model = loaded
            .0
            .as_ref()
            .map(|package| StageHudModel::from_package(package, &registry, &locale));
        hud.stale = false;
    }
    // The selection is revalidated whenever the model or the inventory changes
    // (e.g. a selected tool whose quantity reached zero).
    if hud.is_changed() || inventory.is_changed() {
        let kept = hud
            .model
            .as_ref()
            .and_then(|model| reconcile_item(selected.0.as_ref(), model, &inventory, &tools));
        if selected.0 != kept {
            selected.0 = kept;
        }
    }
}

/// Selects the clicked block if the stage allows it, it is registered and the
/// player owns at least one; otherwise explains why nothing happened.
#[allow(clippy::too_many_arguments)]
pub fn handle_block_clicks(
    interactions: Query<(&Interaction, &HudBlockButton), (Changed<Interaction>, With<Button>)>,
    hud: Res<StageHud>,
    inventory: Res<Inventory>,
    tools: Res<ToolRegistry>,
    registry: Res<BlockRegistry>,
    mut selected: ResMut<SelectedItem>,
    mut feedback: ResMut<PlacementFeedback>,
) {
    let Some(model) = &hud.model else {
        return;
    };
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed || !model.is_selectable(&button.block) {
            continue;
        }
        let item = ItemRef::Block(button.block.clone());
        if !is_item_selectable(&item, model, &inventory, &tools) {
            let name = registry
                .get(&button.block)
                .map_or(button.block.as_str(), |def| def.display_name.as_str());
            feedback.0 = Some(format!("You have no {name} in your inventory."));
            continue;
        }
        if selected.0.as_ref() != Some(&item) {
            selected.0 = Some(item);
        }
    }
}

/// Selects a clicked inventory item if it passes the selection rules.
pub fn handle_hotbar_clicks(
    interactions: Query<(&Interaction, &HudHotbarButton), (Changed<Interaction>, With<Button>)>,
    hud: Res<StageHud>,
    inventory: Res<Inventory>,
    tools: Res<ToolRegistry>,
    mut selected: ResMut<SelectedItem>,
) {
    let Some(model) = &hud.model else {
        return;
    };
    for (interaction, button) in &interactions {
        if *interaction == Interaction::Pressed
            && is_item_selectable(&button.item, model, &inventory, &tools)
            && selected.0.as_ref() != Some(&button.item)
        {
            selected.0 = Some(button.item.clone());
        }
    }
}

/// Updates texts and rebuilds the block and inventory rows when state changed.
#[allow(clippy::too_many_arguments)]
pub fn refresh(
    mut commands: Commands,
    hud: Res<StageHud>,
    selected: Res<SelectedItem>,
    inventory: Res<Inventory>,
    resources: Res<StageResources>,
    registry: Res<BlockRegistry>,
    tools: Res<ToolRegistry>,
    placed: Res<PlacedWorld>,
    rules: Res<StagePlacementRules>,
    rotation: Res<PlacementRotation>,
    theme: Res<UiTheme>,
    mut texts: Query<(&mut Text, &HudText)>,
    block_list: Query<Entity, With<HudBlockList>>,
    hotbar_list: Query<Entity, With<HudHotbarList>>,
    rows: Query<Entity, With<HudBlockRow>>,
) {
    if !(hud.is_changed()
        || selected.is_changed()
        || inventory.is_changed()
        || resources.is_changed()
        || placed.is_changed()
        || rules.is_changed()
        || rotation.is_changed())
    {
        return;
    }

    for (mut text, kind) in &mut texts {
        let value = match (&hud.model, kind) {
            (_, HudText::Feedback) => continue,
            (None, HudText::Title) => "No stage is loaded".to_string(),
            (None, _) => String::new(),
            (Some(model), HudText::Title) => model.title.clone(),
            (Some(model), HudText::Description) => model.description.clone().unwrap_or_default(),
            (Some(_), HudText::Selection) => {
                let mut line = selection_line(selected.0.as_ref(), &registry, &tools);
                if let Some(ItemRef::Block(id)) = &selected.0 {
                    line.push('\n');
                    line.push_str(&block_status_lines(
                        id,
                        &inventory,
                        &rules,
                        placed.count_of(id),
                    ));
                    if let Some(def) = registry.get(id).filter(|d| d.grid.rotations.is_rotatable())
                    {
                        let turned = def.grid.rotations.effective(rotation.0);
                        line.push_str(&format!("\nRotation: {} deg (R)", turned.degrees()));
                    }
                }
                if let Some(ItemRef::Tool(id)) = &selected.0
                    && tools
                        .get(id)
                        .is_some_and(|def| def.kind == ToolKind::Hammer)
                {
                    line.push_str("\nClick a placed block to remove it.");
                }
                line
            }
        };
        if text.0 != value {
            *text = Text::new(value);
        }
    }

    for row in &rows {
        commands.entity(row).despawn();
    }

    let note = |parent: &mut ChildSpawnerCommands, message: String| {
        parent.spawn((
            HudBlockRow,
            Text::new(message),
            text_font(15.0),
            TextColor(theme.secondary_text),
        ));
    };

    if let Ok(container) = block_list.single() {
        commands.entity(container).with_children(|parent| {
            let Some(model) = &hud.model else {
                note(parent, "Stage data is unavailable.".to_string());
                return;
            };
            if let Some(message) = model.list_message() {
                note(parent, message);
            }
            let BlockList::Entries(entries) = &model.blocks else {
                return;
            };
            for block in entries {
                let is_selected =
                    matches!(&selected.0, Some(ItemRef::Block(id)) if id == &block.id);
                if block.is_selectable() {
                    parent.spawn((
                        Button,
                        HudBlockButton {
                            block: block.id.clone(),
                        },
                        HudBlockRow,
                        Node {
                            width: percent(100),
                            padding: UiRect::all(px(8)),
                            ..default()
                        },
                        BackgroundColor(if is_selected {
                            theme.selected
                        } else {
                            theme.button
                        }),
                        Text::new(block.label(is_selected)),
                        text_font(17.0),
                        // The generic button hover system rewrites the
                        // background, so text colour and the "> " marker carry
                        // the selection.
                        TextColor(if is_selected {
                            theme.accent
                        } else {
                            theme.text
                        }),
                    ));
                } else {
                    note(parent, block.label(false));
                }
            }
        });
    }

    if let Ok(container) = hotbar_list.single() {
        commands.entity(container).with_children(|parent| {
            let Some(model) = &hud.model else {
                note(parent, "Stage data is unavailable.".to_string());
                return;
            };
            if let Some(error) = &resources.error {
                note(parent, format!("Starting resources are invalid: {error}"));
            }
            if let Some(message) = model.tools_message() {
                note(parent, message);
            }
            if let Some(error) = &rules.error {
                note(parent, format!("Block rules are invalid: {error}"));
            }
            let entries = build_hotbar(&inventory, model, &registry, &tools);
            if entries.is_empty() && resources.error.is_none() {
                note(parent, "Inventory is empty.".to_string());
            }
            for entry in entries {
                let is_selected = selected.0.as_ref() == Some(&entry.item);
                if entry.is_selectable() {
                    parent.spawn((
                        Button,
                        HudHotbarButton {
                            item: entry.item.clone(),
                        },
                        HudBlockRow,
                        Node {
                            width: percent(100),
                            padding: UiRect::all(px(8)),
                            ..default()
                        },
                        BackgroundColor(if is_selected {
                            theme.selected
                        } else {
                            theme.button
                        }),
                        Text::new(entry.label(is_selected)),
                        text_font(17.0),
                        TextColor(if is_selected {
                            theme.accent
                        } else {
                            theme.text
                        }),
                    ));
                } else {
                    note(parent, entry.label(false));
                }
            }
        });
    }
}

/// Shows the latest placement message; runs only when it changed.
pub fn update_feedback(feedback: Res<PlacementFeedback>, mut texts: Query<(&mut Text, &HudText)>) {
    if !feedback.is_changed() {
        return;
    }
    let value = feedback.0.clone().unwrap_or_default();
    for (mut text, kind) in &mut texts {
        if *kind == HudText::Feedback && text.0 != value {
            *text = Text::new(value.clone());
        }
    }
}
