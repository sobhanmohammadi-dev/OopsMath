//! Sound effects (SFX).
//!
//! ```text
//! committed gameplay state ──► edges::SfxEdges ──► PlaySfx message ──► plugin::play_sfx ──► Bevy audio
//!   (PlacedWorld, Wallet, …)    (pure, tested)      (one route)         (catalog, policy)
//! ```
//!
//! * [`catalog`] - the typed list of sounds ([`Sfx`]), their OGG paths under
//!   `assets/audio/sfx/`, groups, levels and repetition policy. One place.
//! * [`edges`] - pure logic that turns changes of the *authoritative* game state
//!   into sound requests. Sounds follow committed actions, never UI refreshes,
//!   previews or per-frame conditions, and no gameplay code had to change.
//! * [`plugin`] - loading (once, at startup), readiness, the single playback
//!   route ([`PlaySfx`]), per-sound cooldowns, and diagnostics that are logged
//!   once.
//!
//! Other code can request a sound with `MessageWriter<PlaySfx>`; the audio
//! system plays it if the file is available and silently skips it otherwise.
//! See `docs/audio/README.md`.

pub mod catalog;
pub mod edges;
pub mod plugin;

pub use catalog::{Sfx, SfxGroup, SfxVolumes};
pub use plugin::{PlaySfx, SfxLibrary, SfxPlugin};
