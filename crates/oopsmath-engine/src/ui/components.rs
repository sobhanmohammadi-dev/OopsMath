use bevy::prelude::*;

#[derive(Component)]
pub struct GameplayHud;

#[derive(Component)]
pub struct MoneyText;

#[derive(Component)]
pub struct MaterialText;

#[derive(Component)]
pub struct ToolText;

#[derive(Component)]
pub struct ObjectiveText;

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
