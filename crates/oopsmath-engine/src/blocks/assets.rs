//! Bevy asset integration: one scene handle per registered block.
//!
//! Models are loaded once at startup through the normal `AssetServer` (asset
//! root = repository `assets/`, see `crates/oopsmath-game/src/main.rs`) and the
//! handles are reused for every instance. This is intentionally separate from
//! the DAT stage-package loader.
//!
//! Bevy 0.19 renamed `Scene`/`SceneRoot` to `WorldAsset`/`WorldAssetRoot`.

use std::collections::HashMap;

use bevy::asset::LoadState;
use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;
use bevy::world_serialization::WorldAsset;

use super::definition::BlockId;
use super::error::BlockAssetError;
use super::registry::BlockRegistry;

pub type BlockSceneHandle = Handle<WorldAsset>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockAssetState {
    Loading,
    Ready,
    Failed(String),
}

#[derive(Debug)]
struct Entry {
    path: String,
    handle: BlockSceneHandle,
    state: BlockAssetState,
}

#[derive(Resource, Default, Debug)]
pub struct BlockAssets {
    entries: HashMap<BlockId, Entry>,
}

impl BlockAssets {
    pub fn insert(&mut self, id: BlockId, path: String, handle: BlockSceneHandle) {
        self.entries.insert(
            id,
            Entry {
                path,
                handle,
                state: BlockAssetState::Loading,
            },
        );
    }

    pub fn state(&self, id: &BlockId) -> Option<&BlockAssetState> {
        self.entries.get(id).map(|e| &e.state)
    }

    pub fn path(&self, id: &BlockId) -> Option<&str> {
        self.entries.get(id).map(|e| e.path.as_str())
    }

    /// The scene handle, only once the model has finished loading.
    pub fn ready_handle(&self, id: &BlockId) -> Result<&BlockSceneHandle, BlockAssetError> {
        let entry = self
            .entries
            .get(id)
            .ok_or_else(|| BlockAssetError::NotRequested(id.clone()))?;
        match &entry.state {
            BlockAssetState::Ready => Ok(&entry.handle),
            BlockAssetState::Loading => Err(BlockAssetError::Pending {
                id: id.clone(),
                path: entry.path.clone(),
            }),
            BlockAssetState::Failed(reason) => Err(BlockAssetError::LoadFailed {
                id: id.clone(),
                path: entry.path.clone(),
                reason: reason.clone(),
            }),
        }
    }

    pub fn has_pending(&self) -> bool {
        self.entries
            .values()
            .any(|e| e.state == BlockAssetState::Loading)
    }

    /// Test/support hook: marks an entry as ready without an AssetServer.
    pub fn mark_ready(&mut self, id: &BlockId) {
        if let Some(e) = self.entries.get_mut(id) {
            e.state = BlockAssetState::Ready;
        }
    }
}

/// Requires `AssetPlugin` (part of `DefaultPlugins`) and [`super::BlocksPlugin`].
pub struct BlockAssetsPlugin;

impl Plugin for BlockAssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockAssets>()
            .add_systems(Startup, request_block_assets)
            .add_systems(
                Update,
                poll_block_assets.run_if(|assets: Res<BlockAssets>| assets.has_pending()),
            );
    }
}

fn request_block_assets(
    registry: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut assets: ResMut<BlockAssets>,
) {
    for def in registry.iter() {
        let label = GltfAssetLabel::Scene(def.model.scene_index).from_asset(def.model.path.clone());
        let handle: BlockSceneHandle = asset_server.load(label);
        assets.insert(def.id.clone(), def.model.path.clone(), handle);
    }
}

fn poll_block_assets(asset_server: Res<AssetServer>, mut assets: ResMut<BlockAssets>) {
    // Only touch the resource mutably when something actually changes.
    let mut updates = Vec::new();
    for (id, entry) in &assets.entries {
        if entry.state != BlockAssetState::Loading {
            continue;
        }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => updates.push((id.clone(), BlockAssetState::Ready)),
            LoadState::Failed(err) => {
                error!(
                    "Block model '{}' for '{id}' failed to load: {err}",
                    entry.path
                );
                updates.push((id.clone(), BlockAssetState::Failed(err.to_string())));
            }
            LoadState::NotLoaded | LoadState::Loading => {}
        }
    }
    for (id, state) in updates {
        if let Some(entry) = assets.entries.get_mut(&id) {
            entry.state = state;
        }
    }
}
