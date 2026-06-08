//! Human-readable rendering for `flusso check`.
//!
//! The report is built for a person reading a terminal: a short summary of the
//! deployment (source, sinks, indexes), then a field tree per index — the
//! declared shape when offline, or the fully-resolved types and nullability
//! when checked against the database.
//!
//! Color is emitted only when stdout is a terminal and `NO_COLOR` is unset, so
//! piping the output to a file or `grep` stays clean. Alignment is computed on
//! the *uncolored* text, then color is applied as the line is written.

use std::io::{IsTerminal, Write};

use anyhow::Result;
use schema::{Config, ConnectionSpec, IndexMapping, ResolvedField, Secret, Sink, SoftDelete};
use sources_core::{Diagnostic, Severity};

// ── color ───────────────────────────────────────────────────────────────────

/// A palette that paints ANSI color only when enabled. Cheap to copy, so it is
/// threaded by value through the render functions.
#[derive(Clone, Copy)]
pub(crate) struct Pen {
    color: bool,
}

impl Pen {
    /// Color when stdout is a terminal and `NO_COLOR` is not set.
    pub(crate) fn detect() -> Self {
        Self {
            color: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    fn paint(self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    fn bold(self, t: &str) -> String {
        self.paint("1", t)
    }
    fn dim(self, t: &str) -> String {
        self.paint("2", t)
    }
    fn green(self, t: &str) -> String {
        self.paint("32", t)
    }
    fn yellow(self, t: &str) -> String {
        self.paint("33", t)
    }
    fn magenta(self, t: &str) -> String {
        self.paint("35", t)
    }
}

// ── status lines ──────────────────────────────────────────────────────────────

/// A green check mark followed by a bold message.
pub(crate) fn success(out: &mut impl Write, pen: Pen, message: &str) -> Result<()> {
    writeln!(out, "{} {}", pen.green("✓"), pen.bold(message))?;
    Ok(())
}

/// A yellow, bracketed note — e.g. the `[offline]` mode banner.
pub(crate) fn warning(out: &mut impl Write, pen: Pen, scope: &str, message: &str) -> Result<()> {
    writeln!(out, "\n{} {}", pen.yellow(&format!("[{scope}]")), message)?;
    Ok(())
}

/// A bold section title with a dim underline the width of the title.
fn section(out: &mut impl Write, pen: Pen, title: &str) -> Result<()> {
    writeln!(out, "\n{}", pen.bold(title))?;
    writeln!(out, "{}", pen.dim(&"─".repeat(title.chars().count())))?;
    Ok(())
}

// ── configuration summary ─────────────────────────────────────────────────────

/// The deployment at a glance: where data comes from, where it goes, and which
/// indexes are declared. Field detail is left to the schema trees.
pub(crate) fn config(out: &mut impl Write, pen: Pen, config: &Config) -> Result<()> {
    section(out, pen, "Source")?;
    let source_kind = match config.source.source_type {
        schema::SourceType::Postgres => "postgres",
    };
    writeln!(
        out,
        "  {}  {}",
        pen.magenta(source_kind),
        pen.dim(&describe_connection(config.source.connection.as_ref())),
    )?;

    section(out, pen, "Sinks")?;
    if config.sinks.is_empty() {
        writeln!(out, "  {}", pen.dim("(none — defaults to a stdout sink)"))?;
    } else {
        let rows: Vec<(String, String, String)> = config
            .sinks
            .iter()
            .map(|(name, sink)| {
                let (kind, detail) = describe_sink(sink);
                (
                    name.as_ref().to_owned(),
                    pen.magenta(kind),
                    pen.dim(&detail),
                )
            })
            .collect();
        aligned_rows(out, pen, &rows)?;
    }

    section(out, pen, "Indexes")?;
    let rows: Vec<(String, String, String)> = config
        .indexes
        .iter()
        .map(|(name, index)| {
            let schema = &index.schema;
            let state = if index.enabled {
                pen.green("enabled")
            } else {
                pen.dim("disabled")
            };
            let mut detail = format!("{}.{}", schema.db_schema, schema.table);
            if let Some(pk) = &schema.primary_key {
                detail.push_str(&format!("   pk {pk}"));
            }
            if let Some(sd) = &schema.soft_delete {
                detail.push_str(&format!("   soft-delete {}", describe_soft_delete(sd)));
            }
            (name.as_ref().to_owned(), state, pen.dim(&detail))
        })
        .collect();
    aligned_rows(out, pen, &rows)?;
    Ok(())
}

/// Print a left-aligned bold name column sized to the widest entry, followed by
/// two already-colored cells. The name column is padded on its *uncolored*
/// width (via [`pen_pad`]) so colored names still line up.
fn aligned_rows(out: &mut impl Write, pen: Pen, rows: &[(String, String, String)]) -> Result<()> {
    let width = rows
        .iter()
        .map(|(name, _, _)| name.chars().count())
        .max()
        .unwrap_or(0);
    for (name, col1, col2) in rows {
        writeln!(
            out,
            "  {:<width$}  {}  {}",
            pen.bold(name),
            col1,
            col2,
            width = width + pen_pad(pen, name),
        )?;
    }
    Ok(())
}

// ── field trees ───────────────────────────────────────────────────────────────

/// One rendered line of a field tree: its indented name and the columns that
/// describe it (type + nullability, or a source description). Built with
/// *uncolored* text so widths align; colored at print time.
struct Row {
    /// Nesting depth (0 = top-level field).
    depth: usize,
    name: String,
    /// Right-hand columns, paired with an ANSI color code.
    cells: Vec<(String, &'static str)>,
}

/// The resolved schema of every index: each field with the type and nullability
/// the source resolved. This is the heart of an online check.
pub(crate) fn resolved(out: &mut impl Write, pen: Pen, mappings: &[IndexMapping]) -> Result<()> {
    for mapping in mappings {
        section(out, pen, &format!("Index  {}", mapping.index))?;
        let mut rows = Vec::new();
        flatten_resolved(&mapping.fields, 0, &mut rows);
        print_rows(out, pen, &rows)?;
    }
    Ok(())
}

/// The disagreements found checking the declared schema against the database.
/// Errors are red, warnings yellow; an empty list prints a reassuring line.
pub(crate) fn diagnostics(
    out: &mut impl Write,
    pen: Pen,
    diagnostics: &[Diagnostic],
) -> Result<()> {
    section(out, pen, "Database validation")?;
    if diagnostics.is_empty() {
        writeln!(out, "  {}", pen.dim("(schema matches the database)"))?;
        return Ok(());
    }
    for d in diagnostics {
        let (label, code) = match d.severity {
            Severity::Error => ("error", "31"),
            Severity::Warning => ("warning", "33"),
        };
        writeln!(
            out,
            "  {} {}  {}",
            pen.paint(code, &format!("[{label}]")),
            pen.bold(&format!("{}.{}", d.index, d.field)),
            pen.dim(&d.message),
        )?;
    }
    Ok(())
}

fn flatten_resolved(fields: &[ResolvedField], depth: usize, rows: &mut Vec<Row>) {
    for field in fields {
        let nullability = if field.nullable {
            ("optional".to_owned(), "33") // yellow — stands out
        } else {
            ("required".to_owned(), "2") // dim
        };
        rows.push(Row {
            depth,
            name: field.name.to_string(),
            cells: vec![
                (field.mapping.mapping_type.name().to_owned(), "36"),
                nullability,
            ],
        });
        flatten_resolved(&field.children, depth + 1, rows);
    }
}

/// Print rows with dotted leaders aligning the description columns. The name
/// column is sized to the widest `indent + name`; each cell column to its widest
/// entry — so every column lines up regardless of nesting depth.
fn print_rows(out: &mut impl Write, pen: Pen, rows: &[Row]) -> Result<()> {
    let indent = |depth: usize| depth * 2;
    let name_w = rows
        .iter()
        .map(|r| indent(r.depth) + r.name.chars().count())
        .max()
        .unwrap_or(0);
    let cell_count = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    let cell_w: Vec<usize> = (0..cell_count)
        .map(|i| {
            rows.iter()
                .filter_map(|r| r.cells.get(i))
                .map(|(t, _)| t.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();

    for row in rows {
        let pad = "  ".repeat(row.depth);
        let used = indent(row.depth) + row.name.chars().count();
        // Dotted leader from the name to the type column (min two dots).
        let dots = name_w + 3 - used;
        let leader = pen.dim(&format!(" {} ", ".".repeat(dots.max(2) - 2)));

        write!(out, "  {pad}{}{leader}", pen.bold(&row.name))?;
        for (i, (text, code)) in row.cells.iter().enumerate() {
            if i > 0 {
                write!(out, "  ")?;
            }
            write!(out, "{}", pen.paint(code, text))?;
            // Pad every column but the last, so lines carry no trailing space.
            if i + 1 < row.cells.len() {
                let col = cell_w.get(i).copied().unwrap_or(0);
                write!(
                    out,
                    "{}",
                    " ".repeat(col.saturating_sub(text.chars().count()))
                )?;
            }
        }
        writeln!(out)?;
    }
    Ok(())
}

// ── descriptions ──────────────────────────────────────────────────────────────

fn describe_sink(sink: &Sink) -> (&'static str, String) {
    match sink {
        Sink::Opensearch(os) => {
            let mut detail = describe_secret_url(&os.url);
            if !os.tls_verify {
                detail.push_str("   tls-verify off");
            }
            ("opensearch", detail)
        }
        Sink::Stdout(s) => (
            "stdout",
            if s.pretty {
                "pretty".into()
            } else {
                String::new()
            },
        ),
    }
}

fn describe_soft_delete(sd: &SoftDelete) -> String {
    match sd {
        SoftDelete::Column(c) => format!("column \"{}\"", c.column),
        SoftDelete::Field(f) => format!("field \"{}\"", f.field),
    }
}

/// Describe the source connection without resolving it: an env reference shows
/// the variable, a literal URL shows itself (password masked), parts show the
/// host/database, and an absent connection notes the `DATABASE_URL` fallback.
fn describe_connection(spec: Option<&ConnectionSpec>) -> String {
    match spec {
        None => "(from DATABASE_URL at runtime)".to_owned(),
        Some(ConnectionSpec::Url(secret)) => describe_secret_url(secret),
        Some(ConnectionSpec::Parts {
            host,
            port,
            user,
            database,
            ..
        }) => format!("{user}@{host}:{port}/{database}"),
    }
}

/// Describe a URL-bearing secret without leaking it: an env reference shows the
/// variable name, a literal shows the URL with any embedded password masked.
fn describe_secret_url(secret: &Secret) -> String {
    match secret {
        Secret::Env(var) => format!("${{{var}}}"),
        Secret::Value(url) => redact_url(url),
    }
}

/// Mask the password in `scheme://user:password@host…` — the report shows the
/// URL, never the secret.
fn redact_url(url: &str) -> String {
    let Some(after) = url.find("://").map(|i| i + 3) else {
        return url.to_owned();
    };
    let Some(at) = url[after..].find('@').map(|i| after + i) else {
        return url.to_owned();
    };
    match url[after..at].find(':') {
        Some(colon) => format!("{}:***{}", &url[..after + colon], &url[at..]),
        None => url.to_owned(),
    }
}

/// ANSI escapes have zero display width, so a `{:<width}` pad over a *colored*
/// string under-pads. This returns the extra width the escapes occupy, to add
/// back into the format width when the painted string is padded.
fn pen_pad(pen: Pen, text: &str) -> usize {
    pen.bold(text).chars().count() - text.chars().count()
}
