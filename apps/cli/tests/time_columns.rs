#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

//! A `time` or `timetz` column under a `timestamp` or `date` field (issue
//! #309), on a live Postgres and OpenSearch.
//!
//! Each case is one field `v` of the root table `t`, built from two rows
//! through the real Postgres builder (wrapped in the script decorator, as `flusso
//! run` builds it) and written through the real OpenSearch
//! sink: row 1 holds a non-null time of day, row 2 only nulls. The test pins
//! which cases index row 1, and which `validate_indexes` reports as
//! [`DiagnosticKind::TimeOfDayAsDate`]: only those whose every document
//! carries the value (root or plain object) and that refuse it. One inside a
//! join or a `min`/`max` is refused only where a related row holds a time, so
//! it is not reported. The workarounds the message names (a `custom` keyword
//! field, a `timestamp` field with a `format`) index and leave `check` with
//! nothing to say.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use config::yaml::SchemaYaml;
use kernel::{Envelope, GenericValue, IndexName, IndexSchema, ParseFrom, Secret, SinkName};
use script::{ScriptedBuilder, Scripts};
use sink::Sink;
use sink_opensearch::OpensearchSink;
use source::document::{Document, DocumentId};
use source::{DiagnosticKind, RowKey, SourceSpec, validate_indexes};
use source_postgres::PgDocumentBuilder;
use sqlx::AssertSqlSafe;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{GenericImage, ImageExt};

const SETUP: &str = "
CREATE TABLE t (id int PRIMARY KEY, tm time, tz timetz, tms time[], tzs timetz[]);
CREATE TABLE r (id int PRIMARY KEY, t_id int, tm time, tz timetz);
INSERT INTO t VALUES
    (1, '03:04:05.5', '03:04:05.5+02', '{03:04:05, 23:59:59.123456}', '{03:04:05-03:30}'),
    (2, NULL, NULL, NULL, NULL);
INSERT INTO r VALUES (10, 1, '23:59:59.123456', '23:59:59-03:30');
";

/// A format that reads every `time` value: any number of fraction digits
/// (a `timestamp` field writes three or six, a `date` field Postgres's own).
const TIME_FORMAT: &str =
    "strict_date_optional_time||HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]";

/// A format that reads every `timetz` value Postgres writes: any number of
/// fraction digits, an offset with or without minutes.
const TIMETZ_FORMAT: &str = "strict_date_optional_time||HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]XXX||HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]X";

/// How a case comes out: whether row 1 (a non-null time) is indexed, whether
/// `validate_indexes` reports [`DiagnosticKind::TimeOfDayAsDate`], and whether
/// the schema is one the message advises, which must leave `check` with
/// nothing to say.
#[derive(Clone, Copy)]
struct Expect {
    indexes: bool,
    reported: bool,
    advised: bool,
}

const REFUSED_AND_REPORTED: Expect = Expect {
    indexes: false,
    reported: true,
    advised: false,
};
/// Row 1 is refused, but a parent without the related row (row 2) indexes,
/// so the start must not stop: no report.
const REFUSED_NOT_EVERYWHERE: Expect = Expect {
    indexes: false,
    reported: false,
    advised: false,
};
const INDEXES: Expect = Expect {
    indexes: true,
    reported: false,
    advised: false,
};
const ADVISED: Expect = Expect {
    indexes: true,
    reported: false,
    advised: true,
};

/// Every case: its label, the field `v`, and how it comes out.
fn cases() -> Vec<(&'static str, String, Expect)> {
    let field = |kind: &str, column: &str, extra: &str| {
        format!("- {kind}: v\n  column: {column}\n  required: false{extra}")
    };
    let aggregate = |op: &str, column: &str, ty: &str| {
        format!(
            "- {op}: v\n  table: r\n  foreign_key: t_id\n  column: {column}\n  value_type: {ty}"
        )
    };
    let join = |indexed: &str, kind: &str, column: &str| {
        format!(
            "- has_many: v\n  table: r\n  foreign_key: t_id\n  primary_key: id{indexed}\n  fields:\n    \
             - {kind}: at\n      column: {column}\n      required: false"
        )
    };
    let format = |f: &str| format!("\n  options:\n    format: \"{f}\"");
    let array = "\n  array: true";
    vec![
        ("timestamp over time", field("timestamp", "tm", ""), REFUSED_AND_REPORTED),
        ("timestamp over timetz", field("timestamp", "tz", ""), REFUSED_AND_REPORTED),
        ("date over time", field("date", "tm", ""), REFUSED_AND_REPORTED),
        ("date over timetz", field("date", "tz", ""), REFUSED_AND_REPORTED),
        ("timestamp array over time[]", field("timestamp", "tms", array), REFUSED_AND_REPORTED),
        ("timestamp array over timetz[]", field("timestamp", "tzs", array), REFUSED_AND_REPORTED),
        ("date array over time[]", field("date", "tms", array), REFUSED_AND_REPORTED),
        ("max timestamp over time", aggregate("max", "tm", "timestamp"), REFUSED_NOT_EVERYWHERE),
        ("min timestamp over timetz", aggregate("min", "tz", "timestamp"), REFUSED_NOT_EVERYWHERE),
        ("max date over time", aggregate("max", "tm", "date"), REFUSED_NOT_EVERYWHERE),
        ("timestamp over time in a has_many", join("", "timestamp", "tm"), REFUSED_NOT_EVERYWHERE),
        (
            "timestamp over timetz in a has_many",
            join("", "timestamp", "tz"),
            REFUSED_NOT_EVERYWHERE,
        ),
        (
            "timestamp over time, a format",
            field("timestamp", "tm", &format(TIME_FORMAT)),
            ADVISED,
        ),
        (
            "timestamp array over time[], a format",
            field("timestamp", "tms", &format!("{array}{}", format(TIME_FORMAT))),
            INDEXES,
        ),
        (
            "timestamp over timetz, a format",
            field("timestamp", "tz", &format(TIMETZ_FORMAT)),
            ADVISED,
        ),
        (
            "date over time, a format",
            field("date", "tm", &format(TIME_FORMAT)),
            INDEXES,
        ),
        (
            "timestamp over time, ignore_malformed",
            field(
                "timestamp",
                "tm",
                "\n  options:\n    ignore_malformed: true",
            ),
            INDEXES,
        ),
        (
            "timestamp over time in an unindexed has_many",
            join("\n  indexed: false", "timestamp", "tm"),
            INDEXES,
        ),
        (
            "timestamp over time, emit: false",
            field("timestamp", "tm", "\n  emit: false"),
            INDEXES,
        ),
        (
            "timestamp over time, a type of its own",
            field("timestamp", "tm", "\n  options:\n    type: keyword"),
            INDEXES,
        ),
        (
            "timestamp over time in an object",
            "- object: v\n  fields:\n    - timestamp: at\n      column: tm\n      required: false"
                .to_owned(),
            REFUSED_AND_REPORTED,
        ),
        (
            "custom keyword over time",
            "- custom: v\n  column: tm\n  postgres: [time without time zone]\n  opensearch: keyword\n  required: false"
                .to_owned(),
            ADVISED,
        ),
        (
            "custom keyword over timetz",
            "- custom: v\n  column: tz\n  postgres: [time with time zone]\n  opensearch: keyword\n  required: false"
                .to_owned(),
            ADVISED,
        ),
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_time_column_under_a_date_field_is_reported_only_where_no_document_indexes() {
    let pg = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .unwrap();
    let pg_url = format!(
        "postgres://postgres:postgres@127.0.0.1:{}/postgres",
        pg.get_host_port_ipv4(5432).await.unwrap()
    );
    let pool = PgPoolOptions::new().connect(&pg_url).await.unwrap();
    for statement in SETUP.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(AssertSqlSafe(statement.to_owned()))
            .execute(&pool)
            .await
            .unwrap();
    }
    let os = start_opensearch().await;
    let os_url = format!(
        "http://127.0.0.1:{}",
        os.get_host_port_ipv4(9200).await.unwrap()
    );
    let sink = opensearch_sink(&os_url);

    let mut wrong = Vec::new();
    for (i, (label, field, expect)) in cases().into_iter().enumerate() {
        let name = IndexName::try_new(format!("time_{i}")).unwrap();
        let spec = SourceSpec::new(BTreeMap::from([(name.clone(), schema(&field))]));
        let mapping = spec.index_mappings().remove(0);
        let spec = Arc::new(spec);
        let builder = PgDocumentBuilder::connect(&pg_url, Arc::clone(&spec))
            .await
            .unwrap();
        let scripts = Scripts::compile(&spec).unwrap();
        let diagnostics = validate_indexes(&spec, &builder).await.unwrap();
        let reported = diagnostics
            .iter()
            .filter(|d| d.kind == DiagnosticKind::TimeOfDayAsDate)
            .map(|d| d.message.clone())
            .collect::<Vec<_>>();
        if expect.advised && !diagnostics.is_empty() {
            wrong.push(format!(
                "{label}: an advised schema still has findings: {:?}",
                diagnostics
                    .iter()
                    .map(|d| (d.kind, &d.message))
                    .collect::<Vec<_>>()
            ));
        }

        let builder = ScriptedBuilder::wrap(Arc::new(builder), scripts);
        sink.ensure_index(&mapping).await.unwrap();
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
        for (row, document) in builder
            .build_many(&ids)
            .await
            .unwrap()
            .into_iter()
            .enumerate()
        {
            let Document::Upsert { body, .. } = document else {
                panic!("{label}: row {} did not build: {document:?}", row + 1);
            };
            let envelope = Envelope::upsert(
                name.clone(),
                (row + 1).to_string(),
                body,
                None,
                Default::default(),
            );
            sink.apply(&envelope).await.unwrap();
        }
        let report = sink.flush(true).await.unwrap();
        let reasons: Vec<String> = report.rejected.iter().map(|r| r.reason.clone()).collect();
        let rejected: Vec<String> = report.rejected.into_iter().map(|r| r.id).collect();
        let row_1_indexed = !rejected.contains(&"1".to_owned());
        if rejected.contains(&"2".to_owned()) {
            wrong.push(format!(
                "{label}: the all-null row was rejected: {reasons:?}"
            ));
        }
        if row_1_indexed != expect.indexes {
            wrong.push(format!(
                "{label}: row 1 indexed: {row_1_indexed}, expected {}",
                expect.indexes
            ));
        }
        if reported.is_empty() == expect.reported {
            wrong.push(format!(
                "{label}: reported {reported:?}, expected a report: {}",
                expect.reported
            ));
        }
        if !reported.is_empty() && row_1_indexed {
            wrong.push(format!("{label}: reported, yet row 1 indexed"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The data behind the start cases: two root rows, one related row under the
/// first, each time non-null where set.
const START_SETUP: &str = "
CREATE TABLE t (id int PRIMARY KEY, k text, tags text[], tm time);
CREATE TABLE r (id int PRIMARY KEY, t_id int, k text, tm time);
INSERT INTO t VALUES (1, 'x', '{a}', '03:04:05'), (2, NULL, NULL, NULL);
INSERT INTO r VALUES (10, 1, 'y', '03:04:05');
";

/// A time field the start must not stop on (issue #309 review) starts and
/// indexes what it did on 0.17, through the real `flusso run`: one with a
/// mapping `type` of its own, one a pipeline set as an index setting removes,
/// one inside a `has_many` (a parent with no related row indexes). The plain
/// root field still refuses the start.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_time_field_some_document_survives_starts_and_indexes() {
    let (_pg, pg_url) = common::start_postgres(START_SETUP).await;
    let (_os, os_url) = common::start_opensearch().await;
    common::pipeline_removing_v(&os_url, "drop-v").await;

    let plain = "- timestamp: v\n  column: tm\n  required: false";
    let cases: Vec<(&str, &str, &str, u64)> = vec![
        (
            "own_type",
            "- timestamp: v\n  column: tm\n  required: false\n  options:\n    type: keyword",
            "",
            2,
        ),
        (
            "index_pipeline",
            plain,
            "[index.sinks.primary]\nindex_settings = { index = { default_pipeline = \"drop-v\" } }",
            2,
        ),
        (
            "in_a_join",
            "- has_many: v\n  table: r\n  foreign_key: t_id\n  primary_key: id\n  fields:\n    \
             - timestamp: at\n      column: tm\n      required: false",
            "",
            1,
        ),
    ];
    let mut wrong = Vec::new();
    for (name, fields, index_extra, expected) in cases {
        let deployment = common::Deployment {
            name,
            fields,
            index_extra,
            sink_extra: "",
        };
        match common::run(&pg_url, &os_url, &deployment, expected).await {
            common::Outcome::Indexed(_) => {}
            other => wrong.push(format!(
                "{name}: expected {expected} documents, got {other:?}"
            )),
        }
    }
    let stopping = common::Deployment {
        name: "root_time",
        fields: plain,
        index_extra: "",
        sink_extra: "",
    };
    match common::run(&pg_url, &os_url, &stopping, 2).await {
        common::Outcome::Exited(stderr) if stderr.contains("cannot be built") => {}
        other => wrong.push(format!(
            "root_time: expected a refused start, got {other:?}"
        )),
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

fn schema(field: &str) -> IndexSchema {
    let mut yaml =
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n"
            .to_owned();
    for line in field.lines() {
        yaml.push_str("  ");
        yaml.push_str(line);
        yaml.push('\n');
    }
    IndexSchema::try_from(SchemaYaml::try_parse(&yaml).unwrap())
        .unwrap_or_else(|e| panic!("{e}\n{yaml}"))
}

async fn start_opensearch() -> testcontainers_modules::testcontainers::ContainerAsync<GenericImage>
{
    GenericImage::new("opensearchproject/opensearch", "2")
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
        .expect("opensearch container should start")
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
