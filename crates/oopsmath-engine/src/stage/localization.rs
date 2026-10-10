//! Stage-local message resolution.
//!
//! Every compiled stage ships one or more Fluent (`.ftl`) files inside its
//! `LOCL` section, keyed by locale. This module parses the subset of FTL the
//! OopsMath compiler emits and resolves message keys against the app locale
//! with a predictable fallback chain.
//!
//! ## Supported syntax
//!
//! Only the simple `key = value` form is supported, matching what the compiler
//! writes today:
//!
//! ```ftl
//! # a comment
//! stage.001_first_wall.title = My first stage
//! ```
//!
//! Message identifiers may contain dots. Keys are read verbatim.
//!
//! ## Not supported
//!
//! Full Fluent features (attributes, placeables, multi-line values, terms,
//! select expressions, references) are **not** interpreted. A line without a
//! top-level `=` is ignored rather than guessed at, so a future construct is
//! never silently misread as a plain message. This is intentionally *not*
//! full Fluent compatibility.
//!
//! ## Robustness
//!
//! Comments (`#`), blank lines, leading/trailing whitespace and both `\n` and
//! `\r\n` line endings are handled. Duplicate message identifiers follow the
//! Fluent rule: the last definition wins. Invalid UTF-8 is decoded lossily so
//! a broken byte can never panic the game.

use std::collections::BTreeMap;

use bevy::prelude::Resource;

use crate::stage::package::Localization;

/// Locale used when the requested locale has no message for a key.
pub const DEFAULT_FALLBACK_LOCALE: &str = "en-US";

/// Environment variable overriding the app locale at startup.
pub const LOCALE_ENV: &str = "OOPSMATH_LOCALE";

/// The parsed messages of a single locale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FluentMessages {
    messages: BTreeMap<String, String>,
}

impl FluentMessages {
    /// Parses the simple `key = value` FTL subset from raw bytes.
    ///
    /// Never fails: malformed or unsupported lines are skipped.
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let mut messages = BTreeMap::new();
        for raw_line in text.lines() {
            let line = raw_line.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                // Continuations and unsupported constructs are dropped, not
                // reinterpreted as a message with an empty key.
                continue;
            };
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            // Duplicate identifiers: the last definition wins, like Fluent.
            messages.insert(key.to_string(), value.trim().to_string());
        }
        Self { messages }
    }

    /// The message for `key`, if this locale defines it.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.messages.get(key).map(String::as_str)
    }

    /// Number of parsed messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether this locale defined no messages.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Iterates over the parsed `(key, value)` pairs in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.messages
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }
}

/// All localized messages of one stage, grouped by locale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageMessages {
    by_locale: BTreeMap<String, FluentMessages>,
}

impl StageMessages {
    /// Parses every locale in a package's `localization` list.
    ///
    /// When the loader reports the same locale twice (which DAT v1 forbids),
    /// the last entry wins.
    pub fn from_localizations(localizations: &[Localization]) -> Self {
        let mut by_locale = BTreeMap::new();
        for localization in localizations {
            by_locale.insert(
                localization.locale.clone(),
                FluentMessages::parse(&localization.raw),
            );
        }
        Self { by_locale }
    }

    /// The locales present, sorted.
    pub fn locales(&self) -> impl Iterator<Item = &str> {
        self.by_locale.keys().map(String::as_str)
    }

    /// Whether `locale` has a parsed message table.
    pub fn has_locale(&self, locale: &str) -> bool {
        self.by_locale.contains_key(locale)
    }

    /// Resolves `key` for `locale`, falling back to `fallback`.
    ///
    /// The candidate order is: the requested locale, its base language (for
    /// example `fa-IR` -> `fa`), the fallback locale, then the fallback base
    /// language. Returns `None` when no candidate defines `key`.
    pub fn resolve(&self, key: &str, locale: &str, fallback: &str) -> Option<&str> {
        for candidate in locale_chain(locale, fallback) {
            if let Some(message) = self.by_locale.get(&candidate).and_then(|m| m.get(key)) {
                return Some(message);
            }
        }
        None
    }

    /// Like [`Self::resolve`], but returns `key` itself when no translation
    /// exists so the UI never blanks out or crashes on a missing message.
    pub fn resolve_or<'a>(&'a self, key: &'a str, locale: &str, fallback: &str) -> &'a str {
        self.resolve(key, locale, fallback).unwrap_or(key)
    }
}

/// Builds the ordered candidate locale list for a resolution attempt.
fn locale_chain(locale: &str, fallback: &str) -> Vec<String> {
    let mut chain = Vec::new();
    push_locale(&mut chain, locale);
    push_locale(&mut chain, fallback);
    chain
}

fn push_locale(chain: &mut Vec<String>, locale: &str) {
    if locale.is_empty() {
        return;
    }
    let mut push = |candidate: String| {
        if !chain.iter().any(|existing| existing == &candidate) {
            chain.push(candidate);
        }
    };
    push(locale.to_string());
    if let Some((base, _region)) = locale
        .split_once(['-', '_'])
        .filter(|(base, _)| !base.is_empty())
    {
        push(base.to_string());
    }
}

/// Central app locale setting used to render stage text.
///
/// One resource owns the locale choice so the catalog, the browser and details
/// panel never disagree about which language to show.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct StageLocale {
    /// The locale requested by the player, e.g. `"fa"`.
    pub locale: String,
    /// The locale to use when `locale` lacks a message.
    pub fallback: String,
}

impl Default for StageLocale {
    fn default() -> Self {
        let locale = std::env::var(LOCALE_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_FALLBACK_LOCALE.to_string());
        Self {
            locale,
            fallback: DEFAULT_FALLBACK_LOCALE.to_string(),
        }
    }
}

impl From<&str> for StageLocale {
    fn from(locale: &str) -> Self {
        Self {
            locale: locale.to_string(),
            fallback: DEFAULT_FALLBACK_LOCALE.to_string(),
        }
    }
}

impl StageLocale {
    /// Resolves `key` using this locale's fallback chain, returning `key` when
    /// no translation exists.
    pub fn resolve<'a>(&self, messages: &'a StageMessages, key: &'a str) -> &'a str {
        messages.resolve_or(key, &self.locale, &self.fallback)
    }
}
