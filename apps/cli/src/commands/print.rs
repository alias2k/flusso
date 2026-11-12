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
use config::{Config, IndexMapping, ResolvedField, SoftDelete};
use kernel::{OptionValue, Port, PortEntry};

use crate::adapters;
use source::{CoverageReport, Diagnostic, PreImageGap, PreImageReport, Severity};

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

    /// Color when stderr is a terminal and `NO_COLOR` is not set.
    pub(crate) fn detect_stderr() -> Self {
        Self {
            color: std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
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

/// A bold section heading with a dim note beside it.
pub(crate) fn heading(out: &mut impl Write, pen: Pen, title: &str, note: &str) -> Result<()> {
    writeln!(out, "\n{} {}", pen.bold(title), pen.dim(note))?;
    Ok(())
}

/// A red cross followed by the failure, its excerpt lines kept as they are.
pub(crate) fn failure(out: &mut impl Write, pen: Pen, message: &str) -> Result<()> {
    writeln!(out, "{} {}", pen.paint("31", "✗"), message)?;
    Ok(())
}

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

/// Report whether the source streams every table the indexes read, and — when it
/// doesn't — whether `flusso run` will fix it automatically or the operator must.
///
/// `manage` is the effective publication-management setting `run` would use, so
/// the phrasing matches what running would actually do; the remediation SQL is
/// always printed when there's a gap, copy-pasteable for the manual path.
pub(crate) fn coverage(
    out: &mut impl Write,
    pen: Pen,
    report: &CoverageReport,
    manage: bool,
) -> Result<()> {
    section(out, pen, "Publication coverage")?;

    if report.satisfied {
        writeln!(
            out,
            "  {} source streams all {} required table(s)",
            pen.green("✓"),
            report.present.len(),
        )?;
        return Ok(());
    }

    if !report.missing.is_empty() {
        writeln!(
            out,
            "  {} {} table(s) the indexes read are not yet streamed:",
            pen.yellow("!"),
            report.missing.len(),
        )?;
        for table in &report.missing {
            writeln!(out, "    {} {}", pen.dim("•"), table)?;
        }
    }
    if !report.misrouted.is_empty() {
        writeln!(
            out,
            "  {} {} partitioned table(s) stream under their partitions' names, which no index reads:",
            pen.yellow("!"),
            report.misrouted.len(),
        )?;
        for table in &report.misrouted {
            writeln!(out, "    {} {}", pen.dim("•"), table)?;
        }
    }

    if report.manageable && manage {
        writeln!(
            out,
            "  {}",
            pen.green("→ will be fixed automatically on the next `flusso run`"),
        )?;
    } else if report.manageable {
        writeln!(
            out,
            "  {}",
            pen.yellow(
                "→ the source role CAN fix this, but automatic management is disabled \
                 (manage_publication = false) — run the SQL below",
            ),
        )?;
    } else {
        writeln!(
            out,
            "  {}",
            pen.bold(&pen.yellow("→ flusso will NOT fix this automatically:")),
        )?;
        for blocker in &report.blockers {
            writeln!(out, "    {} {}", pen.dim("-"), blocker)?;
        }
    }

    if !report.remediation.is_empty() {
        section(out, pen, "Run to stream every table")?;
        for sql in &report.remediation {
            writeln!(out, "  {sql}")?;
        }
    }
    Ok(())
}

/// Report the tables an index reads whose replica identity isn't the whole old
/// row, in two groups: the ones that miss a parent link (a delete or re-parent
/// leaves the old parent's document stale) or part of the primary key (a
/// delete can't be addressed, so its document stays) and the ones that only
/// lose the unread-column skip. Each says whether `flusso run` will set it (`manage` is
/// the effective `manage_replica_identity`) or keeps it on purpose; the SQL is
/// printed for the ones it won't set and doesn't keep.
pub(crate) fn pre_image(
    out: &mut impl Write,
    pen: Pen,
    report: &PreImageReport,
    manage: bool,
) -> Result<()> {
    section(out, pen, "Replica identity")?;

    if report.complete() {
        writeln!(
            out,
            "  {} every table an index reads logs its whole old row",
            pen.green("✓"),
        )?;
        return Ok(());
    }

    let verdict = |gap: &PreImageGap| {
        if let Some(reason) = &gap.kept {
            pen.dim(&format!(
                "→ kept as is ({reason}); updates to this table always rebuild"
            ))
        } else if gap.will_manage(manage) {
            pen.green("→ will be set automatically on the next `flusso run`")
        } else if gap.manageable {
            pen.yellow("→ the role CAN set it, but manage_replica_identity = false")
        } else {
            pen.yellow(&format!(
                "→ flusso will NOT set it where: {}",
                gap.blockers.join("; ")
            ))
        }
    };

    if report.satisfied() {
        writeln!(
            out,
            "  {} every table's changes carry their primary key, and every child table's the \
             link to its parent",
            pen.green("✓"),
        )?;
    } else {
        writeln!(
            out,
            "  {} {} table(s) don't log their primary key or parent link on delete or update:",
            pen.yellow("!"),
            report.gaps.len(),
        )?;
        for gap in &report.gaps {
            let columns = |label: &str, columns: &[config::ColumnName]| {
                (!columns.is_empty()).then(|| {
                    let names = columns
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{label} {names}")
                })
            };
            let missing = [
                columns("primary key", &gap.missing_key),
                columns("parent link", &gap.missing),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; ");
            writeln!(
                out,
                "    {} {} ({})  {}",
                pen.dim("•"),
                gap.table,
                missing,
                verdict(gap)
            )?;
        }
    }

    if !report.partial.is_empty() {
        writeln!(
            out,
            "  {} {} table(s) don't log their whole old row, so an update there rebuilds what \
             reads it even when it changes no column a document reads:",
            pen.dim("·"),
            report.partial.len(),
        )?;
        for gap in &report.partial {
            writeln!(out, "    {} {}  {}", pen.dim("•"), gap.table, verdict(gap))?;
        }
    }

    fn manual(gaps: &[PreImageGap], manage: bool) -> Vec<&PreImageGap> {
        gaps.iter()
            .filter(|gap| gap.kept.is_none() && !gap.will_manage(manage))
            .collect()
    }
    let unset = manual(&report.gaps, manage);
    if unset.iter().any(|gap| !gap.missing_key.is_empty()) {
        writeln!(
            out,
            "  {}",
            pen.yellow(
                "→ until the primary key is logged, a delete there can't reach its document, \
                 which stays in the index",
            ),
        )?;
    }
    if unset.iter().any(|gap| !gap.missing.is_empty()) {
        writeln!(
            out,
            "  {}",
            pen.yellow(
                "→ until a parent link is logged, deleting or re-parenting that table's rows \
                 leaves the old parent's document stale",
            ),
        )?;
    }
    if !unset.is_empty() {
        section(
            out,
            pen,
            "Run so deletes and re-parents reach the right documents",
        )?;
        for gap in unset {
            writeln!(out, "  {}", gap.remediation)?;
        }
    }
    let rows = manual(&report.partial, manage);
    if !rows.is_empty() {
        section(out, pen, "Run to skip updates of columns no document reads")?;
        for gap in rows {
            writeln!(out, "  {}", gap.remediation)?;
        }
    }
    Ok(())
}

/// The deployment at a glance: where data comes from, where it goes, and which
/// indexes are declared. Field detail is left to the schema trees.
pub(crate) fn config(out: &mut impl Write, pen: Pen, config: &Config) -> Result<()> {
    section(out, pen, "Source")?;
    writeln!(
        out,
        "  {}  {}",
        pen.magenta(&config.source.kind),
        pen.dim(&describe_entry(Port::Source, &config.source)),
    )?;

    section(out, pen, "Stream")?;
    writeln!(
        out,
        "  {}  {}",
        pen.magenta(&config.stream.kind),
        pen.dim(&describe_entry(Port::Stream, &config.stream)),
    )?;

    section(out, pen, "Sinks")?;
    if config.sinks.is_empty() {
        writeln!(out, "  {}", pen.dim("(none — defaults to a stdout sink)"))?;
    } else {
        let rows: Vec<(String, String, String)> = config
            .sinks
            .iter()
            .map(|(name, entry)| {
                (
                    name.as_ref().to_owned(),
                    pen.magenta(&entry.kind),
                    pen.dim(&describe_entry(Port::Sink, entry)),
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
    diagnostic_lines(out, pen, diagnostics)
}

/// The disagreements found with no database (`check --offline`). Prints
/// nothing when there are none.
pub(crate) fn offline_diagnostics(
    out: &mut impl Write,
    pen: Pen,
    diagnostics: &[Diagnostic],
) -> Result<()> {
    if diagnostics.is_empty() {
        return Ok(());
    }
    section(out, pen, "Schema validation")?;
    diagnostic_lines(out, pen, diagnostics)
}

fn diagnostic_lines(out: &mut impl Write, pen: Pen, diagnostics: &[Diagnostic]) -> Result<()> {
    for d in diagnostics {
        let (label, code) = match d.severity {
            Severity::Error => ("error", "31"),
            Severity::Warning => ("warning", "33"),
        };
        writeln!(
            out,
            "  {} {}  {}",
            pen.paint(code, &format!("[{label}]")),
            pen.bold(&format!("{}.{}", d.index, d.dotted_path())),
            pen.dim(&d.message),
        )?;
    }
    Ok(())
}

fn flatten_resolved(fields: &[ResolvedField], depth: usize, rows: &mut Vec<Row>) {
    for field in fields {
        let nullability = if field.nullable {
            ("optional".to_owned(), "33")
        } else {
            ("required".to_owned(), "2")
        };
        rows.push(Row {
            depth,
            name: field.name.to_string(),
            cells: vec![(type_label(&field.mapping), "36"), nullability],
        });
        if field.mapping.indexed {
            flatten_resolved(&field.children, depth + 1, rows);
        }
    }
}

/// A field's mapping type, with a map's key type: `map<text>` for a map whose
/// every key is mapped as `text`. A stored-only field reads `object (not
/// indexed)`, the one property it maps to.
fn type_label(mapping: &config::Mapping) -> String {
    if !mapping.indexed {
        return "object (not indexed)".to_owned();
    }
    match &mapping.map_values {
        Some(values) => format!("map<{}>", values.mapping_type.name()),
        None => mapping.mapping_type.name().to_owned(),
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
        let dots = name_w + 3 - used;
        let leader = pen.dim(&format!(" {} ", ".".repeat(dots.max(2) - 2)));

        write!(out, "  {pad}{}{leader}", pen.bold(&row.name))?;
        for (i, (text, code)) in row.cells.iter().enumerate() {
            if i > 0 {
                write!(out, "  ")?;
            }
            write!(out, "{}", pen.paint(code, text))?;
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

/// One port entry's options on a line, `key=value` pairs in key order, without
/// interpreting them: the adapter's description says which fields are secrets,
/// and those are shown as their variable name (`${VAR}`), as a URL with the
/// password masked, or as `***`.
fn describe_entry(port: Port, entry: &PortEntry) -> String {
    let secrets: Vec<String> = adapters::description(port, &entry.kind)
        .map(|d| d.secrets.clone())
        .unwrap_or_default();
    let mut parts = Vec::new();
    for (key, value) in &entry.options.0 {
        parts.push(format!("{key}={}", describe_value(value, key, &secrets)));
    }
    parts.join(" ")
}

fn describe_value(value: &OptionValue, path: &str, secrets: &[String]) -> String {
    let secret = secrets.iter().any(|s| s == path);
    match value {
        OptionValue::Map(map) => {
            if secret && let Some(var) = map.get("env").and_then(OptionValue::as_str) {
                return format!("${{{var}}}");
            }
            let inner: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k}={}", describe_value(v, &format!("{path}.{k}"), secrets)))
                .collect();
            format!("{{{}}}", inner.join(" "))
        }
        OptionValue::String(text) if secret => {
            if text.contains("://") {
                redact_url(text)
            } else {
                "***".to_owned()
            }
        }
        OptionValue::String(text) => text.clone(),
        OptionValue::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|v| describe_value(v, path, secrets))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        OptionValue::Bool(b) => b.to_string(),
        OptionValue::Integer(i) => i.to_string(),
        OptionValue::Float(f) => f.to_string(),
        OptionValue::Null => "null".to_owned(),
    }
}

fn describe_soft_delete(sd: &SoftDelete) -> String {
    match sd {
        SoftDelete::Column(c) => format!("column \"{}\"", c.column),
        SoftDelete::Field(f) => format!("field \"{}\"", f.field),
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

#[cfg(test)]
mod tests {
    use config::{FlussoType, Mapping};

    use super::{Pen, pre_image, type_label};

    fn gap(table: &str, kept: Option<&str>) -> source::PreImageGap {
        source::PreImageGap {
            table: source::QualifiedTable::new(
                config::DatabaseSchema::try_new("public").unwrap(),
                config::TableName::try_new(table).unwrap(),
            ),
            missing: Vec::new(),
            missing_key: Vec::new(),
            manageable: true,
            blockers: Vec::new(),
            remediation: format!("ALTER TABLE {table} REPLICA IDENTITY FULL;"),
            kept: kept.map(str::to_owned),
        }
    }

    #[test]
    fn a_kept_table_says_why_and_prints_no_sql() {
        let report = source::PreImageReport {
            gaps: Vec::new(),
            partial: vec![gap("teams", None), gap("orders", Some("on an index"))],
        };
        let mut out = Vec::new();
        pre_image(&mut out, Pen { color: false }, &report, false).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(
            out.contains(
                "orders  → kept as is (on an index); updates to this table always rebuild"
            )
        );
        assert!(out.contains("ALTER TABLE teams"));
        assert!(!out.contains("ALTER TABLE orders"), "{out}");
    }

    #[test]
    fn a_partition_without_the_primary_key_says_a_delete_stays_and_prints_its_sql() {
        let report = source::PreImageReport {
            gaps: vec![source::PreImageGap {
                missing_key: vec![config::ColumnName::try_new("id").unwrap()],
                manageable: false,
                ..gap("logs", None)
            }],
            partial: Vec::new(),
        };
        let mut out = Vec::new();
        pre_image(&mut out, Pen { color: false }, &report, true).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("logs (primary key id)"), "{out}");
        assert!(
            out.contains("a delete there can't reach its document, which stays in the index"),
            "{out}"
        );
        assert!(!out.contains("re-parenting"), "{out}");
        assert!(out.contains("ALTER TABLE logs"), "{out}");
    }

    #[test]
    fn a_map_reads_as_its_key_type() {
        let mut map = Mapping::of_type(&FlussoType::Json);
        map.map_values = Some(Box::new(Mapping::of_type(&FlussoType::Text)));
        assert_eq!(type_label(&map), "map<text>");
        assert_eq!(type_label(&Mapping::of_type(&FlussoType::Long)), "long");
    }

    #[test]
    fn a_stored_only_field_reads_as_an_unindexed_object() {
        let mut stored = Mapping::of_type(&FlussoType::Json);
        stored.indexed = false;
        assert_eq!(type_label(&stored), "object (not indexed)");
    }
}
