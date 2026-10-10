//! Application-level stage context: the discovered catalog, the current
//! selection, and the outcome of the last load attempt.
//!
//! These resources are the single source of truth the browser and the loading
//! flow share, so UI code never re-reads the filesystem and gameplay systems
//! never have to re-parse a DAT file.

use std::path::PathBuf;

use bevy::prelude::*;

use crate::stage::catalog::StageCatalog;
use crate::stage::package::StagePackage;

/// The last catalog produced by [`StageCatalog::discover`].
///
/// Populated once when entering [`AppState::StageBrowser`] and refreshed only
/// by the browser's explicit rescan action, never per frame.
///
/// [`AppState::StageBrowser`]: crate::app::states::AppState::StageBrowser
#[derive(Resource, Default)]
pub struct StageCatalogResource(pub StageCatalog);

/// The stage currently highlighted in the browser.
///
/// `index` indexes [`StageCatalogResource::0`]`entries`; `dat_path` is the
/// resolved on-disk package path used when Play Stage is pressed.
#[derive(Resource, Default, Debug, Clone)]
pub struct StageSelection {
    pub index: Option<usize>,
    pub dat_path: Option<PathBuf>,
}

impl StageSelection {
    /// Points the selection at `index` and the given package path.
    pub fn select(&mut self, index: usize, dat_path: PathBuf) {
        self.index = Some(index);
        self.dat_path = Some(dat_path);
    }

    /// Clears the selection.
    pub fn clear(&mut self) {
        self.index = None;
        self.dat_path = None;
    }
}

/// The error from the most recent Play Stage load attempt, shown by the
/// browser so a bad package is recoverable instead of fatal.
#[derive(Resource, Default, Debug, Clone)]
pub struct StageLoadFailure(pub Option<String>);

/// The successfully loaded package, available to all later engine systems.
#[derive(Resource, Default)]
pub struct LoadedStage(pub Option<StagePackage>);
