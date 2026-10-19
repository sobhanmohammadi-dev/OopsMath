//! The block the player has picked from the HUD.
//!
//! Selection is deliberately independent of placement: a future placement
//! system can read this resource, but nothing here places, previews, charges
//! for or counts blocks. The HUD keeps it consistent with the active stage's
//! allowed-block list.

use bevy::prelude::*;

use super::definition::BlockId;

#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct SelectedBlock(pub Option<BlockId>);
