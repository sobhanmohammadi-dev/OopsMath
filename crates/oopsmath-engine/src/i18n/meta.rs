//! `_meta.ron`: the metadata of one language.

use std::collections::BTreeMap;

use serde::Deserialize;
use thiserror::Error;
use unic_langid::LanguageIdentifier;

use super::fonts::{FontRole, is_font_id};

pub const META_FILE: &str = "_meta.ron";
pub const META_FORMAT: u32 = 1;

/// Reading direction of a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

/// Which font to use where. Values are font ids (`font.<name>`, see
/// [`super::fonts`]). A role left out falls back to `default`, then to the
/// built-in font.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct FontSpec {
    pub default: Option<String>,
    pub body: Option<String>,
    pub heading: Option<String>,
    pub title: Option<String>,
    pub button: Option<String>,
    /// Named contexts (`FontContext("hud-numbers")`) -> font id.
    pub contexts: BTreeMap<String, String>,
}

impl FontSpec {
    /// The font id for a text, most specific first: named context, role,
    /// default.
    pub fn resolve(&self, role: FontRole, context: Option<&str>) -> Option<&str> {
        if let Some(context) = context
            && let Some(id) = self.contexts.get(context)
        {
            return Some(id);
        }
        let by_role = match role {
            FontRole::Body => &self.body,
            FontRole::Heading => &self.heading,
            FontRole::Title => &self.title,
            FontRole::Button => &self.button,
        };
        by_role.as_deref().or(self.default.as_deref())
    }

    fn ids(&self) -> impl Iterator<Item = &str> {
        [
            &self.default,
            &self.body,
            &self.heading,
            &self.title,
            &self.button,
        ]
        .into_iter()
        .filter_map(|id| id.as_deref())
        .chain(self.contexts.values().map(String::as_str))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct LanguageMeta {
    pub format: u32,
    /// BCP 47 tag; must equal the folder name.
    pub code: String,
    /// English name.
    pub name: String,
    /// Name in the language itself.
    pub native_name: String,
    pub direction: Direction,
    /// Language used for untranslated messages (default `en-US`).
    pub fallback: Option<String>,
    pub version: String,
    pub authors: Vec<String>,
    pub fonts: FontSpec,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MetaError {
    #[error("cannot parse _meta.ron: {0}")]
    Parse(String),
    #[error("invalid _meta.ron: {0}")]
    Invalid(String),
}

impl LanguageMeta {
    pub fn parse(text: &str) -> Result<Self, MetaError> {
        let meta: LanguageMeta =
            ron::from_str(text).map_err(|err| MetaError::Parse(err.to_string()))?;
        meta.validate()?;
        Ok(meta)
    }

    pub fn validate(&self) -> Result<(), MetaError> {
        let invalid = |msg: String| Err(MetaError::Invalid(msg));
        if self.format != META_FORMAT {
            return invalid(format!(
                "format must be {META_FORMAT} (found {})",
                self.format
            ));
        }
        if self.code.parse::<LanguageIdentifier>().is_err() || self.code.is_empty() {
            return invalid(format!("code '{}' is not a language tag", self.code));
        }
        if self.name.trim().is_empty() || self.native_name.trim().is_empty() {
            return invalid("name and native_name are required".to_string());
        }
        if let Some(bad) = self.fonts.ids().find(|id| !is_font_id(id)) {
            return invalid(format!("'{bad}' is not a font id (expected font.<name>)"));
        }
        Ok(())
    }

    pub fn langid(&self) -> LanguageIdentifier {
        self.code.parse().unwrap_or_default()
    }

    /// Every font id this language mentions.
    pub fn font_ids(&self) -> Vec<&str> {
        self.fonts.ids().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FA: &str = r#"#![enable(implicit_some)]
// comment
(
    format: 1,
    code: "fa",
    name: "Persian",
    native_name: "فارسی",
    direction: Rtl,
    version: "1.0.0",
    authors: ["me"],
    fonts: (
        default: "font.vazirmatn",
        title: "font.vazirmatnbold",
        contexts: { "hud-numbers": "font.vazirmatn", },
    ),
)
"#;

    #[test]
    fn a_full_meta_file_parses() {
        let meta = LanguageMeta::parse(FA).unwrap();
        assert_eq!(meta.code, "fa");
        assert_eq!(meta.direction, Direction::Rtl);
        assert_eq!(meta.native_name, "فارسی");
        assert_eq!(meta.fonts.title.as_deref(), Some("font.vazirmatnbold"));
        assert_eq!(meta.font_ids().len(), 3);
        assert_eq!(meta.langid().to_string(), "fa");
    }

    #[test]
    fn minimal_meta_uses_defaults() {
        let meta = LanguageMeta::parse(
            r#"(format: 1, code: "en-US", name: "English", native_name: "English")"#,
        )
        .unwrap();
        assert_eq!(meta.direction, Direction::Ltr);
        assert!(meta.fonts.default.is_none() && meta.fallback.is_none());
    }

    #[test]
    fn invalid_meta_is_rejected_with_a_reason() {
        let cases = [
            (
                r#"(format: 2, code: "fa", name: "a", native_name: "b")"#,
                "format",
            ),
            (
                r#"(format: 1, code: "", name: "a", native_name: "b")"#,
                "code",
            ),
            (
                r#"(format: 1, code: "not a tag!", name: "a", native_name: "b")"#,
                "code",
            ),
            (
                r#"(format: 1, code: "fa", name: "", native_name: "b")"#,
                "required",
            ),
            (
                r#"(format: 1, code: "fa", name: "a", native_name: "b", fonts: (default: Some("Vazir")))"#,
                "font id",
            ),
        ];
        for (text, needle) in cases {
            let err = LanguageMeta::parse(text).unwrap_err().to_string();
            assert!(err.contains(needle), "{text} -> {err}");
        }
        assert!(matches!(
            LanguageMeta::parse("(code: "),
            Err(MetaError::Parse(_))
        ));
    }

    #[test]
    fn fonts_resolve_context_then_role_then_default() {
        let spec = LanguageMeta::parse(FA).unwrap().fonts;
        assert_eq!(
            spec.resolve(FontRole::Title, None),
            Some("font.vazirmatnbold")
        );
        assert_eq!(spec.resolve(FontRole::Body, None), Some("font.vazirmatn"));
        assert_eq!(spec.resolve(FontRole::Button, None), Some("font.vazirmatn"));
        assert_eq!(
            spec.resolve(FontRole::Title, Some("hud-numbers")),
            Some("font.vazirmatn"),
            "a named context beats the role"
        );
        assert_eq!(
            spec.resolve(FontRole::Title, Some("unknown")),
            Some("font.vazirmatnbold")
        );
        assert_eq!(FontSpec::default().resolve(FontRole::Body, None), None);
    }
}
