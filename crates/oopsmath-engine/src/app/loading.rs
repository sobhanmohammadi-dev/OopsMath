use std::path::PathBuf;

use bevy::prelude::*;

use super::stages::{LoadedStage, StageLoadFailure, StageSelection};
use super::states::AppState;
use crate::stage::loader as stage_loader;

/// Environment variable that launches a specific DAT without the browser.
/// Intended for development and headless runs.
pub const STAGE_DAT_ENV: &str = "OOPSMATH_STAGE_DAT";

/// Loads the selected stage package with the existing DAT v1 loader and
/// transitions to [`AppState::InGame`] on success.
///
/// On failure the browser is re-entered with the error in
/// [`StageLoadFailure`], so a bad package is recoverable rather than fatal.
pub fn load_selected_stage(
    selection: Res<StageSelection>,
    mut loaded: ResMut<LoadedStage>,
    mut failure: ResMut<StageLoadFailure>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let Some(path) = resolve_selected_path(&selection) else {
        failure.0 = Some("No stage is selected.".to_string());
        next_state.set(AppState::StageBrowser);
        return;
    };

    match stage_loader::load(&path) {
        Ok(package) => {
            info!(
                "Loaded stage '{}' from {}",
                package.meta.stage_id,
                path.display()
            );
            loaded.0 = Some(package);
            failure.0 = None;
            next_state.set(AppState::InGame);
        }
        Err(err) => {
            warn!("Failed to load stage from {}: {err}", path.display());
            failure.0 = Some(format!("Failed to load '{}': {err}", path.display()));
            next_state.set(AppState::StageBrowser);
        }
    }
}

/// The selected `.dat` path, or the `OOPSMATH_STAGE_DAT` override.
fn resolve_selected_path(selection: &StageSelection) -> Option<PathBuf> {
    if let Some(path) = &selection.dat_path {
        return Some(path.clone());
    }
    std::env::var_os(STAGE_DAT_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
