//! Gameplay items: placeable blocks and usable tools, the player's inventory,
//! and the single selected item.
//!
//! Block data stays in `blocks::BlockRegistry`; an [`ItemRef::Block`] only
//! names a registered block. Tools live in [`tools::ToolRegistry`]. The
//! inventory (`inventory::Inventory`) holds counts only. Not in this module:
//! consumption on placement, removal, purchasing, currency balance.

pub mod economy;
pub mod inventory;
pub mod shop;
pub mod tools;

#[cfg(test)]
mod tests;

use std::fmt;

use bevy::prelude::*;

use crate::app::stages::LoadedStage;
use crate::app::states::AppState;
use crate::blocks::{BlockDefinition, BlockId, BlockRegistry, SelectedBlock};
use economy::Wallet;
use inventory::{Inventory, StageResources, StartingResources};
use shop::ShopState;
use tools::{ToolId, ToolRegistry, register_builtin_tools};

/// Stable identity of a gameplay item. Blocks and tools are separate
/// namespaces (matching the stage's `blocks`/`tools` sections), so a tool can
/// never be mistaken for a placeable block.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ItemRef {
    Block(BlockId),
    Tool(ToolId),
}

impl ItemRef {
    pub fn as_block(&self) -> Option<&BlockId> {
        match self {
            Self::Block(id) => Some(id),
            Self::Tool(_) => None,
        }
    }

    pub fn as_tool(&self) -> Option<&ToolId> {
        match self {
            Self::Tool(id) => Some(id),
            Self::Block(_) => None,
        }
    }

    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Block(_) => "Block",
            Self::Tool(_) => "Tool",
        }
    }

    /// The registered definition needed to place this item; `None` for every
    /// tool and for unregistered blocks.
    pub fn resolve_block<'r>(&self, registry: &'r BlockRegistry) -> Option<&'r BlockDefinition> {
        self.as_block().and_then(|id| registry.get(id))
    }
}

impl fmt::Display for ItemRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Block(id) => write!(f, "block '{id}'"),
            Self::Tool(id) => write!(f, "tool '{id}'"),
        }
    }
}

/// The one item the player currently has selected (block or tool).
/// [`SelectedBlock`] is derived from it for the placement system.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct SelectedItem(pub Option<ItemRef>);

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        let mut tools = ToolRegistry::default();
        if let Err(err) = register_builtin_tools(&mut tools) {
            error!("Built-in tool registration failed: {err}");
        }
        app.insert_resource(tools)
            .init_resource::<Inventory>()
            .init_resource::<StageResources>()
            .init_resource::<SelectedItem>()
            .init_resource::<Wallet>()
            .init_resource::<ShopState>()
            .add_systems(OnEnter(AppState::InGame), init_stage_items)
            .add_systems(OnExit(AppState::InGame), reset_items)
            .add_systems(
                Update,
                sync_selected_block.run_if(resource_changed::<SelectedItem>),
            );
    }
}

/// Builds the inventory from the loaded stage. Deterministic: the result
/// depends only on the stage data. Invalid data leaves an empty inventory and
/// records the reason in [`StageResources::error`] (shown in the HUD).
fn init_stage_items(
    loaded: Res<LoadedStage>,
    mut inventory: ResMut<Inventory>,
    mut resources: ResMut<StageResources>,
    mut selected: ResMut<SelectedItem>,
    mut wallet: ResMut<Wallet>,
    mut shop: ResMut<ShopState>,
) {
    *selected = SelectedItem::default();
    *wallet = Wallet::default();
    shop.close();
    *inventory = Inventory::default();
    *resources = StageResources::default();
    let Some(package) = loaded.0.as_ref() else {
        return;
    };
    let parsed =
        StartingResources::parse(&package.stage.construction, package.stage.economy.as_ref());
    match parsed {
        Ok(starting) => match Inventory::from_starting(&starting) {
            Ok(initial) => {
                *inventory = initial;
                // economy.budget is the starting balance (documented in SHOP.md).
                *wallet = Wallet::for_stage(&starting);
                resources.starting = starting;
            }
            Err(err) => {
                error!("Could not initialise the inventory: {err}");
                resources.error = Some(err.to_string());
            }
        },
        Err(err) => {
            error!(
                "Invalid starting resources in stage '{}': {err}",
                package.meta.stage_id
            );
            resources.error = Some(err.to_string());
        }
    }
}

fn reset_items(
    mut inventory: ResMut<Inventory>,
    mut resources: ResMut<StageResources>,
    mut selected: ResMut<SelectedItem>,
    mut block: ResMut<SelectedBlock>,
    mut wallet: ResMut<Wallet>,
    mut shop: ResMut<ShopState>,
) {
    *wallet = Wallet::default();
    shop.close();
    *inventory = Inventory::default();
    *resources = StageResources::default();
    *selected = SelectedItem::default();
    block.0 = None;
}

/// Keeps the placement system's [`SelectedBlock`] equal to the selected item
/// when it is a block, and empty when it is a tool or nothing.
fn sync_selected_block(item: Res<SelectedItem>, mut block: ResMut<SelectedBlock>) {
    let wanted = item.0.as_ref().and_then(ItemRef::as_block).cloned();
    if block.0 != wanted {
        block.0 = wanted;
    }
}
