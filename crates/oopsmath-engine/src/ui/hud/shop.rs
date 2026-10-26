//! Shop button and popup.
//!
//! Presentation only: the catalog, prices and the purchase transaction come
//! from [`crate::items::shop`]; money from [`Wallet`]; stock from [`Inventory`].
//! The popup is a full-screen overlay with an `Interaction`, so it blocks the
//! HUD and the world behind it (placement and the hammer already ignore clicks
//! while any UI node is hovered, and the placement systems do not run while the
//! shop is open). The popup entities despawn when the game state exits.

use bevy::prelude::*;
use bevy::ui::GlobalZIndex;

use crate::app::stages::LoadedStage;
use crate::app::states::AppState;
use crate::blocks::stage_rules::StagePlacementRules;
use crate::blocks::{BlockId, BlockRegistry};
use crate::items::economy::Wallet;
use crate::items::inventory::Inventory;
use crate::items::shop::{ShopState, catalog, purchase};
use crate::ui::style::PrimaryButton;
use crate::ui::theme::UiTheme;

#[derive(Component)]
pub struct ShopButton;
#[derive(Component)]
pub struct ShopPopup;
#[derive(Component)]
pub struct ShopCloseButton;
#[derive(Component)]
pub struct ShopQuantityButton {
    delta: i64,
}
#[derive(Component)]
pub struct ShopBuyButton {
    block: BlockId,
}

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// The always-visible Shop button (bottom-left, above the feedback line).
pub fn spawn_shop_button(mut commands: Commands, theme: Res<UiTheme>) {
    commands.spawn((
        ShopButton,
        Button,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(64),
            left: px(16),
            padding: UiRect::axes(px(18), px(10)),
            ..default()
        },
        BackgroundColor(theme.button),
        Text::new("Shop"),
        font(18.0),
        TextColor(theme.text),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Handles every shop interaction. All state changes go through
/// [`ShopState`] or the validated [`purchase`] transaction.
#[allow(clippy::too_many_arguments)]
pub fn handle_shop_input(
    keys: Res<ButtonInput<KeyCode>>,
    open_button: Query<&Interaction, (Changed<Interaction>, With<ShopButton>)>,
    close_button: Query<&Interaction, (Changed<Interaction>, With<ShopCloseButton>)>,
    quantity_buttons: Query<(&Interaction, &ShopQuantityButton), Changed<Interaction>>,
    buy_buttons: Query<(&Interaction, &ShopBuyButton), Changed<Interaction>>,
    loaded: Res<LoadedStage>,
    registry: Res<BlockRegistry>,
    rules: Res<StagePlacementRules>,
    mut wallet: ResMut<Wallet>,
    mut inventory: ResMut<Inventory>,
    mut shop: ResMut<ShopState>,
) {
    if shop.open && keys.just_pressed(KeyCode::Escape) {
        shop.close();
    }
    if open_button.iter().any(|i| *i == Interaction::Pressed) {
        if shop.open {
            shop.close();
        } else if loaded.0.is_some() {
            shop.open();
        }
    }
    // Everything below only acts while the popup is open.
    if !shop.open {
        return;
    }
    if close_button.iter().any(|i| *i == Interaction::Pressed) {
        shop.close();
        return;
    }
    for (interaction, button) in &quantity_buttons {
        if *interaction == Interaction::Pressed {
            let next = i64::from(shop.quantity()) + button.delta;
            shop.set_quantity(next);
        }
    }
    for (interaction, button) in &buy_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let name = registry
            .get(&button.block)
            .map_or(button.block.as_str(), |def| def.display_name.as_str())
            .to_string();
        let quantity = shop.quantity();
        shop.message = Some(
            match purchase(
                &registry,
                &rules,
                &mut wallet,
                &mut inventory,
                &button.block,
                quantity,
            ) {
                Ok(receipt) => format!(
                    "Bought {} x {name} for {} coins.",
                    receipt.quantity, receipt.total
                ),
                Err(err) => err.to_string(),
            },
        );
    }
}

fn small_button(
    parent: &mut ChildSpawnerCommands,
    theme: &UiTheme,
    label: String,
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

/// Rebuilds the popup when anything it shows changed (rare events), and
/// removes it when the shop is closed.
#[allow(clippy::too_many_arguments)]
pub fn refresh_shop_popup(
    mut commands: Commands,
    shop: Res<ShopState>,
    wallet: Res<Wallet>,
    inventory: Res<Inventory>,
    rules: Res<StagePlacementRules>,
    registry: Res<BlockRegistry>,
    theme: Res<UiTheme>,
    existing: Query<Entity, With<ShopPopup>>,
) {
    if !(shop.is_changed() || wallet.is_changed() || inventory.is_changed() || rules.is_changed()) {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    if !shop.open {
        return;
    }
    let products = catalog(&registry, &rules);
    let quantity = shop.quantity();

    commands
        .spawn((
            ShopPopup,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(theme.overlay),
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
                        max_height: percent(90),
                        padding: UiRect::all(px(16)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(10),
                        ..default()
                    },
                    BackgroundColor(theme.panel),
                    Interaction::default(),
                ))
                .with_children(|panel| {
                    panel
                        .spawn(Node {
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|header| {
                            header.spawn((Text::new("SHOP"), font(24.0), TextColor(theme.text)));
                            small_button(header, &theme, "Close".into(), ShopCloseButton);
                        });
                    panel.spawn((
                        Text::new(format!("Money: {} coins", wallet.balance())),
                        font(22.0),
                        TextColor(theme.accent),
                    ));
                    panel
                        .spawn(Node {
                            align_items: AlignItems::Center,
                            column_gap: px(12),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Text::new("Quantity:"),
                                font(17.0),
                                TextColor(theme.secondary_text),
                            ));
                            small_button(row, &theme, "-".into(), ShopQuantityButton { delta: -1 });
                            row.spawn((
                                Text::new(quantity.to_string()),
                                font(18.0),
                                TextColor(theme.text),
                            ));
                            small_button(row, &theme, "+".into(), ShopQuantityButton { delta: 1 });
                        });
                    if products.is_empty() {
                        panel.spawn((
                            Text::new("No blocks are for sale in this stage."),
                            font(17.0),
                            TextColor(theme.secondary_text),
                        ));
                    }
                    for product in &products {
                        let owned =
                            inventory.quantity(&crate::items::ItemRef::Block(product.id.clone()));
                        let total = product.price.checked_mul(quantity);
                        let affordable = total.is_some_and(|t| wallet.can_afford(t));
                        panel
                            .spawn((
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(4),
                                    padding: UiRect::all(px(8)),
                                    ..default()
                                },
                                BackgroundColor(theme.button),
                            ))
                            .with_children(|card| {
                                card.spawn((
                                    Text::new(format!(
                                        "{} - {} coins each (you own {owned})",
                                        product.name, product.price
                                    )),
                                    font(18.0),
                                    TextColor(theme.text),
                                ));
                                if let Some(description) = &product.description {
                                    card.spawn((
                                        Text::new(description.clone()),
                                        font(15.0),
                                        TextColor(theme.secondary_text),
                                    ));
                                }
                                let label = match total {
                                    Some(t) => format!("Buy {quantity} for {t} coins"),
                                    None => "Too expensive".to_string(),
                                };
                                small_button(
                                    card,
                                    &theme,
                                    label,
                                    (
                                        ShopBuyButton {
                                            block: product.id.clone(),
                                        },
                                        PrimaryButton,
                                    ),
                                );
                                if !affordable {
                                    card.spawn((
                                        Text::new("Not enough money"),
                                        font(14.0),
                                        TextColor(theme.secondary_text),
                                    ));
                                }
                            });
                    }
                    if let Some(message) = &shop.message {
                        panel.spawn((
                            Text::new(message.clone()),
                            font(16.0),
                            TextColor(theme.accent),
                        ));
                    }
                });
        });
}
