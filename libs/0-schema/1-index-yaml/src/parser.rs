use schema_core::ParseFrom;

use crate::SUPPORTED_VERSIONS;

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    /// A YAML syntax or shape error, rendered with the offending source line and
    /// a caret pointing at it (see [`render_yaml_error`]).
    #[error("{0}")]
    Syntax(String),
    #[error("unsupported schema version {got}; supported versions: {supported}")]
    UnsupportedVersion { got: u8, supported: &'static str },
}

impl<T: AsRef<str>> ParseFrom<T> for super::SchemaYaml {
    type Error = ParseError;

    fn try_parse(value: T) -> Result<Self, Self::Error> {
        let source = value.as_ref();
        let result: super::SchemaYaml = serde_yaml::from_str(source)
            .map_err(|error| ParseError::Syntax(render_yaml_error(source, &error)))?;

        if !SUPPORTED_VERSIONS.contains(&result.version) {
            return Err(ParseError::UnsupportedVersion {
                got: result.version,
                supported: "1",
            });
        }

        Ok(result)
    }
}

/// Turn a `serde_yaml` error into a human-readable message: a cleaned-up
/// description plus, when the error carries a *trustworthy* location, the
/// offending source line with a caret — the same shape the `toml` parser prints.
///
/// Field-level errors are the exception: a field parses through a `serde_yaml`
/// `Value` round-trip (to find its type tag, see [`field`]), which discards
/// source positions, so `serde_yaml` stamps such an error at the *start of the
/// `fields` sequence* rather than the offending field. A caret there would point
/// at the wrong field, so we omit it — the message already names the field by
/// its type tag and document key, a more reliable locator than a wrong line.
///
/// [`field`]: crate::Field
fn render_yaml_error(source: &str, error: &serde_yaml::Error) -> String {
    let message = clean_message(&error.to_string());
    match error.location() {
        Some(location) if !is_field_scoped(&message) => {
            render_snippet(source, location.line(), location.column(), &message)
        }
        _ => message,
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

    trimmed
        .replace("`field`, ", "")
        .replace(", `field`", "")
        .replace("unknown field", "unknown key")
        .replace("missing field", "missing key")
}

/// Render `message` above the offending source line, with a caret under the
/// reported column. `line`/`column` are 1-based, as `serde_yaml` reports them.
fn render_snippet(source: &str, line: usize, column: usize, message: &str) -> String {
    let text = line.checked_sub(1).and_then(|idx| source.lines().nth(idx));
    let Some(text) = text else {
        return format!("{message} (line {line}, column {column})");
    };

    let number = line.to_string();
    let gutter = " ".repeat(number.len());
    let caret_indent = " ".repeat(column.saturating_sub(1));
    format!(
        "{message}\n{gutter}--> line {line}, column {column}\n\
         {gutter} |\n\
         {number} | {text}\n\
         {gutter} | {caret_indent}^"
    )
}
