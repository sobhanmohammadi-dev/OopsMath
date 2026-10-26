//! Fluent evaluation for one language.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

/// All `core.ftl` sources of one language, evaluated with Fluent (plurals,
/// variables, selects, terms and attributes).
pub struct Translator {
    bundle: FluentBundle<FluentResource>,
    pub diagnostics: Vec<String>,
}

impl Translator {
    /// Builds a translator. Sources later in the list override earlier ones
    /// per message. Parse problems are collected in `diagnostics`; a damaged
    /// file still provides every message that parsed.
    pub fn new(language: LanguageIdentifier, sources: &[&str]) -> Self {
        let mut bundle = FluentBundle::new_concurrent(vec![language]);
        // Unicode isolation marks around placeables would render as boxes in
        // the UI fonts; bidirectional text is handled by the text layout.
        bundle.set_use_isolating(false);
        let mut diagnostics = Vec::new();
        for source in sources {
            let resource = match FluentResource::try_new((*source).to_string()) {
                Ok(resource) => resource,
                Err((resource, errors)) => {
                    diagnostics.extend(
                        errors
                            .iter()
                            .map(|err| format!("fluent syntax error: {err:?}")),
                    );
                    resource
                }
            };
            bundle.add_resource_overriding(resource);
        }
        Self {
            bundle,
            diagnostics,
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.format(key, None).is_some()
    }

    /// Formats `key` (`message-id` or `message-id.attribute`). `None` when the
    /// message or attribute does not exist in this language.
    pub fn format(&self, key: &str, args: Option<&FluentArgs>) -> Option<String> {
        let (message, attribute) = match self.bundle.get_message(key) {
            Some(message) => (message, None),
            None => {
                let (base, attribute) = key.rsplit_once('.')?;
                (self.bundle.get_message(base)?, Some(attribute))
            }
        };
        let pattern = match attribute {
            None => message.value()?,
            Some(name) => message.get_attribute(name)?.value(),
        };
        let mut errors = Vec::new();
        Some(
            self.bundle
                .format_pattern(pattern, args, &mut errors)
                .into_owned(),
        )
    }
}

/// Message ids found in an `.ftl` text, with whether the message is a plain
/// single-line string (no placeables, no continuation lines).
pub fn scan_messages(source: &str) -> Vec<(String, bool)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some((id, value)) = line.split_once('=') else {
            continue;
        };
        let id = id.trim_end();
        let valid = id.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            && !line.starts_with([' ', '\t', '#']);
        if !valid {
            continue;
        }
        let continues = lines
            .get(index + 1)
            .is_some_and(|next| next.starts_with([' ', '\t']) && !next.trim().is_empty());
        found.push((
            id.to_string(),
            !value.trim().is_empty() && !value.contains('{') && !continues,
        ));
    }
    found
}

/// `(id, value)` of every single-line message of an `.ftl` text (values are
/// trimmed; messages with continuation lines or attributes-only are skipped).
pub fn scan_single_line_messages(source: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with([' ', '\t', '#', '-']) {
            continue;
        }
        let Some((id, value)) = line.split_once('=') else {
            continue;
        };
        let id = id.trim_end();
        let valid = id.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        let continues = lines
            .get(index + 1)
            .is_some_and(|next| next.starts_with([' ', '\t']) && !next.trim().is_empty());
        if valid && !continues && !value.trim().is_empty() {
            found.push((id.to_string(), value.trim().to_string()));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluent_bundle::FluentValue;

    fn en(sources: &[&str]) -> Translator {
        Translator::new("en-US".parse().unwrap(), sources)
    }

    const SOURCE: &str = "\
hello = Hello
greet = Hello, { $name }!
bought =
    { $count ->
        [one] You bought one block.
       *[other] You bought { $count } blocks.
    }
-brand = OopsMath
about = About { -brand }
    .title = About the game
";

    #[test]
    fn variables_plurals_terms_and_attributes_work() {
        let t = en(&[SOURCE]);
        assert_eq!(t.format("hello", None).as_deref(), Some("Hello"));
        let mut args = FluentArgs::new();
        args.set("name", "Sam");
        assert_eq!(
            t.format("greet", Some(&args)).as_deref(),
            Some("Hello, Sam!")
        );
        let mut one = FluentArgs::new();
        one.set("count", FluentValue::from(1));
        assert_eq!(
            t.format("bought", Some(&one)).as_deref(),
            Some("You bought one block.")
        );
        let mut many = FluentArgs::new();
        many.set("count", FluentValue::from(5));
        assert_eq!(
            t.format("bought", Some(&many)).as_deref(),
            Some("You bought 5 blocks.")
        );
        assert_eq!(t.format("about", None).as_deref(), Some("About OopsMath"));
        assert_eq!(
            t.format("about.title", None).as_deref(),
            Some("About the game")
        );
        assert!(t.format("missing", None).is_none() && t.format("a.b", None).is_none());
        assert!(
            !t.format("greet", Some(&args)).unwrap().contains('\u{2068}'),
            "no isolation marks"
        );
    }

    #[test]
    fn later_sources_override_and_missing_messages_stay_available() {
        let t = en(&["a = one\nb = two", "a = ONE"]);
        assert_eq!(t.format("a", None).as_deref(), Some("ONE"));
        assert_eq!(t.format("b", None).as_deref(), Some("two"));
        assert!(t.has("b") && !t.has("c"));
    }

    #[test]
    fn a_broken_file_still_provides_its_valid_messages() {
        let t = en(&["good = fine\nthis is not valid\nalso = fine too"]);
        assert_eq!(t.format("good", None).as_deref(), Some("fine"));
        assert_eq!(t.format("also", None).as_deref(), Some("fine too"));
        assert!(!t.diagnostics.is_empty());
    }

    #[test]
    fn persian_plural_rules_are_applied() {
        // Persian has only `one` and `other`; 0 and 1 are both `one`.
        let t = Translator::new(
            "fa".parse().unwrap(),
            &["n = { $c ->\n    [one] یک\n   *[other] چند\n}"],
        );
        let mut args = FluentArgs::new();
        args.set("c", FluentValue::from(1));
        assert_eq!(t.format("n", Some(&args)).as_deref(), Some("یک"));
        let mut args = FluentArgs::new();
        args.set("c", FluentValue::from(7));
        assert_eq!(t.format("n", Some(&args)).as_deref(), Some("چند"));
    }

    #[test]
    fn single_line_messages_are_listed_with_their_values() {
        let found = scan_single_line_messages(&format!("# c\n\n{SOURCE}"));
        let ids: Vec<&str> = found.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["hello", "greet"]);
        assert_eq!(found[1].1, "Hello, { $name }!");
    }

    #[test]
    fn scanning_finds_simple_messages_only_as_simple() {
        let found = scan_messages(&format!("# c\n\n{SOURCE}"));
        let simple: Vec<&str> = found
            .iter()
            .filter(|(_, s)| *s)
            .map(|(id, _)| id.as_str())
            .collect();
        let all: Vec<&str> = found.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(simple, ["hello"]);
        assert_eq!(all, ["hello", "greet", "bought", "about"]);
    }
}
