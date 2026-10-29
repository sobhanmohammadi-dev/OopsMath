//! Loading, readiness and the single playback route for sound effects.

use std::collections::{HashMap, HashSet};

use bevy::asset::LoadState;
use bevy::audio::Volume;
use bevy::prelude::*;

use crate::app::states::AppState;
use crate::blocks::PlacedWorld;
use crate::challenges::{ChallengePanel, ChallengeTracker, StageChallenges};
use crate::items::economy::Wallet;
use crate::items::shop::ShopState;
use crate::objectives::ObjectiveTracker;
use crate::structure::FallAnimations;

use super::catalog::{Sfx, SfxVolumes};
use super::edges::{Observed, SfxEdges};

/// Request to play a sound effect once. The one route to the speakers: gameplay
/// code (or the edge observer) writes it, [`play_sfx`] reads it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaySfx(pub Sfx);

/// Marks the entities of playing sounds (they despawn themselves when done).
#[derive(Component)]
pub struct SfxPlayback;

/// Per-sound repetition guard.
#[derive(Debug, Default)]
pub struct Cooldowns {
    last: HashMap<Sfx, f32>,
}

impl Cooldowns {
    /// Whether `sfx` may play at `now` seconds; records the play when it may.
    pub fn allow(&mut self, sfx: Sfx, now: f32) -> bool {
        let interval = sfx.min_interval().as_secs_f32();
        let ok = self.last.get(&sfx).is_none_or(|last| now - last >= interval);
        if ok {
            self.last.insert(sfx, now);
        }
        ok
    }
}

/// The loaded sounds. Every file is requested once, at startup; the handles are
/// reused for every play.
#[derive(Resource, Default)]
pub struct SfxLibrary {
    handles: HashMap<Sfx, Handle<AudioSource>>,
    cooldowns: Cooldowns,
    summary_logged: bool,
    skipped: HashSet<Sfx>,
}

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySfx>()
            .init_resource::<SfxVolumes>()
            .init_resource::<SfxLibrary>()
            .init_resource::<SfxEdges>()
            .add_systems(Startup, load_sfx)
            .add_systems(OnEnter(AppState::InGame), reset_edges)
            .add_systems(
                Last,
                (
                    observe_gameplay.run_if(in_state(AppState::InGame)),
                    play_sfx,
                    report_sfx_assets,
                )
                    .chain(),
            );
    }
}

/// Requests every catalog file once. A missing file is not an error here; it is
/// reported once by [`report_sfx_assets`] and its sound is skipped.
fn load_sfx(mut library: ResMut<SfxLibrary>, assets: Option<Res<AssetServer>>) {
    let Some(assets) = assets else {
        return;
    };
    for sfx in Sfx::ALL {
        let handle: Handle<AudioSource> = assets.load(sfx.path());
        library.handles.insert(sfx, handle);
    }
}

/// Logs, once, which sound files loaded and which are missing, as soon as every
/// load has finished. Never runs again afterwards, so nothing is logged per frame.
fn report_sfx_assets(assets: Option<Res<AssetServer>>, mut library: ResMut<SfxLibrary>) {
    if library.summary_logged {
        return;
    }
    let Some(assets) = assets else {
        return;
    };
    let mut loaded = Vec::new();
    let mut missing = Vec::new();
    for sfx in Sfx::ALL {
        match library.handles.get(&sfx).map(|h| assets.load_state(h.id())) {
            Some(LoadState::Loaded) => loaded.push(sfx),
            Some(LoadState::Failed(_)) | None => missing.push(sfx),
            Some(LoadState::NotLoaded | LoadState::Loading) => return,
        }
    }
    library.summary_logged = true;
    if missing.is_empty() {
        info!("audio: all {} sound effects loaded from assets/{}", loaded.len(), super::catalog::SFX_DIR);
    } else {
        let list: Vec<String> = missing
            .iter()
            .map(|s| format!("{} ({})", s.path(), s.purpose()))
            .collect();
        warn!(
            "audio: {} of {} sound effects are not available and will be silent: {}",
            missing.len(),
            Sfx::ALL.len(),
            list.join(", ")
        );
    }
}

/// A new stage run starts: forget the previous state so the first frame only
/// records a baseline and never plays a sound.
fn reset_edges(mut edges: ResMut<SfxEdges>) {
    edges.reset();
}

/// Watches the committed game state and requests the sounds of real actions
/// (see [`super::edges`]). It only reads game state.
#[allow(clippy::too_many_arguments)]
fn observe_gameplay(
    placed: Res<PlacedWorld>,
    wallet: Res<Wallet>,
    shop: Res<ShopState>,
    panel: Res<ChallengePanel>,
    challenges: Res<StageChallenges>,
    tracker: Res<ChallengeTracker>,
    falling: Res<FallAnimations>,
    objectives: Res<ObjectiveTracker>,
    mut edges: ResMut<SfxEdges>,
    mut requests: MessageWriter<PlaySfx>,
) {
    let solved = (0..challenges.challenges.len())
        .filter(|index| tracker.is_solved_at(*index))
        .count();
    let observed = Observed {
        placed: placed.len(),
        balance: wallet.balance(),
        shop: &shop,
        shop_changed: shop.is_changed(),
        panel: &panel,
        panel_changed: panel.is_changed(),
        solved,
        falling: falling.len(),
        complete: objectives.all_complete(),
    };
    for sfx in edges.observe(&observed) {
        requests.write(PlaySfx(sfx));
    }
}

/// Plays the requested sounds: each distinct sound at most once per frame, only
/// when its file is loaded, and subject to its own cooldown. One-shot entities
/// despawn when the sound ends; other playing sounds are never touched.
fn play_sfx(
    mut commands: Commands,
    mut requests: MessageReader<PlaySfx>,
    assets: Option<Res<AssetServer>>,
    volumes: Res<SfxVolumes>,
    time: Res<Time>,
    mut library: ResMut<SfxLibrary>,
) {
    let mut wanted: Vec<Sfx> = Vec::new();
    for PlaySfx(sfx) in requests.read() {
        if !wanted.contains(sfx) {
            wanted.push(*sfx);
        }
    }
    if wanted.is_empty() {
        return;
    }
    let Some(assets) = assets else {
        return;
    };
    let now = time.elapsed_secs();
    for sfx in wanted {
        let Some(handle) = library.handles.get(&sfx).cloned() else {
            continue;
        };
        if !matches!(assets.load_state(handle.id()), LoadState::Loaded) {
            // Missing or still loading: skip quietly (the summary already named
            // missing files); note it once for development.
            if library.skipped.insert(sfx) {
                debug!("audio: {} is not ready; skipping its sound", sfx.path());
            }
            continue;
        }
        if !library.cooldowns.allow(sfx, now) {
            continue;
        }
        commands.spawn((
            AudioPlayer(handle),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volumes.gain(sfx))),
            SfxPlayback,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn cooldowns_only_guard_the_bursty_sounds() {
        let mut cooldowns = Cooldowns::default();
        // Ordinary actions are never suppressed.
        for i in 0..20 {
            assert!(cooldowns.allow(Sfx::BlockPlace, i as f32 * 0.001));
        }
        // A collapse sound is not repeated within its interval, but is after it.
        assert!(cooldowns.allow(Sfx::StructureCollapse, 10.0));
        assert!(!cooldowns.allow(Sfx::StructureCollapse, 10.2));
        let interval = Sfx::StructureCollapse.min_interval();
        assert!(cooldowns.allow(Sfx::StructureCollapse, 10.0 + interval.as_secs_f32()));
        // Cooldowns are per sound.
        assert!(cooldowns.allow(Sfx::ActionDenied, 10.2));
        assert!(!cooldowns.allow(Sfx::ActionDenied, 10.2 + 0.01));
        assert!(Sfx::ActionDenied.min_interval() >= Duration::from_millis(100));
    }

    #[test]
    fn a_refused_request_does_not_start_the_cooldown() {
        let mut cooldowns = Cooldowns::default();
        assert!(cooldowns.allow(Sfx::ActionDenied, 1.0));
        assert!(!cooldowns.allow(Sfx::ActionDenied, 1.05));
        // The refused request at 1.05 did not extend the guard past 1.0 + interval.
        assert!(cooldowns.allow(Sfx::ActionDenied, 1.0 + Sfx::ActionDenied.min_interval().as_secs_f32()));
    }
}
