//! The set of installed languages, merged from folders and zip packs.
//!
//! Merge rules (packs are processed in order: folders, then zips by name):
//!
//! * `_meta.ron`: the first valid one for a language wins;
//! * `core.ftl`: kept as an ordered list; a later pack overrides per message;
//! * `levels/<id>.ftl`: concatenated, a later pack overrides per message;
//! * fonts: the first file for an id wins.
//!
//! A language without a valid `_meta.ron` is ignored with a diagnostic.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::stage::localization::FluentMessages;

use super::fonts::id_from_stem;
use super::meta::{LanguageMeta, META_FILE};
use super::pack::PackFiles;

pub const SOURCE_LANGUAGE: &str = "en-US";

#[derive(Debug, Clone)]
pub struct LanguageData {
    pub meta: LanguageMeta,
    /// Fluent sources of `core.ftl`, in override order.
    pub core: Vec<String>,
    /// Level id -> concatenated simple-FTL text.
    pub levels: BTreeMap<String, String>,
    /// Font id -> bytes, from `fonts/` inside the pack.
    pub fonts: BTreeMap<String, Arc<Vec<u8>>>,
    pub origins: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct LanguageCatalog {
    languages: BTreeMap<String, LanguageData>,
    pub diagnostics: Vec<String>,
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_start_matches('\u{feff}')
        .to_string()
}

impl LanguageCatalog {
    pub fn from_packs(packs: &[PackFiles]) -> Self {
        let mut catalog = Self::default();
        // Pass 1: metadata.
        let mut metas: BTreeMap<String, LanguageMeta> = BTreeMap::new();
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        for pack in packs {
            for (key, bytes) in &pack.files {
                let Some((lang, rest)) = key.split_once('/') else {
                    continue;
                };
                seen.entry(lang.to_string())
                    .or_insert_with(|| pack.origin.clone());
                if rest != META_FILE || metas.contains_key(lang) {
                    continue;
                }
                match LanguageMeta::parse(&text(bytes)) {
                    Ok(meta) if meta.code == lang => {
                        metas.insert(lang.to_string(), meta);
                    }
                    Ok(meta) => catalog.diagnostics.push(format!(
                        "{}: {key}: code '{}' does not match the folder name '{lang}'",
                        pack.origin, meta.code
                    )),
                    Err(err) => catalog
                        .diagnostics
                        .push(format!("{}: {key}: {err}", pack.origin)),
                }
            }
        }
        for lang in seen.keys().filter(|lang| !metas.contains_key(*lang)) {
            catalog.diagnostics.push(format!(
                "language '{lang}' has no valid {META_FILE}; ignored"
            ));
        }
        // Pass 2: content.
        for (lang, meta) in metas {
            catalog.languages.insert(
                lang,
                LanguageData {
                    meta,
                    core: Vec::new(),
                    levels: BTreeMap::new(),
                    fonts: BTreeMap::new(),
                    origins: Vec::new(),
                },
            );
        }
        for pack in packs {
            for (key, bytes) in &pack.files {
                let parts: Vec<&str> = key.split('/').collect();
                let Some(data) = parts
                    .first()
                    .and_then(|lang| catalog.languages.get_mut(*lang))
                else {
                    continue;
                };
                if !data.origins.contains(&pack.origin) {
                    data.origins.push(pack.origin.clone());
                }
                match parts.as_slice() {
                    [_, "core.ftl"] => data.core.push(text(bytes)),
                    [_, "levels", file] => {
                        if let Some(id) = file.strip_suffix(".ftl") {
                            let entry = data.levels.entry(id.to_string()).or_default();
                            entry.push('\n');
                            entry.push_str(&text(bytes));
                        }
                    }
                    [_, "fonts", file] => {
                        let stem = file.rsplit_once('.').map_or(*file, |(stem, _)| stem);
                        data.fonts
                            .entry(id_from_stem(stem))
                            .or_insert_with(|| Arc::new(bytes.clone()));
                    }
                    _ => {}
                }
            }
        }
        catalog
    }

    pub fn language(&self, code: &str) -> Option<&LanguageData> {
        self.languages.get(code)
    }

    pub fn codes(&self) -> Vec<&str> {
        self.languages.keys().map(String::as_str).collect()
    }

    pub fn languages(&self) -> impl Iterator<Item = &LanguageData> {
        self.languages.values()
    }

    /// Every language's level strings, merged per language, for the stage
    /// message tables.
    pub fn level_overlay(&self) -> BTreeMap<String, FluentMessages> {
        let mut overlay = BTreeMap::new();
        for (code, data) in &self.languages {
            let mut messages = FluentMessages::default();
            for text in data.levels.values() {
                messages.merge(&FluentMessages::parse(text.as_bytes()));
            }
            if !messages.is_empty() {
                overlay.insert(code.clone(), messages);
            }
        }
        overlay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(origin: &str, files: &[(&str, &str)]) -> PackFiles {
        PackFiles {
            origin: origin.to_string(),
            files: files
                .iter()
                .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
                .collect(),
        }
    }

    fn meta(code: &str) -> String {
        format!(r#"(format: 1, code: "{code}", name: "N", native_name: "N")"#)
    }

    #[test]
    fn languages_are_merged_across_packs() {
        let catalog = LanguageCatalog::from_packs(&[
            pack(
                "folder",
                &[("fa/_meta.ron", &meta("fa")), ("fa/core.ftl", "a = 1")],
            ),
            pack(
                "update.zip",
                &[
                    ("fa/core.ftl", "a = 2"),
                    ("fa/levels/s1.ftl", "stage.s1.title = T"),
                    ("fa/fonts/Vazir-Bold.ttf", "bytes"),
                ],
            ),
        ]);
        let fa = catalog.language("fa").unwrap();
        assert_eq!(fa.core, ["a = 1", "a = 2"]);
        assert!(fa.levels["s1"].contains("stage.s1.title = T"));
        assert!(fa.fonts.contains_key("font.vazirbold"));
        assert_eq!(fa.origins, ["folder", "update.zip"]);
        assert!(catalog.diagnostics.is_empty());
        assert_eq!(
            catalog.level_overlay()["fa"].get("stage.s1.title"),
            Some("T")
        );
    }

    #[test]
    fn the_first_valid_meta_wins_and_bad_ones_are_reported() {
        let catalog = LanguageCatalog::from_packs(&[
            pack("a", &[("fa/_meta.ron", "(broken")]),
            pack("b", &[("fa/_meta.ron", &meta("fa"))]),
            pack(
                "c",
                &[("de/_meta.ron", &meta("fr")), ("it/core.ftl", "x = y")],
            ),
        ]);
        assert_eq!(catalog.codes(), ["fa"]);
        let all = catalog.diagnostics.join("\n");
        assert!(all.contains("does not match the folder name"), "{all}");
        assert!(all.contains("language 'it' has no valid"), "{all}");
        assert!(all.contains("language 'de' has no valid"), "{all}");
    }
}
