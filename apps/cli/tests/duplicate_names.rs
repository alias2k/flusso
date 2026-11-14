#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout
)]

//! The live matrix behind the `duplicate_field_name` warning (issue #284).
//!
//! For every ordered pair of field kinds that can share a name, an index whose
//! root holds both fields under the name `v` is built from two rows through the
//! real Postgres builder (wrapped in the script decorator, as `flusso run`
//! builds it) and written through the real OpenSearch sink, under the mapping
//! flusso derives for it. What happened is classified (fails to build, rejected
//! by the sink, no value, the value one of the two builds on its own, the last
//! one's value cut to the first one's child fields, or something else) and
//! must match [`source::duplicate_outcome`], the classification the warning's
//! text is built from. Where the classifier says it cannot tell
//! ([`DuplicateOutcome::Unknown`]), anything goes: its wording says so.
//!
//! Row 1 has related rows and a set `belongs_to` link; row 2 has none, so a
//! pair whose outcome depends on related rows is seen both ways.
//!
//! `DUPLICATE_MATRIX=print` prints the whole matrix as a Markdown table. The
//! manual's "One name per level" table is written by hand; check it against
//! that print when an outcome changes.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use config::yaml::SchemaYaml;
use kernel::{Envelope, Field, GenericValue, IndexName, IndexSchema, ParseFrom, Secret, SinkName};
use script::{ScriptedBuilder, Scripts};
use serde_json::{Value, json};
use sink::Sink;
use sink_opensearch::OpensearchSink;
use source::document::{Document, DocumentId};
use source::{DuplicateOutcome, RowKey, SourceSpec};
use source_postgres::PgDocumentBuilder;
use sqlx::AssertSqlSafe;
use sqlx::postgres::{PgPool, PgPoolOptions};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::Semaphore;

/// Every kind of field that can share a name: its label and its YAML, as a
/// field named `v` of the root table `t`.
const KINDS: &[(&str, &str)] = &[
    ("keyword", "- keyword: v\n  column: k\n  required: false"),
    ("text", "- text: v\n  column: k\n  required: false"),
    ("integer", "- integer: v\n  column: n\n  required: false"),
    ("double", "- double: v\n  column: f\n  required: false"),
    ("decimal", "- decimal: v\n  column: dec\n  required: false"),
    ("boolean", "- boolean: v\n  column: b\n  required: false"),
    (
        "timestamp",
        "- timestamp: v\n  column: d\n  required: false",
    ),
    ("uuid", "- uuid: v\n  column: u\n  required: false"),
    (
        "enum",
        "- enum: v\n  column: e\n  required: false\n  variants: [a, b]",
    ),
    ("binary", "- binary: v\n  column: bin\n  required: false"),
    ("json", "- json: v\n  column: j\n  required: false"),
    (
        "json unindexed",
        "- json: v\n  column: j\n  required: false\n  indexed: false",
    ),
    (
        "map",
        "- map: v\n  column: m\n  values: keyword\n  required: false",
    ),
    ("geo", "- geo: v\n  lat: lat\n  lon: lon\n  required: false"),
    (
        "custom",
        "- custom: v\n  column: n\n  postgres: [integer]\n  opensearch: long\n  required: false",
    ),
    (
        "keyword array",
        "- keyword: v\n  column: tags\n  array: true\n  required: false",
    ),
    (
        "json array",
        "- json: v\n  column: jarr\n  array: true\n  required: false",
    ),
    (
        "has_many",
        "- has_many: v\n  table: r\n  foreign_key: t_id\n  primary_key: id\n  fields:\n    - keyword: label\n      required: false",
    ),
    (
        "has_many unindexed",
        "- has_many: v\n  table: r\n  foreign_key: t_id\n  primary_key: id\n  indexed: false\n  fields:\n    - keyword: label\n      required: false",
    ),
    (
        "has_one",
        "- has_one: v\n  table: r\n  foreign_key: t_id\n  primary_key: id\n  required: false\n  fields:\n    - keyword: label\n      required: false",
    ),
    (
        "belongs_to",
        "- belongs_to: v\n  table: c2\n  column: c_id\n  primary_key: id\n  required: false\n  fields:\n    - keyword: label\n      required: false\n    - keyword: kind\n      required: false",
    ),
    (
        "many_to_many",
        "- many_to_many: v\n  table: r\n  through:\n    table: tj\n    left_key: t_id\n    right_key: r_id\n  primary_key: id\n  fields:\n    - keyword: label\n      required: false",
    ),
    (
        "object",
        "- object: v\n  fields:\n    - keyword: k\n      required: false",
    ),
    ("count", "- count: v\n  table: r\n  foreign_key: t_id"),
    (
        "sum",
        "- sum: v\n  table: r\n  foreign_key: t_id\n  column: id\n  value_type: long",
    ),
    (
        "ids",
        "- ids: v\n  table: r\n  foreign_key: t_id\n  element_type: integer",
    ),
    ("exists", "- exists: v\n  table: r\n  foreign_key: t_id"),
    (
        "sum decimal",
        "- sum: v\n  table: r\n  foreign_key: t_id\n  column: id\n  value_type: decimal",
    ),
    ("constant", "- constant: v\n  value: 5"),
    ("constant string", "- constant: v\n  value: hello"),
    ("constant true", "- constant: v\n  value: true"),
    ("constant 2.5", "- constant: v\n  value: 2.5"),
    ("constant list", "- constant: v\n  value: [1, 2]"),
    ("constant mixed list", "- constant: v\n  value: [1, a]"),
    ("constant object", "- constant: v\n  value: { a: 1 }"),
    (
        "script",
        "- keyword: v\n  script: '\"s\"'\n  required: false",
    ),
    (
        "script array",
        "- keyword: v\n  script: '[\"s\"]'\n  array: true\n  required: false",
    ),
    (
        "keyword emit: false",
        "- keyword: v\n  column: k\n  required: false\n  emit: false",
    ),
];

const SETUP: &str = "
CREATE TABLE c2 (id int PRIMARY KEY, label text, kind text);
CREATE TABLE t (
    id int PRIMARY KEY, k text, n int, f float8, b bool, d timestamptz, u uuid, e text,
    bin bytea, j jsonb, m jsonb, lat float8, lon float8, tags text[], jarr jsonb,
    c_id int REFERENCES c2(id), dec numeric
);
CREATE TABLE r (id int PRIMARY KEY, t_id int, label text);
CREATE TABLE tj (t_id int, r_id int, PRIMARY KEY (t_id, r_id));
INSERT INTO c2 VALUES (1, 'cat', 'pet');
INSERT INTO t VALUES
    (1, 'x', 7, 1.5, true, '2026-01-02T03:04:05Z', '6f1c2b1a-0000-4000-8000-000000000001', 'a',
     '\\x0102', '{\"a\": 1, \"label\": \"j\"}', '{\"en\": \"hi\"}', 1.5, 2.5, '{p,q}',
     '[{\"a\": 1}]', 1, 1.25),
    (2, 'y', 8, 2.5, false, '2026-02-03T04:05:06Z', '6f1c2b1a-0000-4000-8000-000000000002', 'b',
     '\\x0304', '{\"b\": 2}', '{\"it\": \"ciao\"}', 3.5, 4.5, '{r}', '[{\"b\": 2}]', NULL, 2.5);
INSERT INTO r VALUES (10, 1, 'one'), (11, 1, 'two');
INSERT INTO tj VALUES (1, 10);
";

/// What one pair (or one kind on its own) did to the two rows.
#[derive(Debug, Clone)]
struct Run {
    /// Per row: the rendered value under `v` (`None`: the name is absent), or
    /// the build failure.
    rows: Vec<Result<Option<Value>, String>>,
    /// Per row: the sink's rejection, when it refused the document.
    rejected: Vec<Option<String>>,
    /// Per row: the built document, as the sink gets it.
    bodies: Vec<Option<GenericValue>>,
}

/// What happened to a pair, in the classifier's terms.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seen {
    FailsToBuild,
    Rejected,
    Nothing,
    /// Every document holds `null` under the name.
    Null,
    /// A text rendering of the last one's value (in a one-element array or
    /// not), which neither builds on its own.
    AsText,
    /// The value the namesake at this position builds on its own, with the
    /// same verdict from the sink.
    Holds(usize),
    /// The last one's value in a one-element array.
    Wrapped,
    /// The last one's value cut to the first one's child fields.
    FirstsChildren,
    Other,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires docker"]
async fn the_duplicate_warning_says_what_a_live_pipeline_does() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;
    for statement in SETUP.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(AssertSqlSafe(statement.to_owned()))
            .execute(&pg.pool)
            .await
            .unwrap();
    }
    let http = reqwest::Client::new();

    let mut alone = Vec::new();
    for (i, (_, yaml)) in KINDS.iter().enumerate() {
        let index = format!("alone_{i}");
        let (run, mapping) = build(&pg.url, &index, schema(&[yaml])).await;
        let sink = opensearch_sink(&os.url);
        alone.push(write(&sink, &http, &os.url, &index, run, mapping).await);
    }

    let gate = Arc::new(Semaphore::new(6));
    let mut tasks = Vec::new();
    for (a, (_, first)) in KINDS.iter().enumerate() {
        for (b, (_, last)) in KINDS.iter().enumerate() {
            let schema = schema(&[first, last]);
            let (url, os_url) = (pg.url.clone(), os.url.clone());
            let (http, gate) = (http.clone(), Arc::clone(&gate));
            tasks.push(tokio::spawn(async move {
                let _permit = gate.acquire_owned().await.unwrap();
                let index = format!("p_{a}_{b}");
                let fields = schema.fields.clone();
                let (run, mapping) = build(&url, &index, schema).await;
                // A sink per pair: a flush covers everything its sink holds.
                let sink = opensearch_sink(&os_url);
                let run = write(&sink, &http, &os_url, &index, run, mapping).await;
                (a, b, fields, run)
            }));
        }
    }

    let mut table = Vec::new();
    let mut mismatches = Vec::new();
    for task in tasks {
        let (a, b, fields, run) = task.await.unwrap();
        let named: Vec<&Field> = fields.iter().filter(|f| f.field.as_ref() == "v").collect();
        let predicted = source::duplicate_outcome(&named);
        let firsts_children = named[0]
            .children()
            .iter()
            .map(|c| c.field.to_string())
            .collect::<Vec<_>>();
        let seen = classify(&run, &[&alone[a], &alone[b]], &firsts_children);
        let agrees = match &predicted {
            DuplicateOutcome::Unknown => true,
            DuplicateOutcome::FailsToBuild { .. } => seen == Seen::FailsToBuild,
            DuplicateOutcome::Nothing => seen == Seen::Nothing,
            DuplicateOutcome::Null => seen == Seen::Null,
            DuplicateOutcome::AsText { .. } => seen == Seen::AsText,
            // Namesakes that build the same on their own can't be told apart.
            DuplicateOutcome::Holds(at) => {
                seen == Seen::Holds(*at)
                    || (matches!(seen, Seen::Holds(_))
                        && alone[a].rows == alone[b].rows
                        && alone[a].rejected == alone[b].rejected)
            }
            DuplicateOutcome::Wrapped => seen == Seen::Wrapped,
            DuplicateOutcome::FirstsChildren { .. } => seen == Seen::FirstsChildren,
            DuplicateOutcome::Rejected {
                unless_it_fits: Some(at),
                ..
            } => seen == Seen::Rejected || seen == Seen::Holds(*at),
            DuplicateOutcome::Rejected { .. } => seen == Seen::Rejected,
            _ => false,
        };
        if !agrees {
            mismatches.push(format!(
                "{} then {}: predicted {predicted:?}, saw {seen:?} ({run:?})",
                KINDS[a].0, KINDS[b].0
            ));
        }
        table.push((a, b, predicted, seen, run));
    }

    if std::env::var("DUPLICATE_MATRIX").as_deref() == Ok("print") {
        println!("| first | last | what happens | classified as |");
        println!("| --- | --- | --- | --- |");
        for (a, b, predicted, seen, run) in &table {
            println!(
                "| {} | {} | {} | {} |",
                KINDS[*a].0,
                KINDS[*b].0,
                describe(seen, run),
                short(predicted)
            );
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {} pairs disagree with the warning:\n{}",
        mismatches.len(),
        table.len(),
        mismatches.join("\n")
    );
}

/// The pair's outcome: a failed build first, then what the documents hold.
fn classify(run: &Run, alone: &[&Run; 2], firsts_children: &[String]) -> Seen {
    if run.rows.iter().any(Result::is_err) {
        return Seen::FailsToBuild;
    }
    let values: Vec<Option<&Value>> = run
        .rows
        .iter()
        .map(|row| row.as_ref().ok().and_then(Option::as_ref))
        .collect();
    if values.iter().all(Option::is_none) {
        return Seen::Nothing;
    }
    if values.iter().all(|v| v == &Some(&Value::Null)) && run.rejected.iter().all(Option::is_none) {
        return Seen::Null;
    }
    let refused = |r: &Run| r.rejected.iter().map(Option::is_some).collect::<Vec<_>>();
    let same_as = |other: &Run, change: &dyn Fn(&Value) -> Value| {
        values.iter().zip(&other.rows).all(|(value, theirs)| {
            let theirs = theirs.as_ref().ok().and_then(Option::as_ref).map(change);
            value.map(normalized) == theirs.as_ref().map(normalized)
        })
    };
    let wrapped = |v: &Value| match v {
        Value::Null | Value::Array(_) => v.clone(),
        other => Value::Array(vec![other.clone()]),
    };
    let accepted = run.rejected.iter().all(Option::is_none);
    if same_as(alone[1], &Value::clone) && refused(run) == refused(alone[1]) {
        return Seen::Holds(1);
    }
    if same_as(alone[0], &Value::clone) && refused(run) == refused(alone[0]) {
        return Seen::Holds(0);
    }
    if same_as(alone[1], &wrapped) && accepted {
        return Seen::Wrapped;
    }
    if same_as(alone[1], &|v| projected(v, firsts_children)) && accepted {
        return Seen::FirstsChildren;
    }
    if !accepted {
        return Seen::Rejected;
    }
    let text = |v: &Option<&Value>| match v {
        Some(Value::String(_)) => true,
        Some(Value::Array(items)) => matches!(items.as_slice(), [Value::String(_)]),
        _ => false,
    };
    if values.iter().all(text) {
        return Seen::AsText;
    }
    Seen::Other
}

/// `value` with each object (or each object element) cut to `keys`.
fn projected(value: &Value, keys: &[String]) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| keys.contains(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(|i| projected(i, keys)).collect()),
        other => other.clone(),
    }
}

/// `value` with every number as a float, so `7` and `7.0` compare equal.
fn normalized(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64()),
        Value::Array(items) => Value::Array(items.iter().map(normalized).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), normalized(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn describe(seen: &Seen, run: &Run) -> String {
    let rows = |f: &dyn Fn(usize) -> String| (0..run.rows.len()).map(f).collect::<Vec<_>>();
    match seen {
        Seen::FailsToBuild => rows(&|i| match &run.rows[i] {
            Err(_) => format!("row {}: fails to build", i + 1),
            Ok(_) => format!("row {}: builds", i + 1),
        })
        .join("; "),
        Seen::Rejected => rows(&|i| match &run.rejected[i] {
            Some(_) => format!("row {}: rejected", i + 1),
            None => format!("row {}: indexed", i + 1),
        })
        .join("; "),
        Seen::Nothing => "no value".to_owned(),
        Seen::Null => "`null`".to_owned(),
        Seen::AsText => "the last one's value as text".to_owned(),
        Seen::Holds(0) => "the first one's value".to_owned(),
        Seen::Holds(_) => "the last one's value".to_owned(),
        Seen::FirstsChildren => "the last one's value cut to the first one's children".to_owned(),
        Seen::Wrapped => "the last one's value in a one-element array".to_owned(),
        Seen::Other => rows(&|i| match &run.rows[i] {
            Ok(v) => format!(
                "row {}: `{}`",
                i + 1,
                v.as_ref().map_or("absent".into(), Value::to_string)
            ),
            Err(e) => format!("row {}: {e}", i + 1),
        })
        .join("; "),
    }
}

fn short(outcome: &DuplicateOutcome) -> String {
    match outcome {
        DuplicateOutcome::Holds(0) => "first's value".to_owned(),
        DuplicateOutcome::Holds(_) => "last's value".to_owned(),
        DuplicateOutcome::FirstsChildren { .. } => "first's children".to_owned(),
        DuplicateOutcome::Wrapped => "wrapped".to_owned(),
        DuplicateOutcome::Nothing => "nothing".to_owned(),
        DuplicateOutcome::Null => "null".to_owned(),
        DuplicateOutcome::AsText { .. } => "as text".to_owned(),
        DuplicateOutcome::FailsToBuild { .. } => "fails to build".to_owned(),
        DuplicateOutcome::Rejected { .. } => "rejected".to_owned(),
        _ => "general wording".to_owned(),
    }
}

fn schema(fields: &[&str]) -> IndexSchema {
    let mut yaml =
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n"
            .to_owned();
    for field in fields {
        for line in field.lines() {
            yaml.push_str("  ");
            yaml.push_str(line);
            yaml.push('\n');
        }
    }
    IndexSchema::try_from(SchemaYaml::try_parse(&yaml).unwrap())
        .unwrap_or_else(|e| panic!("{e}\n{yaml}"))
}

/// Build rows 1 and 2 of `schema` as index `index` the way `flusso run` does.
async fn build(url: &str, index: &str, schema: IndexSchema) -> (Run, kernel::IndexMapping) {
    let name = IndexName::try_new(index).unwrap();
    let spec = SourceSpec::new(BTreeMap::from([(name.clone(), schema)]));
    let scripts = Scripts::compile(&spec).unwrap();
    let mapping = spec.index_mappings().remove(0);
    let builder = PgDocumentBuilder::connect(url, Arc::new(spec))
        .await
        .unwrap();
    let builder = ScriptedBuilder::wrap(Arc::new(builder), scripts);
    let ids: Vec<DocumentId> = [1, 2]
        .into_iter()
        .map(|id| DocumentId {
            index: name.clone(),
            key: RowKey(vec![(
                kernel::ColumnName::try_new("id").unwrap(),
                GenericValue::Int(id),
            )]),
        })
        .collect();
    let mut rows = Vec::new();
    let mut bodies = Vec::new();
    for document in builder.build_many(&ids).await.unwrap() {
        match document {
            Document::Upsert { body, .. } => {
                rows.push(Ok(sink::to_json(&body).get("v").cloned()));
                bodies.push(Some(body));
            }
            Document::Failed { failure, .. } => {
                rows.push(Err(failure.message));
                bodies.push(None);
            }
            Document::Delete { .. } => {
                rows.push(Err("deleted".to_owned()));
                bodies.push(None);
            }
        }
    }
    (
        Run {
            rows,
            rejected: Vec::new(),
            bodies,
        },
        mapping,
    )
}

/// Write the built rows through the OpenSearch sink and record its verdict on
/// each, then drop the index.
async fn write(
    sink: &dyn Sink,
    http: &reqwest::Client,
    os_url: &str,
    index: &str,
    mut run: Run,
    mapping: kernel::IndexMapping,
) -> Run {
    let name = IndexName::try_new(index).unwrap();
    sink.ensure_index(&mapping).await.unwrap();
    for (i, body) in run.bodies.iter().enumerate() {
        if let Some(body) = body {
            let envelope = Envelope::upsert(
                name.clone(),
                (i + 1).to_string(),
                body.clone(),
                None,
                Default::default(),
            );
            sink.apply(&envelope).await.unwrap();
        }
    }
    let report = sink.flush(true).await.unwrap();
    run.rejected = vec![None; run.rows.len()];
    for rejection in report.rejected {
        let row: usize = rejection.id.parse::<usize>().unwrap() - 1;
        run.rejected[row] = Some(rejection.reason);
    }
    // `{index}_*`, not `{index}*`: `p_1_2*` also matches `p_1_20…`, the indexes
    // of pairs still being written.
    let _ = http.delete(format!("{os_url}/{index}_*")).send().await;
    run
}

struct Pg {
    _container: ContainerAsync<Postgres>,
    pool: PgPool,
    url: String,
}

struct Os {
    _container: ContainerAsync<GenericImage>,
    url: String,
}

async fn start_postgres() -> Pg {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    Pg {
        _container: container,
        pool,
        url,
    }
}

async fn start_opensearch() -> Os {
    let container = GenericImage::new("opensearchproject/opensearch", "2")
        .with_exposed_port(9200.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/_cluster/health")
                .with_port(9200.tcp())
                .with_expected_status_code(200u16)
                .with_poll_interval(Duration::from_secs(1)),
        ))
        .with_env_var("discovery.type", "single-node")
        .with_env_var("DISABLE_SECURITY_PLUGIN", "true")
        .with_env_var("DISABLE_INSTALL_DEMO_CONFIG", "true")
        .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("opensearch container should start");
    let port = container.get_host_port_ipv4(9200).await.unwrap();
    Os {
        _container: container,
        url: format!("http://127.0.0.1:{port}"),
    }
}

fn opensearch_sink(os_url: &str) -> OpensearchSink {
    let config = sink_opensearch::OpensearchConfig {
        url: Secret::Value(os_url.to_owned()),
        username: None,
        password: None,
        tls_verify: false,
        ca_file: None,
        batch_size: 1000,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 30,
        max_retries: 3,
        pipeline: None,
        number_of_shards: 1,
        number_of_replicas: 0,
        refresh_interval: "1s".to_owned(),
        text_analysis: sink_opensearch::TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    };
    OpensearchSink::from_config(&SinkName::try_new("e2e").unwrap(), &config).unwrap()
}
