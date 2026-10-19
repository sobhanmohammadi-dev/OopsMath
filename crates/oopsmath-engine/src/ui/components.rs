use bevy::prelude::*;

use crate::blocks::BlockId;
use crate::items::ItemRef;

#[derive(Component)]
pub struct GameplayHud;

/// Which piece of stage information a HUD text node displays.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudText {
    Title,
    Description,
    Selection,
    Feedback,
}

/// The column that hosts the allowed-block rows of the gameplay HUD.
#[derive(Component)]
pub struct HudBlockList;

/// Any row inside [`HudBlockList`] (despawned and rebuilt on change).
#[derive(Component)]
pub struct HudBlockRow;

/// The column that hosts the objective rows.
#[derive(Component)]
pub struct HudObjectiveList;

/// A row inside [`HudObjectiveList`] (rebuilt when objectives change).
#[derive(Component)]
pub struct HudObjectiveRow;

/// The column that hosts the inventory hotbar rows.
#[derive(Component)]
pub struct HudHotbarList;

/// A selectable inventory row.
#[derive(Component)]
pub struct HudHotbarButton {
    pub item: ItemRef,
}

/// A selectable allowed-block row.
#[derive(Component)]
pub struct HudBlockButton {
    pub block: BlockId,
}

/// The root of the stage browser screen (also despawned on exit).
#[derive(Component)]
pub struct StageBrowserRoot;

/// The column that hosts the selectable stage entries.
#[derive(Component)]
pub struct StageListContainer;

/// One selectable stage entry; `index` addresses the catalog.
#[derive(Component)]
pub struct StageListEntry {
    pub index: usize,
}

/// The details panel's text block.
#[derive(Component)]
pub struct StageDetailsText;

/// The header's status line (stage count, diagnostics, load failures).
#[derive(Component)]
pub struct StageStatusText;

#[derive(Component)]
pub struct UiButton {
    pub action: UiAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    /// Main-menu "Start": opens the stage browser.
    Play,
    /// Highlights the catalog entry at the given index.
    SelectStage(usize),
    /// Loads the currently selected stage.
    PlaySelected,
    /// Returns to the main menu from the stage browser.
    Back,
    /// Re-scans the runtime stages directory.
    Rescan,
}
