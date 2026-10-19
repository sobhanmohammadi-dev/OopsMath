//! The player's runtime money.
//!
//! Money is whole coins (`u32`, the unit of `BlockEconomyConfig` prices).
//! The balance can only change through [`Wallet::spend`] / [`Wallet::deposit`],
//! which never wrap or saturate. Initial balance: the stage's `economy.budget`
//! when it declares one, otherwise 0 (see `docs/blocks/SHOP.md`).

use bevy::prelude::*;
use thiserror::Error;

use super::inventory::StartingResources;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WalletError {
    #[error("not enough money: {balance} available, {requested} needed")]
    Insufficient { balance: u32, requested: u32 },
    #[error("the balance would overflow")]
    Overflow,
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wallet(u32);

impl Wallet {
    pub const fn new(balance: u32) -> Self {
        Self(balance)
    }

    /// The starting wallet of a stage: `economy.budget` or nothing. A legacy
    /// stage without a budget starts with 0; money is never granted implicitly.
    pub fn for_stage(starting: &StartingResources) -> Self {
        Self(starting.currency.unwrap_or(0))
    }

    pub const fn balance(&self) -> u32 {
        self.0
    }

    pub fn can_afford(&self, amount: u32) -> bool {
        self.0 >= amount
    }

    /// Deducts `amount`; the balance is unchanged on error.
    pub fn spend(&mut self, amount: u32) -> Result<(), WalletError> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or(WalletError::Insufficient {
                balance: self.0,
                requested: amount,
            })?;
        Ok(())
    }

    pub fn deposit(&mut self, amount: u32) -> Result<(), WalletError> {
        self.0 = self.0.checked_add(amount).ok_or(WalletError::Overflow)?;
        Ok(())
    }
}
