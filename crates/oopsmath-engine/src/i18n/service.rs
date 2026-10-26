//! [`I18n`]: the active language, message lookup with per-message English
//! fallback, and the language preference.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;

use bevy::prelude::Resource;
use fluent_bundle::{FluentArgs, FluentValue};

use super::catalog::{LanguageCatalog, SOURCE_LANGUAGE};
use super::meta::{Direction, FontSpec, LanguageMeta};
use super::templates::TemplateIndex;
use super::translator::{Translator, scan_single_line_messages};
use crate::stage::localization::LOCALE_ENV;

/// A value passed into a message (`{ $name }`).
#[derive(Debug, Clone, PartialEq)]
pub enum LocArg {
    Int(i64),
    Float(f64),
    Text(String),
}

impl LocArg {
    fn to_value(&self) -> FluentValue<'static> {
        match self {
            LocArg::Int(value) => FluentValue::from(*value),
            LocArg::Float(value) => FluentValue::from(*value),
            LocArg::Text(value) => FluentValue::String(Cow::Owned(value.clone())),
        }
    }
}

impl From<i64> for LocArg {
    fn from(value: i64) -> Self {
        LocArg::Int(value)
    }
}
impl From<i32> for LocArg {
    fn from(value: i32) -> Self {
        LocArg::Int(value.into())
    }
}
impl From<u32> for LocArg {
    fn from(value: u32) -> Self {
        LocArg::Int(value.into())
    }
}
impl From<f64> for LocArg {
    fn from(value: f64) -> Self {
        LocArg::Float(value)
    }
}
impl From<&str> for LocArg {
    fn from(value: &str) -> Self {
        LocArg::Text(value.to_string())
    }
}
impl From<String> for LocArg {
    fn from(value: String) -> Self {
        LocArg::Text(value)
    }
}

fn to_fluent_args(args: &[(String, LocArg)]) -> FluentArgs<'static> {
    let mut fluent_args = FluentArgs::new();
    for (name, value) in args {
        fluent_args.set(name.clone(), value.to_value());
    }
    fluent_args
}

/// Picks the installed language that best matches a request: an exact tag
/// (case-insensitive), then its base language (`fa-IR` -> `fa`), then an
/// installed regional variant of the request (`en` -> `en-US`).
pub fn resolve_language(requested: &str, available: &[&str]) -> Option<String> {
    let requested = requested.trim().replace('_', "-");
    if requested.is_empty() {
        return None;
    }
    if let Some(found) = available
        .iter()
        .find(|a| a.eq_ignore_ascii_case(&requested))
    {
        return Some((*found).to_string());
    }
    let base = requested.split('-').next().unwrap_or_default();
    if let Some(found) = available.iter().find(|a| a.eq_ignore_ascii_case(base)) {
        return Some((*found).to_string());
    }
    available
        .iter()
        .find(|a| {
            a.split('-')
                .next()
                .is_some_and(|b| b.eq_ignore_ascii_case(base))
        })
        .map(|a| (*a).to_string())
}

/// The language the player asked for: the environment variable, then the saved
/// preference, then English.
pub fn pick_requested(env: Option<String>, saved: Option<String>) -> String {
    env.filter(|v| !v.trim().is_empty())
        .or(saved.filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| SOURCE_LANGUAGE.to_string())
}

/// Where the language preference is kept (per user, outside the game folder).
pub fn config_dir(os: &str, get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    match os {
        "windows" => get("APPDATA").map(PathBuf::from),
        "macos" => {
            get("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
        }
        _ => get("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| get("HOME").map(|h| PathBuf::from(h).join(".config"))),
    }
}

fn preference_file() -> Option<PathBuf> {
    let get = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    config_dir(std::env::consts::OS, &get).map(|dir| dir.join("OopsMath").join("language.txt"))
}

pub fn load_saved_language() -> Option<String> {
    let text = std::fs::read_to_string(preference_file()?).ok()?;
    let code = text.trim();
    (!code.is_empty() && code.len() < 32).then(|| code.to_string())
}

pub fn save_language(code: &str) {
    let Some(path) = preference_file() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, code);
}

pub fn requested_language() -> String {
    pick_requested(std::env::var(LOCALE_ENV).ok(), load_saved_language())
}

fn core_sources(catalog: &LanguageCatalog, code: &str) -> Vec<String> {
    catalog
        .language(code)
        .map(|d| d.core.clone())
        .unwrap_or_default()
}

fn build_translator(catalog: &LanguageCatalog, code: &str) -> Translator {
    let sources = core_sources(catalog, code);
    let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
    let language = catalog
        .language(code)
        .map(|d| d.meta.langid())
        .unwrap_or_else(|| SOURCE_LANGUAGE.parse().unwrap_or_default());
    Translator::new(language, &refs)
}

/// The localization service.
#[derive(Resource)]
pub struct I18n {
    catalog: LanguageCatalog,
    active: String,
    active_translator: Translator,
    source_translator: Translator,
    /// English messages as templates, to translate text built in code.
    index: TemplateIndex,
    generation: u32,
}

impl I18n {
    pub fn new(catalog: LanguageCatalog, requested: &str) -> Self {
        let codes = catalog.codes();
        let active =
            resolve_language(requested, &codes).unwrap_or_else(|| SOURCE_LANGUAGE.to_string());
        let active_translator = build_translator(&catalog, &active);
        let source_translator = build_translator(&catalog, SOURCE_LANGUAGE);
        // Later sources override earlier ones, as in the translator.
        let mut english: BTreeMap<String, String> = BTreeMap::new();
        for source in core_sources(&catalog, SOURCE_LANGUAGE) {
            english.extend(scan_single_line_messages(&source));
        }
        let index = TemplateIndex::build(
            english
                .iter()
                .map(|(id, value)| (id.as_str(), value.as_str())),
        );
        Self {
            catalog,
            active,
            active_translator,
            source_translator,
            index,
            generation: 0,
        }
    }

    pub fn catalog(&self) -> &LanguageCatalog {
        &self.catalog
    }

    pub fn active_code(&self) -> &str {
        &self.active
    }

    /// Metadata of the active language (English metadata if it is missing).
    pub fn active_meta(&self) -> Option<&LanguageMeta> {
        self.catalog.language(&self.active).map(|d| &d.meta)
    }

    pub fn direction(&self) -> Direction {
        self.active_meta().map(|m| m.direction).unwrap_or_default()
    }

    pub fn font_spec(&self) -> Option<&FontSpec> {
        self.active_meta().map(|m| &m.fonts)
    }

    pub fn languages(&self) -> Vec<&LanguageMeta> {
        self.catalog.languages().map(|d| &d.meta).collect()
    }

    /// Increases whenever the language changes.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Switches language; returns whether it changed.
    pub fn set_language(&mut self, requested: &str) -> bool {
        let codes = self.catalog.codes();
        let Some(code) = resolve_language(requested, &codes) else {
            return false;
        };
        if code == self.active {
            return false;
        }
        self.active_translator = build_translator(&self.catalog, &code);
        self.active = code;
        self.generation += 1;
        true
    }

    /// Switches to the next installed language (wrapping); returns its code.
    pub fn cycle_language(&mut self) -> String {
        let codes: Vec<String> = self.catalog.codes().iter().map(|c| c.to_string()).collect();
        if let Some(index) = codes.iter().position(|c| *c == self.active) {
            let next = codes[(index + 1) % codes.len()].clone();
            self.set_language(&next);
        }
        self.active.clone()
    }

    /// Looks `key` up in the active language, then English, then returns the
    /// key itself so a missing message is visible but never blank.
    pub fn format(&self, key: &str, args: &[(String, LocArg)]) -> String {
        let fluent_args = to_fluent_args(args);
        let fluent_args = (!args.is_empty()).then_some(&fluent_args);
        self.active_translator
            .format(key, fluent_args)
            .or_else(|| self.source_translator.format(key, fluent_args))
            .unwrap_or_else(|| key.to_string())
    }

    pub fn tr(&self, key: &str) -> String {
        self.format(key, &[])
    }

    pub fn tr_args(&self, key: &str, args: &[(&str, LocArg)]) -> String {
        let owned: Vec<(String, LocArg)> = args
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        self.format(key, &owned)
    }

    /// Translates text the game built from English pieces (see
    /// [`super::templates`]), line by line. `None` when no line is recognised
    /// or the active language lacks the matching messages (the text is then
    /// left as it is).
    pub fn translate_text(&self, text: &str) -> Option<String> {
        let mut changed = false;
        let lines: Vec<String> = text
            .split('\n')
            .map(|line| match self.translate_line(line, 0) {
                Some(translated) => {
                    changed = true;
                    translated
                }
                None => line.to_string(),
            })
            .collect();
        changed.then(|| lines.join("\n"))
    }

    fn translate_line(&self, line: &str, depth: u8) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() || depth > 2 {
            return None;
        }
        let lead = &line[..line.len() - line.trim_start().len()];
        let trail = &line[line.trim_end().len()..];
        // A selection marker in front of a row stays in front.
        let (marker, body) = match trimmed.strip_prefix("> ") {
            Some(rest) => ("> ", rest),
            None => ("", trimmed),
        };
        let translated = self.translate_body(body, depth)?;
        Some(format!("{lead}{marker}{translated}{trail}"))
    }

    fn translate_body(&self, body: &str, depth: u8) -> Option<String> {
        if let Some(id) = self.index.exact(body) {
            return self.active_translator.format(id, None);
        }
        if let Some((id, variables)) = self.index.find(body) {
            let args: Vec<(String, LocArg)> = variables
                .into_iter()
                .map(|(name, value)| {
                    let arg = self.translate_value(&value, depth + 1);
                    (name, arg)
                })
                .collect();
            return self
                .active_translator
                .format(id, Some(&to_fluent_args(&args)));
        }
        // "Label: value" lines (details panels): translate a known label.
        let (label, value) = body.split_once(": ")?;
        let id = self.index.exact(label.trim())?;
        let label = self.active_translator.format(id, None)?;
        let value = match self.translate_value(value, depth + 1) {
            LocArg::Text(text) => text,
            LocArg::Int(n) => n.to_string(),
            LocArg::Float(n) => n.to_string(),
        };
        Some(format!("{label}: {value}"))
    }

    /// A captured value as a message argument: numbers stay numbers, English
    /// messages, templates and block / tool ids are translated.
    fn translate_value(&self, value: &str, depth: u8) -> LocArg {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return LocArg::Text(String::new());
        }
        if let Ok(n) = trimmed.parse::<i64>() {
            return LocArg::Int(n);
        }
        if let Ok(n) = trimmed.parse::<f64>() {
            return LocArg::Float(n);
        }
        if let Some(id) = self.index.exact(trimmed)
            && let Some(text) = self.active_translator.format(id, None)
        {
            return LocArg::Text(text);
        }
        if let Some(name) = self.name_of_id(trimmed) {
            return LocArg::Text(name);
        }
        if let Some(text) = self.translate_line(trimmed, depth) {
            return LocArg::Text(text);
        }
        LocArg::Text(trimmed.to_string())
    }

    /// The localized name of a block / tool id such as `block.wood_log`
    /// (message `block-wood_log`) or `hammer` (message `tool-hammer`).
    fn name_of_id(&self, id: &str) -> Option<String> {
        let valid = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
        if !valid {
            return None;
        }
        let key = if id.starts_with("block.") {
            id.replace('.', "-")
        } else {
            format!("tool-{}", id.replace('.', "-"))
        };
        self.active_translator.format(&key, None)
    }

    pub fn diagnostics(&self) -> impl Iterator<Item = &str> {
        self.active_translator
            .diagnostics
            .iter()
            .chain(self.source_translator.diagnostics.iter())
            .map(String::as_str)
            .chain(self.catalog.diagnostics.iter().map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::pack::PackFiles;

    fn pack(files: &[(&str, &str)]) -> PackFiles {
        PackFiles {
            origin: "test".into(),
            files: files
                .iter()
                .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
                .collect(),
        }
    }

    fn meta(code: &str, direction: &str) -> String {
        format!(
            r#"(format: 1, code: "{code}", name: "N", native_name: "N", direction: {direction})"#
        )
    }

    fn service(requested: &str) -> I18n {
        let catalog = LanguageCatalog::from_packs(&[pack(&[
            ("en-US/_meta.ron", &meta("en-US", "Ltr")),
            (
                "en-US/core.ftl",
                "play = PLAY\nbuy = Buy { $n } blocks\nonly-en = Only English\nback = Back",
            ),
            ("fa/_meta.ron", &meta("fa", "Rtl")),
            ("fa/core.ftl", "play = شروع\nbuy = { $n } بلوک بخر"),
        ])]);
        I18n::new(catalog, requested)
    }

    #[test]
    fn language_requests_resolve_to_installed_languages() {
        let a = ["en-US", "fa"];
        assert_eq!(resolve_language("fa", &a).as_deref(), Some("fa"));
        assert_eq!(resolve_language("FA", &a).as_deref(), Some("fa"));
        assert_eq!(resolve_language("fa-IR", &a).as_deref(), Some("fa"));
        assert_eq!(resolve_language("fa_IR", &a).as_deref(), Some("fa"));
        assert_eq!(resolve_language("en", &a).as_deref(), Some("en-US"));
        assert_eq!(resolve_language("en-GB", &a).as_deref(), Some("en-US"));
        assert_eq!(resolve_language("de", &a), None);
        assert_eq!(resolve_language("  ", &a), None);
    }

    #[test]
    fn the_requested_language_prefers_env_then_saved_then_english() {
        assert_eq!(pick_requested(Some("fa".into()), Some("de".into())), "fa");
        assert_eq!(pick_requested(Some(" ".into()), Some("de".into())), "de");
        assert_eq!(pick_requested(None, None), "en-US");
    }

    #[test]
    fn the_config_folder_depends_on_the_platform() {
        let env = |k: &str| match k {
            "APPDATA" => Some("C:/Users/x/AppData/Roaming".to_string()),
            "HOME" => Some("/home/x".to_string()),
            _ => None,
        };
        assert_eq!(
            config_dir("windows", &env),
            Some(PathBuf::from("C:/Users/x/AppData/Roaming"))
        );
        assert_eq!(
            config_dir("linux", &env),
            Some(PathBuf::from("/home/x/.config"))
        );
        assert_eq!(
            config_dir("macos", &env),
            Some(PathBuf::from("/home/x/Library/Application Support"))
        );
        assert_eq!(config_dir("linux", &|_| None), None);
    }

    #[test]
    fn messages_fall_back_to_english_per_message_then_to_the_key() {
        let i18n = service("fa");
        assert_eq!(i18n.active_code(), "fa");
        assert_eq!(i18n.tr("play"), "شروع");
        assert_eq!(
            i18n.tr("only-en"),
            "Only English",
            "untranslated -> English"
        );
        assert_eq!(i18n.tr("nowhere"), "nowhere", "unknown -> the key");
        assert_eq!(i18n.tr_args("buy", &[("n", 3.into())]), "3 بلوک بخر");
        assert_eq!(
            service("en-US").tr_args("buy", &[("n", 3.into())]),
            "Buy 3 blocks"
        );
        assert_eq!(
            service("xx").active_code(),
            "en-US",
            "unknown language -> English"
        );
    }

    #[test]
    fn switching_and_cycling_languages() {
        let mut i18n = service("en-US");
        assert_eq!(i18n.direction(), Direction::Ltr);
        assert!(i18n.set_language("fa"));
        assert_eq!(i18n.direction(), Direction::Rtl);
        assert_eq!(i18n.generation(), 1);
        assert!(!i18n.set_language("fa"), "no change");
        assert!(!i18n.set_language("de"), "not installed");
        assert_eq!(i18n.cycle_language(), "en-US");
        assert_eq!(i18n.cycle_language(), "fa");
        assert_eq!(i18n.languages().len(), 2);
    }

    fn templates_service(requested: &str) -> I18n {
        let en = "hud-money = Money: { $amount } coins\n\
                  hud-placed = Placed: { $placed }\n\
                  hud-placed-max = Placed: { $placed } / { $max }\n\
                  hud-none = You have no { $name } in your inventory.\n\
                  block-wood_log = Wood Log\n\
                  tool-hammer = Hammer\n\
                  label-level = Level\n\
                  kind-tool = Tool\n\
                  hotbar-row = [{ $kind }] { $name } x{ $quantity }\n\
                  play = PLAY\n\
                  back = Back";
        let fa = "hud-money = پول: { $amount } سکه\n\
                  hud-placed-max = گذاشته: { $placed } از { $max }\n\
                  hud-none = در انبار { $name } ندارید.\n\
                  block-wood_log = کنده چوب\n\
                  tool-hammer = چکش\n\
                  label-level = مرحله\n\
                  kind-tool = ابزار\n\
                  hotbar-row = [{ $kind }] { $name } ×{ $quantity }\n\
                  play = شروع";
        let catalog = LanguageCatalog::from_packs(&[pack(&[
            ("en-US/_meta.ron", &meta("en-US", "Ltr")),
            ("en-US/core.ftl", en),
            ("fa/_meta.ron", &meta("fa", "Rtl")),
            ("fa/core.ftl", fa),
        ])]);
        I18n::new(catalog, requested)
    }

    #[test]
    fn text_built_in_code_is_translated_line_by_line() {
        let i18n = templates_service("fa");
        assert_eq!(
            i18n.translate_text("Money: 25 coins").as_deref(),
            Some("پول: 25 سکه")
        );
        assert_eq!(
            i18n.translate_text("In inventory: x\nPlaced: 3 / 20")
                .as_deref(),
            Some("In inventory: x\nگذاشته: 3 از 20"),
            "a line that is not a known message is left alone"
        );
        assert_eq!(
            i18n.translate_text("Money: 7 coins\nPlaced: 3 / 20")
                .as_deref(),
            Some("پول: 7 سکه\nگذاشته: 3 از 20")
        );
    }

    #[test]
    fn names_ids_and_kinds_inside_a_line_are_translated() {
        let i18n = templates_service("fa");
        assert_eq!(
            i18n.translate_text("You have no Wood Log in your inventory.")
                .as_deref(),
            Some("در انبار کنده چوب ندارید.")
        );
        assert_eq!(
            i18n.translate_text("You have no block.wood_log in your inventory.")
                .as_deref(),
            Some("در انبار کنده چوب ندارید."),
            "ids become names"
        );
        assert_eq!(
            i18n.translate_text("> [Tool] Hammer x1").as_deref(),
            Some("> [ابزار] چکش ×1"),
            "the selection marker stays in front"
        );
        assert_eq!(
            i18n.translate_text("Level: 3").as_deref(),
            Some("مرحله: 3"),
            "Label: value lines"
        );
    }

    #[test]
    fn messages_the_language_lacks_are_left_in_english() {
        let i18n = templates_service("fa");
        assert_eq!(
            i18n.translate_text("Placed: 3"),
            None,
            "fa has no hud-placed"
        );
        assert_eq!(i18n.translate_text("Back"), None, "fa has no back");
        assert_eq!(i18n.translate_text("PLAY").as_deref(), Some("شروع"));
        assert_eq!(i18n.translate_text("Something unknown"), None);
    }

    #[test]
    fn english_is_a_fixed_point() {
        let i18n = templates_service("en-US");
        for text in ["Money: 25 coins", "> [Tool] Hammer x1", "Level: 3", "PLAY"] {
            let out = i18n
                .translate_text(text)
                .unwrap_or_else(|| text.to_string());
            assert_eq!(out, text);
        }
    }
}
