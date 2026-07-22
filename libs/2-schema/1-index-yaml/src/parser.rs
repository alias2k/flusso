use std::fmt;

use schema_core::ParseFrom;

use crate::SUPPORTED_VERSIONS;

/// The 1-based source position a syntax error points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorLocation {
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub column: usize,
}

#[derive(Debug)]
pub enum ParseError {
    /// A YAML syntax or shape error. `Display` renders the message with the
    /// snippet (the offending source line and a caret) when one is present;
    /// tools that draw their own context (the visual designer's Code editor)
    /// read `message`/`location` structurally instead.
    Syntax {
        /// The cleaned description, without location or snippet.
        message: String,
        /// Where the error points — `None` for field-scoped errors, whose
        /// reported position is untrustworthy (see `syntax_error`).
        location: Option<ErrorLocation>,
        /// The rendered `--> line …` + caret block shown under the message.
        snippet: Option<String>,
    },
    /// The file declares a schema version this parser doesn't speak.
    UnsupportedVersion {
        /// The declared version.
        got: u8,
        /// The versions this parser accepts.
        supported: &'static str,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax {
                message,
                location,
                snippet,
            } => {
                f.write_str(message)?;
                match (snippet, location) {
                    (Some(snippet), _) => write!(f, "\n{snippet}"),
                    // A trustworthy location whose line fell outside the source
                    // (so no snippet could be drawn): state it inline.
                    (None, Some(at)) => write!(f, " (line {}, column {})", at.line, at.column),
                    (None, None) => Ok(()),
                }
            }
            Self::UnsupportedVersion { got, supported } => {
                write!(
                    f,
                    "unsupported schema version {got}; supported versions: {supported}"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    /// The `(type tag, field name, detail)` a field-scoped syntax error names —
    /// the `` `tag` field `name`: detail `` format the field deserializer
    /// emits. This is the structured counterpart to the message prose, so tools
    /// (the designer anchors its editor squiggle with it) don't parse the
    /// string themselves.
    pub fn field_scope(&self) -> Option<(&str, &str, &str)> {
        let Self::Syntax { message, .. } = self else {
            return None;
        };
        let rest = message.strip_prefix('`')?;
        let (tag, rest) = rest.split_once('`')?;
        let rest = rest.strip_prefix(" field `")?;
        let (name, rest) = rest.split_once('`')?;
        let detail = rest.strip_prefix(": ")?;
        Some((tag, name, detail))
    }
}

impl<T: AsRef<str>> ParseFrom<T> for super::SchemaYaml {
    type Error = ParseError;

    fn try_parse(value: T) -> Result<Self, Self::Error> {
        let source = value.as_ref();
        let result: super::SchemaYaml =
            serde_yaml::from_str(source).map_err(|error| syntax_error(source, &error))?;

        if !SUPPORTED_VERSIONS.contains(&result.version) {
            return Err(ParseError::UnsupportedVersion {
                got: result.version,
                supported: "1",
            });
        }

        Ok(result)
    }
}

/// Turn a `serde_yaml` error into a [`ParseError::Syntax`]: a cleaned-up
/// description plus, when the error carries a *trustworthy* location, the
/// 1-based position and the offending source line with a caret — the same
/// shape the `toml` parser prints.
///
/// Field-level errors are the exception: a field parses through a `serde_yaml`
/// `Value` round-trip (to find its type tag, see [`field`]), which discards
/// source positions, so `serde_yaml` stamps such an error at the *start of the
/// `fields` sequence* rather than the offending field. A caret there would point
/// at the wrong field, so we omit the location entirely — the message already
/// names the field by its type tag and document key ([`ParseError::field_scope`]
/// is its structured form), a more reliable locator than a wrong line.
///
/// [`field`]: crate::Field
fn syntax_error(source: &str, error: &serde_yaml::Error) -> ParseError {
    let message = clean_message(&error.to_string());
    match error.location() {
        Some(at) if !is_field_scoped(&message) => ParseError::Syntax {
            snippet: render_snippet(source, at.line(), at.column()),
            location: Some(ErrorLocation {
                line: at.line(),
                column: at.column(),
            }),
            message,
        },
        _ => ParseError::Syntax {
            message,
            location: None,
            snippet: None,
        },
    }
}

/// Whether a (cleaned) message came from parsing a single field, and so carries
/// an unreliable location. Field messages either name the field — `` `keyword`
/// field `email`: … `` — or are the tag diagnostics that open with `field `.
fn is_field_scoped(message: &str) -> bool {
    message.starts_with('`') || message.starts_with("field ")
}

/// Tidy a raw `serde_yaml` message into our phrasing:
/// - drop the trailing ` at line L column C` (the snippet shows it instead),
/// - hide the internal `field` key we inject while parsing (see [`field`]) so it
///   never appears in serde's "expected one of …" lists,
/// - say "key" rather than serde's "field" (a schema field is a different thing).
///
/// [`field`]: crate::Field
fn clean_message(raw: &str) -> String {
    let without_location = match raw.rfind(" at line ") {
        Some(idx) => raw.get(..idx).unwrap_or(raw),
        None => raw,
    };

    // Drop serde's leading `fields:` path breadcrumb(s); the snippet already
    // points at the exact line, so the breadcrumb only stutters.
    let mut trimmed = without_location;
    while let Some(rest) = trimmed.strip_prefix("fields: ") {
        trimmed = rest;
    }

    let cleaned = trimmed
        .replace("`field`, ", "")
        .replace(", `field`", "")
        .replace("unknown field", "unknown key")
        .replace("missing field", "missing key");
    suggest_unknown_key(&cleaned).unwrap_or(cleaned)
}

/// Append a `did you mean` hint to an unknown-key message when one of the
/// expected keys is a near-miss (edit distance ≤ 2) of the unknown one — the
/// usual cause is a typo (`transform` for `transforms`), and serde's flat
/// "expected one of" list makes the reader hunt for it.
fn suggest_unknown_key(message: &str) -> Option<String> {
    let (before, after) = message.split_once("unknown key `")?;
    let (unknown, rest) = after.split_once('`')?;
    let expected = rest.split_once("expected one of ")?.1;
    let (distance, best) = expected
        .split('`')
        .skip(1)
        .step_by(2)
        .map(|candidate| (edit_distance(unknown, candidate), candidate))
        .min_by_key(|(distance, _)| *distance)?;
    // The question mark replaces the comma that followed the unknown key.
    let rest = rest.strip_prefix(',').unwrap_or(rest);
    (distance > 0 && distance <= 2)
        .then(|| format!("{before}unknown key `{unknown}` — did you mean `{best}`?{rest}"))
}

/// Plain Levenshtein distance, small-string sized (key names).
fn edit_distance(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = Vec::with_capacity(b_chars.len() + 1);
        current.push(i + 1);
        for (j, cb) in b_chars.iter().enumerate() {
            let deletion = prev
                .get(j + 1)
                .copied()
                .unwrap_or(usize::MAX)
                .saturating_add(1);
            let insertion = current
                .get(j)
                .copied()
                .unwrap_or(usize::MAX)
                .saturating_add(1);
            let substitution = prev
                .get(j)
                .copied()
                .unwrap_or(usize::MAX)
                .saturating_add(usize::from(ca != *cb));
            current.push(deletion.min(insertion).min(substitution));
        }
        prev = current;
    }
    prev.last().copied().unwrap_or(usize::MAX)
}

/// Render the offending source line with a caret under the reported column
/// (the block `Display` prints under the message). `None` when the line falls
/// outside the source, in which case `Display` states the position inline.
/// `line`/`column` are 1-based, as `serde_yaml` reports them.
fn render_snippet(source: &str, line: usize, column: usize) -> Option<String> {
    let text = line
        .checked_sub(1)
        .and_then(|idx| source.lines().nth(idx))?;

    let number = line.to_string();
    let gutter = " ".repeat(number.len());
    let caret_indent = " ".repeat(column.saturating_sub(1));
    Some(format!(
        "{gutter}--> line {line}, column {column}\n\
         {gutter} |\n\
         {number} | {text}\n\
         {gutter} | {caret_indent}^"
    ))
}
