# Economy and shop

Code: `items/economy.rs` (`Wallet`), `items/shop.rs` (catalog, `purchase`,
`ShopState`), `ui/hud/shop.rs` (button and popup).

## Money

* `Wallet(u32)` in whole coins; only `spend` / `deposit` change it (no wrapping,
  no saturating).
* **Initial balance = the stage's `economy.budget`**; a stage without it starts
  with 0 (nothing is granted implicitly). Ambiguity: the schema only says
  `budget` is a non-negative number (it also has a `must_finish_within_budget`
  rule that could mean a spending cap). It is the only money-like field and was
  already parsed as the starting currency, so it is used as the starting balance;
  no field was added or renamed. Revisit if budget is later defined as a cap.
* The wallet is set once when entering the game and cleared on exit; opening the
  shop never resets it. Reloading a stage re-seeds it from the stage.
* The hammer refund returns the block item only; there is no monetary refund.

## Catalog and prices

Products = blocks in the `BlockRegistry` (registration order) that the active
stage allows (`allowed_blocks`) **and** that have a valid price. Name,
description and price are read from the `BlockDefinition`
(`BlockEconomyConfig.default_unit_price`); there is no separate price table.
A price of 0 is "no valid price": the block is omitted (it would be free).
Unregistered ids, blocks the stage does not allow and tools never appear. An
empty catalog shows "No blocks are for sale in this stage."

## Purchasing (`items::shop::purchase`)

Checks, in order: quantity >= 1, block registered, allowed by the stage, valid
price, total = price x quantity (checked), enough money, inventory can hold the
quantity. Only then the inventory and wallet change together; any failure
changes neither. Buying does not touch placement counts or the grid. The quantity
selector is 1..=99 and applies to the Buy buttons.

## UI

A **Shop** button (bottom-left) toggles the popup; **Close** and **Escape** also
close it. It cannot open without a loaded stage. The popup shows `Money: N coins`,
the quantity selector, a card per product with a Buy button, and the result of the
last attempt. While it is open the overlay blocks HUD and world clicks, and the
placement, rotation and hammer systems do not run. Leaving the game closes it and
despawns it. Not implemented: scrolling for very long catalogs, selling, tools,
stock limits, dynamic prices.
