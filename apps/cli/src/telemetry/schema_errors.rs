//! The schema errors `flusso run` starts with anyway: an index that starting
//! flusso does not make work as its schema says (issue #485), whose kind does
//! not stop the start (`backends`'s `stops_the_start`).
//!
//! Each one is logged at `error` with its index, kind, field path and message,
//! at every start, so it reaches the log and the OTel export like any other
//! event. Each (index, kind) pair is also set on `flusso.index.schema_errors`,
//! a gauge of how many such errors the last start found: an alert on `> 0`
//! fires while one is there, and a start after the fix has no series for it.
//! A gap in capture `flusso run` won't close (`publication_gap`,
//! `replica_identity_gap`, see [`crate::gaps`]) is reported the same way,
//! once per index that reads its table.
//! A gauge, not a counter: the value is a state of the deployment, not
//! something that happens, and a counter would read 1 for the life of a
//! process whatever its rate window.
//!
//! ```text
//! flusso_index_schema_errors{index="products",kind="undeclared_array_column"} 1
//! ```
//!
//! **One copy per reader.** The start's printed report and these events both
//! go to stderr, so who reads stderr decides ([`Reader::of_stderr`]): at a
//! terminal the report prints them and the events stay off the stderr log
//! ([`on_the_stderr_log`]); into a log the events are the copy and the report
//! leaves them out. Either way the events reach the OTel export, where with no
//! log bridge they are span events of the `daemon.start` span.

use std::collections::BTreeMap;
use std::io::IsTerminal;

use opentelemetry::metrics::Meter;
use opentelemetry::{KeyValue, global};
use source::{Diagnostic, Severity};

/// The target of the events, by which the stderr log leaves them out at a
/// terminal.
pub(crate) const TARGET: &str = "flusso::schema_errors";

/// Who reads stderr: a person at a terminal, or a log a deployment collects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reader {
    /// stderr is a terminal: the printed report is read.
    Person,
    /// stderr goes elsewhere: the log lines are read.
    Log,
}

impl Reader {
    /// Who reads this process's stderr.
    pub(crate) fn of_stderr() -> Self {
        if std::io::stderr().is_terminal() {
            Self::Person
        } else {
            Self::Log
        }
    }
}

/// Whether the stderr log writes an event of `target` for `reader`: every
/// event but these, which a person reads in the report instead.
pub(crate) fn on_the_stderr_log(target: &str, reader: Reader) -> bool {
    reader == Reader::Log || target != TARGET
}

/// Log every error-severity diagnostic in `diagnostics` and set the gauge on
/// the global meter. Warnings are left to the printed report.
pub(crate) fn report(diagnostics: &[Diagnostic]) {
    record(&global::meter("flusso"), diagnostics);
}

pub(crate) fn record(meter: &Meter, diagnostics: &[Diagnostic]) {
    let errors = diagnostics.iter().filter(|d| d.severity == Severity::Error);
    let mut counts: BTreeMap<(String, &'static str), u64> = BTreeMap::new();
    for d in errors {
        tracing::error!(
            target: TARGET,
            index = %d.index,
            kind = d.kind.as_str(),
            field = %d.dotted_path(),
            "{}",
            d.message
        );
        *counts
            .entry((d.index.to_string(), d.kind.as_str()))
            .or_default() += 1;
    }
    set(meter, counts);
}

/// Log every gap in capture the start goes on with, once per index that
/// reads its table, and set the gauge on the global meter.
pub(crate) fn report_gaps(gaps: &[crate::gaps::Gap]) {
    record_gaps(&global::meter("flusso"), gaps);
}

pub(crate) fn record_gaps(meter: &Meter, gaps: &[crate::gaps::Gap]) {
    let mut counts: BTreeMap<(String, &'static str), u64> = BTreeMap::new();
    for gap in gaps {
        for index in &gap.indexes {
            tracing::error!(
                target: TARGET,
                index = %index,
                kind = gap.kind.as_str(),
                table = %gap.table,
                "{}",
                gap.message
            );
            *counts
                .entry((index.to_string(), gap.kind.as_str()))
                .or_default() += 1;
        }
    }
    set(meter, counts);
}

fn set(meter: &Meter, counts: BTreeMap<(String, &'static str), u64>) {
    if counts.is_empty() {
        return;
    }
    let gauge = meter
        .u64_gauge("flusso.index.schema_errors")
        .with_description(
            "Errors `flusso check` reports that the last start went on with, by index and kind: \
             the index does not hold what its schema says, or misses changes (a gap in capture \
             `flusso run` won't close). `flusso run --strict` refuses them",
        )
        .build();
    for ((index, kind), count) in counts {
        gauge.record(
            count,
            &[KeyValue::new("index", index), KeyValue::new("kind", kind)],
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::{Arc, Mutex};

    use opentelemetry::metrics::MeterProvider as _;
    use prometheus::{Registry, TextEncoder};
    use source::{Anchor, DiagnosticKind};
    use tracing::field::{Field, Visit};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;
    use crate::telemetry::metrics::provider_builder;

    fn finding(index: &str, kind: DiagnosticKind, field: &str) -> Diagnostic {
        Diagnostic::new(
            kernel::IndexName::try_new(index).unwrap(),
            kernel::FieldName::try_new(field).unwrap(),
            Anchor::Field,
            kind,
            format!("a {kind:?} finding"),
        )
    }

    fn findings() -> Vec<Diagnostic> {
        vec![
            finding("products", DiagnosticKind::UndeclaredArrayColumn, "tags"),
            finding("products", DiagnosticKind::UndeclaredArrayColumn, "codes"),
            finding("products", DiagnosticKind::Nullability, "name"),
            finding("users", DiagnosticKind::IdsElementLossy, "teamIds"),
            finding("users", DiagnosticKind::IgnoredPrimaryKey, "team"),
        ]
    }

    #[test]
    fn each_index_and_kind_is_a_gauge_of_its_errors() {
        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .scope_info_enabled(false)
            .build()
            .unwrap();
        let provider = provider_builder().with_reader(exporter).build();
        record(&provider.meter("flusso"), &findings());
        let text = TextEncoder::new()
            .encode_to_string(&registry.gather())
            .unwrap();
        for line in [
            r#"flusso_index_schema_errors{index="products",kind="undeclared_array_column"} 2"#,
            r#"flusso_index_schema_errors{index="products",kind="nullability"} 1"#,
            r#"flusso_index_schema_errors{index="users",kind="ids_element_lossy"} 1"#,
        ] {
            assert!(text.contains(line), "{line} missing from:\n{text}");
        }
        assert!(
            !text.contains("ignored_primary_key"),
            "a warning is no error:\n{text}"
        );
    }

    #[derive(Default)]
    struct Fields(Vec<(String, String)>);

    impl Visit for Fields {
        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.push((field.name().to_owned(), value.to_owned()));
        }
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.push((field.name().to_owned(), format!("{value:?}")));
        }
    }

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<(tracing::Level, Vec<(String, String)>)>>>);

    impl<S: tracing::Subscriber> Layer<S> for Capture {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut fields = Fields::default();
            fields
                .0
                .push(("target".to_owned(), event.metadata().target().to_owned()));
            event.record(&mut fields);
            self.0
                .lock()
                .unwrap()
                .push((*event.metadata().level(), fields.0));
        }
    }

    /// A gap in capture is counted on each index that reads its table, under
    /// its own kind.
    #[test]
    fn each_gap_is_counted_on_every_index_that_reads_its_table() {
        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .scope_info_enabled(false)
            .build()
            .unwrap();
        let provider = provider_builder().with_reader(exporter).build();
        let gap = crate::gaps::Gap {
            kind: crate::gaps::GapKind::Publication,
            table: source::QualifiedTable::new(
                config::DatabaseSchema::try_new("public").unwrap(),
                config::TableName::try_new("orders").unwrap(),
            ),
            indexes: vec![
                kernel::IndexName::try_new("orders").unwrap(),
                kernel::IndexName::try_new("users").unwrap(),
            ],
            message: "not in the publication".to_owned(),
        };
        record_gaps(&provider.meter("flusso"), &[gap]);
        let text = TextEncoder::new()
            .encode_to_string(&registry.gather())
            .unwrap();
        for line in [
            r#"flusso_index_schema_errors{index="orders",kind="publication_gap"} 1"#,
            r#"flusso_index_schema_errors{index="users",kind="publication_gap"} 1"#,
        ] {
            assert!(text.contains(line), "{line} missing from:\n{text}");
        }
    }

    #[test]
    fn the_stderr_log_leaves_the_events_to_the_report_only_at_a_terminal() {
        assert!(!on_the_stderr_log(TARGET, Reader::Person));
        assert!(on_the_stderr_log(TARGET, Reader::Log));
        assert!(on_the_stderr_log("flusso::daemon", Reader::Person));
    }

    #[test]
    fn each_error_is_an_error_event_with_its_index_kind_and_field() {
        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        let meter = provider_builder().build().meter("flusso");
        tracing::subscriber::with_default(subscriber, || record(&meter, &findings()));
        let events = capture.0.lock().unwrap().clone();
        assert_eq!(
            events.len(),
            4,
            "one per error, none for a warning: {events:?}"
        );
        assert!(
            events
                .iter()
                .all(|(level, _)| *level == tracing::Level::ERROR)
        );
        assert!(
            events.iter().all(|(_, fields)| fields
                .iter()
                .any(|(name, value)| name == "target" && value == TARGET)),
            "every event carries the target: {events:?}"
        );
        let field = |at: usize, name: &str| {
            events[at]
                .1
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(field(0, "index"), "products");
        assert_eq!(field(0, "kind"), "undeclared_array_column");
        assert_eq!(field(0, "field"), "tags");
        assert_eq!(field(0, "message"), "a UndeclaredArrayColumn finding");
        assert_eq!(field(3, "index"), "users");
        assert_eq!(field(3, "kind"), "ids_element_lossy");
    }
}
