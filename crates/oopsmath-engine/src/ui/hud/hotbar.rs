//! Inventory hotbar data and selection rules, independent of rendering.
//!
//! Selection rule (shared by the hotbar, the block list and stage changes):
//! * Tool: allowed by the stage's `allowed_tools`, registered, **and** present
//!   in the inventory.
//! * Block: allowed by the stage's `allowed_blocks`, registered, **and** present
//!   in the inventory (placement consumes one unit per block).

use super::model::StageHudModel;
use crate::blocks::stage_rules::StagePlacementRules;
use crate::blocks::{BlockId, BlockRegistry};
use crate::items::ItemRef;
use crate::items::inventory::Inventory;
use crate::items::tools::ToolRegistry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotbarStatus {
    Selectable,
    NotRegistered,
    NotAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotbarEntry {
    pub item: ItemRef,
    /// Registry display name; `None` for unregistered ids.
    pub name: Option<String>,
    pub quantity: u32,
    pub status: HotbarStatus,
}

impl HotbarEntry {
    pub fn is_selectable(&self) -> bool {
        self.status == HotbarStatus::Selectable
    }

    pub fn label(&self, selected: bool) -> String {
        let kind = self.item.kind_label();
        match (&self.status, &self.name) {
            (HotbarStatus::Selectable, Some(name)) => {
                let marker = if selected { "> " } else { "" };
                format!("{marker}[{kind}] {name} x{}", self.quantity)
            }
            (status, _) => {
                let id = match &self.item {
                    ItemRef::Block(id) => id.to_string(),
                    ItemRef::Tool(id) => id.to_string(),
                };
                let reason = match status {
                    HotbarStatus::NotAllowed => "not allowed in this stage".to_string(),
                    _ => format!("not a registered {}", kind.to_lowercase()),
                };
                format!("[{kind}] {id} x{} - unavailable: {reason}", self.quantity)
            }
        }
    }
}

pub fn build_hotbar(
    inventory: &Inventory,
    model: &StageHudModel,
    blocks: &BlockRegistry,
    tools: &ToolRegistry,
) -> Vec<HotbarEntry> {
    inventory
        .iter()
        .map(|(item, quantity)| {
            let (name, registered, allowed) = match item {
                ItemRef::Block(id) => (
                    blocks.get(id).map(|d| d.display_name.clone()),
                    blocks.contains(id),
                    model.find(id).is_some(),
                ),
                ItemRef::Tool(id) => (
                    tools.get(id).map(|d| d.display_name.clone()),
                    tools.contains(id),
                    model.allows_tool(id),
                ),
            };
            let status = if !registered {
                HotbarStatus::NotRegistered
            } else if !allowed {
                HotbarStatus::NotAllowed
            } else {
                HotbarStatus::Selectable
            };
            HotbarEntry {
                item: item.clone(),
                name,
                quantity,
                status,
            }
        })
        .collect()
}

pub fn is_item_selectable(
    item: &ItemRef,
    model: &StageHudModel,
    inventory: &Inventory,
    tools: &ToolRegistry,
) -> bool {
    match item {
        ItemRef::Block(id) => model.is_selectable(id) && inventory.has(item, 1),
        ItemRef::Tool(id) => model.allows_tool(id) && tools.contains(id) && inventory.has(item, 1),
    }
}

/// The selection to keep after stage or inventory changes; nothing is auto-picked.
pub fn reconcile_item(
    current: Option<&ItemRef>,
    model: &StageHudModel,
    inventory: &Inventory,
    tools: &ToolRegistry,
) -> Option<ItemRef> {
    current
        .filter(|item| is_item_selectable(item, model, inventory, tools))
        .cloned()
}

/// `Selected: Wood Log (Block)` / `Selected: none`.
pub fn selection_line(
    selected: Option<&ItemRef>,
    blocks: &BlockRegistry,
    tools: &ToolRegistry,
) -> String {
    let name = selected.and_then(|item| match item {
        ItemRef::Block(id) => blocks.get(id).map(|d| d.display_name.as_str()),
        ItemRef::Tool(id) => tools.get(id).map(|d| d.display_name.as_str()),
    });
    match (selected, name) {
        (Some(item), Some(name)) => format!("Selected: {name} ({})", item.kind_label()),
        _ => "Selected: none".to_string(),
    }
}

/// Remaining quantity and placement count for a selected block:
/// `In inventory: 7` / `Placed: 3 / 15` (no `/ max` when the stage sets none).
pub fn block_status_lines(
    block: &BlockId,
    inventory: &Inventory,
    rules: &StagePlacementRules,
    placed: u32,
) -> String {
    let owned = inventory.quantity(&ItemRef::Block(block.clone()));
    let placed_line = match rules.max_placed(block) {
        Some(max) => format!("Placed: {placed} / {max}"),
        None => format!("Placed: {placed}"),
    };
    format!("In inventory: {owned}\n{placed_line}")
}
