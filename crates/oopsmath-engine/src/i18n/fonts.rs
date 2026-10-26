//! Fonts: ids, roles and the library that turns an id into a Bevy font.
//!
//! A font id is `font.` plus the font file name without extension, lower case,
//! letters and digits only: `Vazirmatn-Bold.ttf` is `font.vazirmatnbold`. A
//! language names fonts by id in its `_meta.ron`; the engine finds the files in
//! `assets/fonts/`, in `assets/i18n/<lang>/fonts/` and in language zips, and
//! never fails when one is missing (the built-in font is used, with one
//! warning).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::text::Font;

/// Where a text sits in the visual hierarchy. Without a `FontRole` component
/// the role is guessed from the font size ([`role_for_size`]).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontRole {
    Body,
    Heading,
    Title,
    Button,
}

/// A named context (`"hud-numbers"`) a language can give its own font.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct FontContext(pub String);

/// The role of a text with no explicit [`FontRole`], from its size in px.
pub fn role_for_size(size: f32) -> FontRole {
    if size >= 24.0 {
        FontRole::Title
    } else if size >= 19.0 {
        FontRole::Heading
    } else {
        FontRole::Body
    }
}

/// `font.` + lower-case letters and digits of a file stem.
pub fn id_from_stem(stem: &str) -> String {
    let body: String = stem
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    format!("font.{body}")
}

/// Whether `id` has the shape `font.<letters and digits>`.
pub fn is_font_id(id: &str) -> bool {
    id.strip_prefix("font.").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
}

enum FontData {
    Path(PathBuf),
    Bytes(Arc<Vec<u8>>),
}

/// All fonts the game can use, by id. Fonts are read and registered with Bevy
/// the first time they are needed.
#[derive(Resource, Default)]
pub struct FontLibrary {
    files: BTreeMap<String, FontData>,
    handles: HashMap<String, Handle<Font>>,
    reported: HashSet<String>,
}

impl FontLibrary {
    /// Registers every `.ttf` / `.otf` in `dir`; ids already known are kept.
    /// Returns how many new ids were added.
    pub fn scan_dir(&mut self, dir: &Path) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_font_file(p))
            .collect();
        paths.sort();
        let mut added = 0;
        for path in paths {
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let id = id_from_stem(stem);
            if !self.files.contains_key(&id) {
                self.files.insert(id, FontData::Path(path));
                added += 1;
            }
        }
        added
    }

    /// Registers font bytes (from a language pack); an existing id is kept.
    pub fn add_bytes(&mut self, id: &str, bytes: Arc<Vec<u8>>) {
        self.files
            .entry(id.to_string())
            .or_insert(FontData::Bytes(bytes));
    }

    pub fn contains(&self, id: &str) -> bool {
        self.files.contains_key(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// The Bevy font for `id`, loading it on first use. `None` (with one
    /// warning per id) when the font is unknown or unreadable.
    pub fn handle(&mut self, id: &str, assets: &mut Assets<Font>) -> Option<Handle<Font>> {
        if let Some(handle) = self.handles.get(id) {
            return Some(handle.clone());
        }
        let bytes = match self.files.get(id) {
            None => {
                self.warn_once(
                    id,
                    format!("font '{id}' was not found in assets/fonts or a language pack; using the built-in font"),
                );
                return None;
            }
            Some(FontData::Bytes(bytes)) => bytes.as_ref().clone(),
            Some(FontData::Path(path)) => match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(err) => {
                    let message = format!("cannot read font {}: {err}", path.display());
                    self.warn_once(id, message);
                    return None;
                }
            },
        };
        let handle = assets.add(Font::from_bytes(bytes));
        self.handles.insert(id.to_string(), handle.clone());
        Some(handle)
    }

    fn warn_once(&mut self, key: &str, message: String) {
        if self.reported.insert(key.to_string()) {
            warn!("i18n: {message}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_come_from_file_stems() {
        assert_eq!(id_from_stem("Vazirmatn-Bold"), "font.vazirmatnbold");
        assert_eq!(id_from_stem("Noto Sans_Regular 2"), "font.notosansregular2");
        assert!(is_font_id("font.vazirmatnbold"));
        for bad in ["font.", "vazirmatn", "font.Vazir", "font.a-b", "font. a"] {
            assert!(!is_font_id(bad), "{bad}");
        }
        assert!(is_font_id(&id_from_stem("Vazirmatn-Bold")));
    }

    #[test]
    fn roles_follow_size_when_not_given() {
        assert_eq!(role_for_size(28.0), FontRole::Title);
        assert_eq!(role_for_size(24.0), FontRole::Title);
        assert_eq!(role_for_size(20.0), FontRole::Heading);
        assert_eq!(role_for_size(18.0), FontRole::Body);
        assert_eq!(role_for_size(14.0), FontRole::Body);
    }

    #[test]
    fn the_library_scans_a_folder_and_keeps_the_first_of_a_clash() {
        let dir = std::env::temp_dir().join(format!("oopsmath_fonts_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "Vazirmatn-Bold.ttf",
            "Vazirmatn-Regular.TTF",
            "readme.txt",
            "VazirmatnBold.otf",
        ] {
            std::fs::write(dir.join(name), b"not a real font").unwrap();
        }
        let mut library = FontLibrary::default();
        assert_eq!(library.scan_dir(&dir), 2);
        assert!(library.contains("font.vazirmatnbold"));
        assert!(library.contains("font.vazirmatnregular"));
        assert!(!library.contains("font.readme"));
        assert_eq!(library.scan_dir(&dir), 0, "known ids are kept");
        library.add_bytes("font.extra", Arc::new(vec![1, 2, 3]));
        library.add_bytes("font.extra", Arc::new(vec![9]));
        assert_eq!(library.ids().count(), 3);
        assert_eq!(FontLibrary::default().scan_dir(&dir.join("missing")), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
