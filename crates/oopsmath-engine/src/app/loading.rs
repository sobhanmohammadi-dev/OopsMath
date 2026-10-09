use bevy::prelude::*;

use super::states::AppState;
use crate::stage::loader as stage_loader;
use crate::stage::package::StagePackage;

/// The stage DAT selected for loading. Defaults to `build/001_first_wall.dat`
/// resolved relative to the workspace root; override with the
/// `OOPSMATH_STAGE_DAT` environment variable. No machine-specific absolute
/// path is hard-coded.
#[derive(Resource)]
pub struct SelectedStage(pub String);

impl Default for SelectedStage {
    fn default() -> Self {
        Self(
            std::env::var("OOPSMATH_STAGE_DAT")
                .unwrap_or_else(|_| "build/001_first_wall.dat".to_string()),
        )
    }
}

/// Resource set after a successful stage load, so gameplay systems can
/// consume the package without re-reading the DAT file.
#[derive(Resource, Default)]
pub struct LoadedStage(pub Option<StagePackage>);

/// Loads the selected stage package synchronously and transitions to InGame
/// on success, or to MainMenu when the DAT cannot be loaded (the UruiTheme
/// main menu then shows a sensible fallback rather than a hard crash).
pub fn load_test_stage(
    selection: Option<Res<SelectedStage>>,
    mut loaded: ResMut<LoadedStage>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let path = selection
        .map(|sel| sel.0.clone())
        .unwrap_or_else(|| "build/001_first_wall.dat".to_string());
    match stage_loader::load(&path) {
        Ok(package) => {
            info!("Loaded stage '{}' from {path}", package.meta.stage_id);
            loaded.0 = Some(package);
            next_state.set(AppState::InGame);
        }
        Err(err) => {
            warn!("Failed to load stage from {path}: {err}");
            next_state.set(AppState::MainMenu);
        }
    }
}
