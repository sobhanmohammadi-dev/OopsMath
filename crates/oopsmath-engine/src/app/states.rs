use bevy::prelude::*;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Boot,
    MainMenu,
    /// Stage selection: list of discovered packages plus a details panel.
    StageBrowser,
    LoadingStage,
    InGame,
    Paused,
    /// Opt-in development scene for the Blocks module (`OOPSMATH_BLOCK_PREVIEW=1`).
    BlockPreview,
}
