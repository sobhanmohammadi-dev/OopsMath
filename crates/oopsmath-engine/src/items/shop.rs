//! Shop catalog and purchase transaction (no rendering).
//!
//! Catalog: every block in the canonical `BlockRegistry` (registration order)
//! that the active stage allows (`StagePlacementRules`) and that has a valid
//! price. Names, descriptions and prices are read from the `BlockDefinition`;
//! there is no separate price table. Policy: a price of 0 is not purchasable
//! (it would create free items), so such blocks are omitted. Tools are not
//! sold.
//!
//! Purchase: validate everything first, then change inventory and wallet
//! together. Stage placement caps are untouched (buying is not placing).

use bevy::prelude::*;
use thiserror::Error;

use super::ItemRef;
use super::economy::Wallet;
use super::inventory::Inventory;
use crate::blocks::stage_rules::StagePlacementRules;
use crate::blocks::{BlockDefinition, BlockId, BlockRegistry};

pub const MAX_PURCHASE_QUANTITY: u32 = 99;

/// One row of the shop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopProduct {
    pub id: BlockId,
    pub name: String,
    pub description: Option<String>,
    /// Price of one unit, in coins (always > 0).
    pub price: u32,
}

/// The block's purchasable unit price from `BlockEconomyConfig`, or `None`
/// when it has no valid price (0).
pub fn purchasable_price(def: &BlockDefinition) -> Option<u32> {
    let price = def.economy.default_unit_price;
    (price > 0).then_some(price)
}

pub fn catalog(registry: &BlockRegistry, rules: &StagePlacementRules) -> Vec<ShopProduct> {
    registry
        .iter()
        .filter(|def| rules.is_allowed(&def.id))
        .filter_map(|def| {
            Some(ShopProduct {
                id: def.id.clone(),
                name: def.display_name.clone(),
                description: def.description.clone(),
                price: purchasable_price(def)?,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PurchaseError {
    #[error("Quantity must be at least 1.")]
    ZeroQuantity,
    #[error("'{0}' is not a registered block.")]
    UnknownBlock(BlockId),
    #[error("'{0}' is not sold in this stage.")]
    NotAllowed(BlockId),
    #[error("'{0}' has no valid price and cannot be bought.")]
    NoPrice(BlockId),
    #[error("The total price is too large.")]
    PriceOverflow,
    #[error("Not enough money: you need {needed} coins but have {balance}.")]
    InsufficientFunds { needed: u32, balance: u32 },
    #[error("Your inventory cannot hold that many.")]
    InventoryFull,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub block: BlockId,
    pub quantity: u32,
    pub total: u32,
}

/// Buys `quantity` units of `block`. Atomic: on any error neither the wallet
/// nor the inventory has changed.
pub fn purchase(
    registry: &BlockRegistry,
    rules: &StagePlacementRules,
    wallet: &mut Wallet,
    inventory: &mut Inventory,
    block: &BlockId,
    quantity: u32,
) -> Result<Receipt, PurchaseError> {
    if quantity == 0 {
        return Err(PurchaseError::ZeroQuantity);
    }
    let def = registry
        .get(block)
        .ok_or_else(|| PurchaseError::UnknownBlock(block.clone()))?;
    if !rules.is_allowed(block) {
        return Err(PurchaseError::NotAllowed(block.clone()));
    }
    let price = purchasable_price(def).ok_or_else(|| PurchaseError::NoPrice(block.clone()))?;
    let total = price
        .checked_mul(quantity)
        .ok_or(PurchaseError::PriceOverflow)?;
    if !wallet.can_afford(total) {
        return Err(PurchaseError::InsufficientFunds {
            needed: total,
            balance: wallet.balance(),
        });
    }
    let item = ItemRef::Block(block.clone());
    // Capacity is checked before any money moves.
    if inventory.quantity(&item).checked_add(quantity).is_none() {
        return Err(PurchaseError::InventoryFull);
    }
    if inventory.add(item.clone(), quantity).is_err() {
        return Err(PurchaseError::InventoryFull);
    }
    if wallet.spend(total).is_err() {
        // Cannot happen (affordability was checked); undo the grant anyway.
        let _ = inventory.remove(&item, quantity);
        return Err(PurchaseError::InsufficientFunds {
            needed: total,
            balance: wallet.balance(),
        });
    }
    Ok(Receipt {
        block: block.clone(),
        quantity,
        total,
    })
}

/// Shop popup state. Opening or closing only touches this resource.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct ShopState {
    pub open: bool,
    quantity: u32,
    /// Result of the last purchase attempt, shown in the popup.
    pub message: Option<String>,
}

impl Default for ShopState {
    fn default() -> Self {
        Self {
            open: false,
            quantity: 1,
            message: None,
        }
    }
}

impl ShopState {
    pub fn quantity(&self) -> u32 {
        self.quantity
    }

    /// Clamps to `1..=MAX_PURCHASE_QUANTITY`.
    pub fn set_quantity(&mut self, quantity: i64) {
        self.quantity = quantity.clamp(1, i64::from(MAX_PURCHASE_QUANTITY)) as u32;
    }

    pub fn open(&mut self) {
        self.open = true;
        self.message = None;
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::blocks::PlacedWorld;
    use crate::blocks::definition::{BlockDimensions, BlockEconomyConfig, BlockModel};
    use crate::blocks::models::{register_builtin_blocks, wood_log_id};
    use crate::blocks::stage_rules::StagePlacementRules as StagePlacementRulesAlias;

    fn def(id: &str, price: u32) -> BlockDefinition {
        BlockDefinition::new(
            BlockId::new(id),
            id,
            BlockModel::glb("x.glb"),
            BlockDimensions::new(UVec3::ONE, Vec3::ONE),
            BlockEconomyConfig::with_default_price(price),
        )
    }

    fn registry() -> BlockRegistry {
        let mut r = BlockRegistry::new();
        register_builtin_blocks(&mut r).unwrap();
        r.register(def("block.stone", 8)).unwrap();
        r.register(def("block.free", 0)).unwrap();
        r
    }

    fn rules(ids: &[&str]) -> StagePlacementRulesAlias {
        let list: Vec<_> = ids.iter().map(|id| json!({ "id": id })).collect();
        StagePlacementRulesAlias::from_construction(&json!({ "allowed_blocks": list }))
    }

    fn log() -> ItemRef {
        ItemRef::Block(wood_log_id())
    }

    #[test]
    fn price_comes_from_the_block_definition() {
        let r = registry();
        assert_eq!(purchasable_price(r.get(&wood_log_id()).unwrap()), Some(3));
        assert_eq!(
            purchasable_price(r.get(&BlockId::new("block.free")).unwrap()),
            None
        );
    }

    #[test]
    fn catalog_has_only_registered_allowed_priced_blocks() {
        let r = registry();
        let rules = rules(&["block.stone", "block.wood_log", "brick", "block.free"]);
        let ids: Vec<_> = catalog(&r, &rules)
            .into_iter()
            .map(|p| p.id.to_string())
            .collect();
        // Registration order; `brick` is unregistered and `block.free` has no price.
        assert_eq!(ids, ["block.wood_log", "block.stone"]);
        let log = &catalog(&r, &rules)[0];
        assert_eq!((log.name.as_str(), log.price), ("Wood Log", 3));
        assert!(log.description.is_some());
        // A stage that allows nothing (or legacy) sells nothing.
        assert!(catalog(&r, &rules_empty()).is_empty());
    }

    fn rules_empty() -> StagePlacementRulesAlias {
        StagePlacementRulesAlias::from_construction(&json!({}))
    }

    #[test]
    fn single_purchase_charges_the_price_and_adds_one() {
        let r = registry();
        let rules = rules(&["block.wood_log"]);
        let (mut wallet, mut inv) = (Wallet::new(10), Inventory::default());
        let receipt = purchase(&r, &rules, &mut wallet, &mut inv, &wood_log_id(), 1).unwrap();
        assert_eq!(receipt.total, 3);
        assert_eq!(wallet.balance(), 7);
        assert_eq!(inv.quantity(&log()), 1);
    }

    #[test]
    fn quantity_purchase_uses_checked_total() {
        let r = registry();
        let rules = rules(&["block.wood_log"]);
        let (mut wallet, mut inv) = (Wallet::new(10), Inventory::default());
        let receipt = purchase(&r, &rules, &mut wallet, &mut inv, &wood_log_id(), 3).unwrap();
        assert_eq!(
            (
                receipt.quantity,
                receipt.total,
                wallet.balance(),
                inv.quantity(&log())
            ),
            (3, 9, 1, 3)
        );
        // 4 more would cost 12 with 1 left.
        assert_eq!(
            purchase(&r, &rules, &mut wallet, &mut inv, &wood_log_id(), 4),
            Err(PurchaseError::InsufficientFunds {
                needed: 12,
                balance: 1
            })
        );
        assert_eq!((wallet.balance(), inv.quantity(&log())), (1, 3));
        // Multiplication overflow is an error, not a saturated price.
        let mut rich = Wallet::new(u32::MAX);
        let mut big = BlockRegistry::new();
        big.register(def("block.gold", 1_000_000)).unwrap();
        assert_eq!(
            purchase(
                &big,
                &rules_for("block.gold"),
                &mut rich,
                &mut inv,
                &BlockId::new("block.gold"),
                u32::MAX
            ),
            Err(PurchaseError::PriceOverflow)
        );
        assert_eq!(rich.balance(), u32::MAX);
    }

    fn rules_for(id: &str) -> StagePlacementRulesAlias {
        rules(&[id])
    }

    #[test]
    fn unaffordable_and_invalid_purchases_change_nothing() {
        let r = registry();
        let rules = rules(&["block.wood_log", "block.free", "ghost.block"]);
        let mut wallet = Wallet::new(2);
        let mut inv = Inventory::default();
        let cases = [
            (
                wood_log_id(),
                1,
                PurchaseError::InsufficientFunds {
                    needed: 3,
                    balance: 2,
                },
            ),
            (wood_log_id(), 0, PurchaseError::ZeroQuantity),
            (
                BlockId::new("ghost.block"),
                1,
                PurchaseError::UnknownBlock(BlockId::new("ghost.block")),
            ),
            (
                BlockId::new("block.stone"),
                1,
                PurchaseError::NotAllowed(BlockId::new("block.stone")),
            ),
            (
                BlockId::new("block.free"),
                1,
                PurchaseError::NoPrice(BlockId::new("block.free")),
            ),
        ];
        for (id, qty, expected) in cases {
            assert_eq!(
                purchase(&r, &rules, &mut wallet, &mut inv, &id, qty),
                Err(expected)
            );
            assert_eq!(wallet.balance(), 2);
            assert!(inv.is_empty());
        }
    }

    #[test]
    fn inventory_overflow_is_rejected_before_any_money_moves() {
        let r = registry();
        let rules = rules(&["block.wood_log"]);
        let (mut wallet, mut inv) = (Wallet::new(100), Inventory::default());
        inv.add(log(), u32::MAX).unwrap();
        assert_eq!(
            purchase(&r, &rules, &mut wallet, &mut inv, &wood_log_id(), 1),
            Err(PurchaseError::InventoryFull)
        );
        assert_eq!((wallet.balance(), inv.quantity(&log())), (100, u32::MAX));
    }

    #[test]
    fn buying_is_not_placing() {
        let r = registry();
        let rules = rules(&["block.wood_log"]);
        let (mut wallet, mut inv) = (Wallet::new(100), Inventory::default());
        let world = PlacedWorld::default();
        purchase(&r, &rules, &mut wallet, &mut inv, &wood_log_id(), 5).unwrap();
        assert_eq!(world.count_of(&wood_log_id()), 0);
        assert!(world.is_empty());
    }

    #[test]
    fn opening_and_closing_touch_only_the_shop_state() {
        let (wallet, inv) = (Wallet::new(10), {
            let mut i = Inventory::default();
            i.add(log(), 2).unwrap();
            i
        });
        let (w0, i0) = (wallet, inv.clone());
        let mut shop = ShopState::default();
        shop.open();
        shop.set_quantity(7);
        shop.close();
        assert_eq!(shop, ShopState::default());
        assert_eq!((wallet, inv), (w0, i0));
    }

    #[test]
    fn quantity_selector_is_clamped() {
        let mut shop = ShopState::default();
        assert_eq!(shop.quantity(), 1);
        shop.set_quantity(0);
        assert_eq!(shop.quantity(), 1);
        shop.set_quantity(1000);
        assert_eq!(shop.quantity(), MAX_PURCHASE_QUANTITY);
    }

    #[test]
    fn wallet_arithmetic_never_wraps_and_stage_budget_seeds_it() {
        let mut w = Wallet::new(5);
        assert!(w.spend(6).is_err() && w.balance() == 5);
        assert!(w.spend(5).is_ok() && w.balance() == 0);
        let mut max = Wallet::new(u32::MAX);
        assert!(max.deposit(1).is_err() && max.balance() == u32::MAX);
        use crate::items::inventory::StartingResources;
        let with = StartingResources::parse(&json!({}), Some(&json!({"budget": 100}))).unwrap();
        assert_eq!(Wallet::for_stage(&with).balance(), 100);
        let legacy = StartingResources::parse(&json!({}), None).unwrap();
        assert_eq!(Wallet::for_stage(&legacy).balance(), 0, "no hidden grant");
    }
}
