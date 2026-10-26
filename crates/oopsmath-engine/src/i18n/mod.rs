//! Localization: language packs, Fluent messages, fonts and level translations.
//!
//! See `docs/i18n/README.md` for the player/translator guide. In short:
//!
//! * `assets/i18n/<lang>/` holds one language: `_meta.ron` (name, direction,
//!   fonts), `core.ftl` (all game strings, Fluent) and `levels/<id>.ftl`
//!   (strings of one stage). A `*.zip` with the same layout dropped into
//!   `assets/i18n/` works too (that is what `tools/oopsmath-i18n` builds).
//! * English (`en-US`) is the source: anything a language does not translate
//!   falls back to it, per message.
//! * [`I18n`] translates keys; [`LocalizedText`] keeps a UI text up to date
//!   across language changes; fonts follow the language ([`FontRole`],
//!   [`FontContext`], [`FontLibrary`]).
//! * Text built in code is translated through templates ([`templates`]).
//! * Level strings are merged into the stage message tables
//!   ([`crate::stage::localization::install_level_overlay`]).

pub mod catalog;
pub mod fonts;
pub mod meta;
pub mod pack;
pub mod plugin;
pub mod service;
pub mod templates;
pub mod translator;

pub use fonts::{FontContext, FontLibrary, FontRole};
pub use plugin::{I18nPlugin, LocalizedText};
pub use service::{I18n, LocArg};
