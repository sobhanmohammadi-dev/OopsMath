//! Translating text that the game builds in code.
//!
//! Much player-facing text is assembled from English pieces: `"Placed: 3 / 20"`,
//! `"You have no Wood Log in your inventory."`. Instead of rewriting every place
//! that builds such a string, the English messages of `core.ftl` double as
//! *templates*: a message like `hud-placed-max = Placed: { $placed } / { $max }`
//! recognises the line `Placed: 3 / 20`, extracts `placed = 3` and `max = 20`,
//! and formats the same message in the active language.
//!
//! * Matching is per line (a text with `\n` is split into lines).
//! * Only messages whose value is a single line with plain `{ $name }`
//!   placeables are templates (select expressions, terms and functions are for
//!   messages used by key).
//! * A variable captures text up to the next literal piece of the template; two
//!   variables in a row (`{ $required }{ $mark }`) need the first to be a number.
//! * A template must contain some literal text, and more specific templates
//!   (more literal text) are tried first.
//! * The values are translated too: a value that is itself an English message
//!   (`Wood Log` -> `block-wood_log`) or another template is translated, and a
//!   block or tool id (`block.wood_log`) becomes its localized name.
//!
//! The English text of a template message is therefore a contract with the code
//! that builds the string. New code should use message keys
//! ([`super::LocalizedText`]) instead; `tests/i18n_pack.rs` checks that the
//! strings the game produces today are still recognised.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Lit(String),
    Var(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub id: String,
    pub segments: Vec<Segment>,
    literal_len: usize,
}

/// Splits a message value into literals and `{ $name }` placeables. `None`
/// when the value uses anything else (or has no literal text at all).
pub fn parse_template(raw: &str) -> Option<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut rest = raw;
    while let Some(open) = rest.find('{') {
        literal.push_str(&rest[..open]);
        let close = rest[open..].find('}')? + open;
        let inner = rest[open + 1..close].trim();
        let name = inner.strip_prefix('$')?;
        if name.is_empty()
            || !name.chars().next()?.is_ascii_alphabetic()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        if !literal.is_empty() {
            segments.push(Segment::Lit(std::mem::take(&mut literal)));
        }
        segments.push(Segment::Var(name.to_string()));
        rest = &rest[close + 1..];
    }
    if rest.contains('}') {
        return None;
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        segments.push(Segment::Lit(literal));
    }
    segments
        .iter()
        .any(|s| matches!(s, Segment::Lit(l) if !l.trim().is_empty()))
        .then_some(segments)
}

fn numeric_prefix(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut end = 0;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == 0 {
        return 0;
    }
    if end + 1 < bytes.len() && bytes[end] == b'.' && bytes[end + 1].is_ascii_digit() {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
    }
    end
}

impl Template {
    pub fn new(id: &str, segments: Vec<Segment>) -> Self {
        let literal_len = segments
            .iter()
            .map(|s| {
                if let Segment::Lit(l) = s {
                    l.chars().count()
                } else {
                    0
                }
            })
            .sum();
        Self {
            id: id.to_string(),
            segments,
            literal_len,
        }
    }

    /// The variables of `text` if it is an output of this template.
    pub fn matches(&self, text: &str) -> Option<Vec<(String, String)>> {
        let mut captured = Vec::new();
        let mut pos = 0;
        let last = self.segments.len().checked_sub(1)?;
        for (index, segment) in self.segments.iter().enumerate() {
            match segment {
                Segment::Lit(literal) => {
                    if !text[pos..].starts_with(literal.as_str()) {
                        return None;
                    }
                    pos += literal.len();
                }
                Segment::Var(name) => {
                    let end = match self.segments.get(index + 1) {
                        None => text.len(),
                        Some(Segment::Var(_)) => {
                            let n = numeric_prefix(&text[pos..]);
                            if n == 0 {
                                return None;
                            }
                            pos + n
                        }
                        Some(Segment::Lit(next)) if index + 1 == last => {
                            // The closing literal must end the text.
                            let end = text.len().checked_sub(next.len())?;
                            if end < pos
                                || !text.is_char_boundary(end)
                                || !text[end..].starts_with(next.as_str())
                            {
                                return None;
                            }
                            end
                        }
                        Some(Segment::Lit(next)) => pos + text[pos..].find(next.as_str())?,
                    };
                    captured.push((name.clone(), text[pos..end].to_string()));
                    pos = end;
                }
            }
        }
        (pos == text.len()).then_some(captured)
    }
}

/// All templates and exact messages of a language (normally English).
#[derive(Debug, Default, Clone)]
pub struct TemplateIndex {
    /// Message text without placeables -> message id.
    exact: BTreeMap<String, String>,
    /// Templates, most specific first.
    templates: Vec<Template>,
}

impl TemplateIndex {
    /// Builds the index from `(id, single-line value)` pairs.
    pub fn build<'a>(messages: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut index = Self::default();
        let mut sorted: BTreeMap<&str, &str> = BTreeMap::new();
        for (id, value) in messages {
            sorted.insert(id, value);
        }
        for (id, value) in sorted {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            if !value.contains('{') {
                index
                    .exact
                    .entry(value.to_string())
                    .or_insert_with(|| id.to_string());
            } else if let Some(segments) = parse_template(value) {
                index.templates.push(Template::new(id, segments));
            }
        }
        index.templates.sort_by(|a, b| {
            b.literal_len
                .cmp(&a.literal_len)
                .then_with(|| a.id.cmp(&b.id))
        });
        index
    }

    /// The id of an exact (variable-free) English message.
    pub fn exact(&self, text: &str) -> Option<&str> {
        self.exact.get(text).map(String::as_str)
    }

    /// The first template that produced `text`, with its variables.
    pub fn find(&self, text: &str) -> Option<(&str, Vec<(String, String)>)> {
        self.templates
            .iter()
            .find_map(|t| t.matches(text).map(|vars| (t.id.as_str(), vars)))
    }

    pub fn len(&self) -> usize {
        self.exact.len() + self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> TemplateIndex {
        TemplateIndex::build([
            ("money", "Money: { $amount } coins"),
            ("placed", "Placed: { $placed }"),
            ("placed-max", "Placed: { $placed } / { $max }"),
            ("row", "{ $name } - { $price } coins"),
            ("row-max", "{ $name } - { $price } coins (max { $max })"),
            (
                "row-minmax",
                "{ $name } - { $price } coins (min { $min }, max { $max })",
            ),
            (
                "objective",
                "{ $title }: { $progress } / { $required }{ $mark }",
            ),
            (
                "plan-line",
                "{ $name }: need { $required }, have { $available }{ $short } ({ $price })",
            ),
            ("hammer", "Hammer"),
            ("only-vars", "{ $a }{ $b }"),
            ("select", "{ $n ->"),
            ("term", "About { -brand }"),
        ])
    }

    #[test]
    fn templates_are_parsed_and_unsupported_syntax_is_skipped() {
        assert_eq!(
            parse_template("Placed: { $a } / {$b}"),
            Some(vec![
                Segment::Lit("Placed: ".into()),
                Segment::Var("a".into()),
                Segment::Lit(" / ".into()),
                Segment::Var("b".into()),
            ])
        );
        for bad in [
            "{ $a }",
            "x { -brand }",
            "x { NUMBER($a) }",
            "x { $n ->",
            "x } y",
            "x { $ }",
        ] {
            assert_eq!(parse_template(bad), None, "{bad}");
        }
        assert!(parse_template("no placeables").is_some());
        let idx = index();
        assert_eq!(idx.exact("Hammer"), Some("hammer"));
        assert!(
            idx.find("anything at all").is_none(),
            "var-only and unsupported messages are not templates"
        );
    }

    fn vars(
        found: Option<(&str, Vec<(String, String)>)>,
    ) -> Option<(String, Vec<(String, String)>)> {
        found.map(|(id, v)| (id.to_string(), v))
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(k, x)| (k.to_string(), x.to_string()))
            .collect()
    }

    #[test]
    fn a_line_is_matched_to_its_template_with_the_variables() {
        let idx = index();
        assert_eq!(
            vars(idx.find("Money: 25 coins")),
            Some(("money".into(), pairs(&[("amount", "25")])))
        );
        assert_eq!(
            vars(idx.find("Placed: 3 / 20")),
            Some((
                "placed-max".into(),
                pairs(&[("placed", "3"), ("max", "20")])
            ))
        );
        assert_eq!(
            vars(idx.find("Placed: 3")),
            Some(("placed".into(), pairs(&[("placed", "3")])))
        );
        assert!(idx.find("Placed:").is_none());
        assert!(
            idx.find("Money: 25 coin").is_none(),
            "the closing literal must end the text"
        );
    }

    #[test]
    fn the_most_specific_template_wins() {
        let idx = index();
        assert_eq!(vars(idx.find("Wood Log - 3 coins")).unwrap().0, "row");
        assert_eq!(
            vars(idx.find("Wood Log - 3 coins (max 20)")),
            Some((
                "row-max".into(),
                pairs(&[("name", "Wood Log"), ("price", "3"), ("max", "20")])
            ))
        );
        assert_eq!(
            vars(idx.find("Wood Log - 3 coins (min 1, max 20)")),
            Some((
                "row-minmax".into(),
                pairs(&[
                    ("name", "Wood Log"),
                    ("price", "3"),
                    ("min", "1"),
                    ("max", "20")
                ])
            ))
        );
    }

    #[test]
    fn adjacent_variables_need_a_leading_number() {
        let idx = index();
        assert_eq!(
            vars(idx.find("Walls: 3 / 5  (done)")),
            Some((
                "objective".into(),
                pairs(&[
                    ("title", "Walls"),
                    ("progress", "3"),
                    ("required", "5"),
                    ("mark", "  (done)")
                ])
            ))
        );
        assert_eq!(
            vars(idx.find("Walls: 3 / 5")).unwrap().1.last().unwrap(),
            &("mark".to_string(), String::new())
        );
        assert!(idx.find("Walls: 3 / five").is_none());
        let line = "Wood Log: need 8, have 2, 6 more needed (3 coins each = 9 coins)";
        let (id, v) = vars(idx.find(line)).unwrap();
        assert_eq!(id, "plan-line");
        assert_eq!(v[2], ("available".to_string(), "2".to_string()));
        assert_eq!(v[3], ("short".to_string(), ", 6 more needed".to_string()));
        assert_eq!(
            v[4],
            ("price".to_string(), "3 coins each = 9 coins".to_string())
        );
    }

    #[test]
    fn non_ascii_text_never_panics() {
        let idx = index();
        for text in [
            "Money: ۲۵ coins",
            "پول: 25 سکه",
            "Money: é coins",
            "Plac",
            "",
        ] {
            let _ = idx.find(text);
        }
    }
}
