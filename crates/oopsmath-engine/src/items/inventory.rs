//! Mutable inventory state and stage starting resources.
//!
//! Rules (documented here and in `docs/items/README.md`):
//! * Quantities are `u32`. Adding/removing zero is an error; overflow and
//!   underflow are errors, never wrapping or saturating.
//! * A zero quantity is represented by *absence*: an entry reaching zero is
//!   removed, and zero-valued starting entries are ignored.
//! * The inventory stores only ids and counts. Names, models and prices stay in
//!   the block/tool registries.
//! * Nothing here consumes items; placement does not touch the inventory yet.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde_json::Value;
use thiserror::Error;

use super::ItemRef;
use super::tools::ToolId;
use crate::blocks::BlockId;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InventoryError {
    #[error("quantity must be greater than zero")]
    ZeroQuantity,
    #[error("quantity of {0} would overflow")]
    Overflow(ItemRef),
    #[error("not enough {item}: have {have}, need {requested}")]
    Insufficient {
        item: ItemRef,
        have: u32,
        requested: u32,
    },
}

#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    counts: BTreeMap<ItemRef, u32>,
}

impl Inventory {
    pub fn quantity(&self, item: &ItemRef) -> u32 {
        self.counts.get(item).copied().unwrap_or(0)
    }

    /// True when at least `quantity` of the item is present (`quantity` of 0
    /// is trivially satisfied).
    pub fn has(&self, item: &ItemRef, quantity: u32) -> bool {
        self.quantity(item) >= quantity
    }

    /// Adds `quantity` and returns the new total.
    pub fn add(&mut self, item: ItemRef, quantity: u32) -> Result<u32, InventoryError> {
        if quantity == 0 {
            return Err(InventoryError::ZeroQuantity);
        }
        let total = self
            .quantity(&item)
            .checked_add(quantity)
            .ok_or_else(|| InventoryError::Overflow(item.clone()))?;
        self.counts.insert(item, total);
        Ok(total)
    }

    /// Removes `quantity` and returns what is left; an entry reaching zero is
    /// dropped. Nothing changes on error.
    pub fn remove(&mut self, item: &ItemRef, quantity: u32) -> Result<u32, InventoryError> {
        if quantity == 0 {
            return Err(InventoryError::ZeroQuantity);
        }
        let have = self.quantity(item);
        let left = have
            .checked_sub(quantity)
            .ok_or_else(|| InventoryError::Insufficient {
                item: item.clone(),
                have,
                requested: quantity,
            })?;
        if left == 0 {
            self.counts.remove(item);
        } else {
            self.counts.insert(item.clone(), left);
        }
        Ok(left)
    }

    /// Items with a non-zero quantity, in deterministic (sorted) order.
    pub fn iter(&self) -> impl Iterator<Item = (&ItemRef, u32)> {
        self.counts.iter().map(|(item, qty)| (item, *qty))
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn clear(&mut self) {
        self.counts.clear();
    }

    /// A fresh inventory holding exactly the stage's starting items.
    pub fn from_starting(starting: &StartingResources) -> Result<Self, InventoryError> {
        let mut inventory = Self::default();
        for (item, quantity) in &starting.items {
            inventory.add(item.clone(), *quantity)?;
        }
        Ok(inventory)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResourceError {
    #[error("construction.starting_inventory{0} must be an object")]
    NotAnObject(String),
    #[error("starting quantity of '{item}' must be a non-negative integer that fits in 32 bits")]
    InvalidQuantity { item: String },
    #[error("economy.budget must be a non-negative whole number of coins")]
    InvalidBudget,
}

/// What a stage grants at the start. Currency is parsed and stored for the
/// future economy milestone; there is no balance or spending yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartingResources {
    /// Sorted, with no zero quantities and no duplicates.
    pub items: Vec<(ItemRef, u32)>,
    /// `economy.budget` in whole coins, if the stage declares one.
    pub currency: Option<u32>,
}

impl StartingResources {
    /// Reads `construction.starting_inventory.{blocks,tools}` and
    /// `economy.budget`. A stage without them (legacy stages) yields nothing:
    /// no item or currency is ever granted implicitly. Ids are not checked
    /// against the registries here; the HUD reports unknown ones.
    pub fn parse(construction: &Value, economy: Option<&Value>) -> Result<Self, ResourceError> {
        let mut items = Vec::new();
        match construction.get("starting_inventory") {
            None | Some(Value::Null) => {}
            Some(Value::Object(inventory)) => {
                for key in ["blocks", "tools"] {
                    let section = match inventory.get(key) {
                        None | Some(Value::Null) => continue,
                        Some(Value::Object(map)) => map,
                        Some(_) => return Err(ResourceError::NotAnObject(format!(".{key}"))),
                    };
                    for (name, value) in section {
                        let quantity = value
                            .as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .ok_or_else(|| ResourceError::InvalidQuantity { item: name.clone() })?;
                        if quantity == 0 {
                            continue;
                        }
                        let item = if key == "blocks" {
                            ItemRef::Block(BlockId::new(name.as_str()))
                        } else {
                            ItemRef::Tool(ToolId::new(name.as_str()))
                        };
                        items.push((item, quantity));
                    }
                }
            }
            Some(_) => return Err(ResourceError::NotAnObject(String::new())),
        }
        items.sort();

        let currency = match economy.and_then(|e| e.get("budget")) {
            None | Some(Value::Null) => None,
            Some(value) => {
                let n = value.as_f64().ok_or(ResourceError::InvalidBudget)?;
                if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > f64::from(u32::MAX) {
                    return Err(ResourceError::InvalidBudget);
                }
                Some(n as u32)
            }
        };
        Ok(Self { items, currency })
    }
}

/// The active stage's starting resources (or why they could not be read).
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct StageResources {
    pub starting: StartingResources,
    pub error: Option<String>,
}
