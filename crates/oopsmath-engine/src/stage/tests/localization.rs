//! Unit tests for `stage::localization`.

use crate::stage::localization::{FluentMessages, StageLocale, StageMessages};
use crate::stage::package::Localization;

fn localization(locale: &str, text: &str) -> Localization {
    Localization {
        locale: locale.to_string(),
        raw: text.as_bytes().to_vec(),
    }
}

#[test]
fn parses_simple_messages_and_dotted_keys() {
    let messages = FluentMessages::parse(
        b"stage.001_first_wall.title = My first stage\nstage.001_first_wall.description = Build a wall.\n",
    );
    assert_eq!(
        messages.get("stage.001_first_wall.title"),
        Some("My first stage")
    );
    assert_eq!(
        messages.get("stage.001_first_wall.description"),
        Some("Build a wall.")
    );
    assert_eq!(messages.get("missing"), None);
    assert_eq!(messages.len(), 2);
}

#[test]
fn ignores_comments_blank_lines_and_whitespace() {
    let messages =
        FluentMessages::parse(b"# a comment\n\n   \n  title   =   spaced value   \n#another\n");
    assert_eq!(messages.get("title"), Some("spaced value"));
    assert_eq!(messages.len(), 1);
}

#[test]
fn handles_crlf_line_endings() {
    let messages = FluentMessages::parse(b"title = hello\r\ndescription = world\r\n");
    assert_eq!(messages.get("title"), Some("hello"));
    assert_eq!(messages.get("description"), Some("world"));
}

#[test]
fn duplicate_ids_keep_the_last_definition() {
    let messages = FluentMessages::parse(b"title = first\ntitle = second\n");
    assert_eq!(messages.get("title"), Some("second"));
    assert_eq!(messages.len(), 1);
}

#[test]
fn ignores_lines_without_a_top_level_equals() {
    let messages = FluentMessages::parse(b"# comment\nnot a message\nkey = value\n= no key\n");
    assert_eq!(messages.get("key"), Some("value"));
    assert_eq!(messages.len(), 1);
}

#[test]
fn does_not_panic_on_invalid_utf8() {
    let messages = FluentMessages::parse(&[b't', b'=', 0xFF, 0xFE]);
    assert!(messages.get("t").is_some());
}

#[test]
fn resolves_missing_key_to_the_key_itself() {
    let messages = StageMessages::from_localizations(&[localization("en-US", "title = hello")]);
    let locale = StageLocale::from("en-US");
    assert_eq!(locale.resolve(&messages, "title"), "hello");
    assert_eq!(locale.resolve(&messages, "missing.key"), "missing.key");
}

#[test]
fn falls_back_to_the_fallback_locale() {
    let messages = StageMessages::from_localizations(&[
        localization("en-US", "title = English"),
        localization("fa", "question = Persian"),
    ]);
    let locale = StageLocale {
        locale: "fa".to_string(),
        fallback: "en-US".to_string(),
    };
    // Present in the requested locale.
    assert_eq!(locale.resolve(&messages, "question"), "Persian");
    // Missing in `fa`, present in the fallback.
    assert_eq!(locale.resolve(&messages, "title"), "English");
}

#[test]
fn base_language_is_a_candidate() {
    let messages = StageMessages::from_localizations(&[localization("fa", "title = Farsi")]);
    let locale = StageLocale {
        locale: "fa-IR".to_string(),
        fallback: "en-US".to_string(),
    };
    assert_eq!(locale.resolve(&messages, "title"), "Farsi");
}

#[test]
fn absent_locale_never_panics() {
    let messages = StageMessages::from_localizations(&[localization("en-US", "title = hello")]);
    let locale = StageLocale::from("de");
    assert_eq!(locale.resolve(&messages, "title"), "hello");
    assert_eq!(locale.resolve(&messages, "unknown"), "unknown");
}

#[test]
fn exposes_locales_and_presence() {
    let messages = StageMessages::from_localizations(&[
        localization("en-US", "title = a"),
        localization("fa", "title = b"),
    ]);
    let locales: Vec<&str> = messages.locales().collect();
    assert_eq!(locales, ["en-US", "fa"]);
    assert!(messages.has_locale("fa"));
    assert!(!messages.has_locale("de"));
}
