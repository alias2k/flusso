//! A filter compares its value as written, whatever the size of its column
//! (issue #338).
//!
//! An explicit cast to a sized type changes a value without an error:
//! `'archived'::varchar(5)` is `archi`, `1.005::numeric(10,2)` is `1.01`,
//! `'101'::bit(5)` is `10100`, `'12:00:00.5'::time(0)` is `12:00:01`. A filter
//! operand cast that way matches a row written for another value, and turns
//! `neq`/`not_in` into excluding the row they should keep. Each case here is
//! a value that does not fit its column, built both one document at a time
//! and batched:
//!
//! - every sized type (`varchar(n)`, `numeric(p,s)`, `char(n)`, `bit(n)`,
//!   `varbit(n)`, `time(p)`, `timestamp(p)`, `timestamptz(p)`, `interval(p)`,
//!   `interval year`, a domain over `varchar(n)` and one over `numeric(p,s)`,
//!   and a domain whose `CHECK` refuses the value) under every operator that
//!   takes a value, as a root filter;
//! - a `varchar(n)` and a `numeric(p,s)` value in every place a filter sits:
//!   the root, a soft-delete `when`, a `has_many`, a `belongs_to`, a
//!   `many_to_many` target and its junction, an aggregate, an aggregate's
//!   junction and a `via` hop.
//!
//! `char(n)` keeps its own equality: trailing spaces never count, so `ab`
//! equals a stored `ab   ` and `abcde  ` equals `abcde`; any other character
//! past the size is a value no row holds.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-source-postgres --test filter_operands --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::Arc;

use config::ParseFrom;
use config::yaml::SchemaYaml;
use kernel::{ColumnName, GenericValue, IndexName, IndexSchema};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowKey, SourceSpec};
use source_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// `(filter, matches)`: a root filter on the one row of `sized`, and whether
/// that row is a document under it.
const ROOT_CASES: &[(&str, bool)] = &[
    // varchar(5) holding `archi`
    ("{ column: s, op: eq, value: 'archived' }", false),
    ("{ column: s, op: neq, value: 'archived' }", true),
    ("{ column: s, op: in, value: ['archived'] }", false),
    ("{ column: s, op: not_in, value: ['archived'] }", true),
    ("{ column: s, op: gte, value: 'archived' }", false),
    ("{ column: s, op: lt, value: 'archived' }", true),
    ("{ column: s, op: like, value: 'archived%' }", false),
    ("{ column: s, op: ilike, value: 'ARCHIVED%' }", false),
    (
        "{ column: s, op: between, value: ['archived', 'archivez'] }",
        false,
    ),
    ("{ column: s, op: eq, value: 'archi' }", true),
    ("{ column: s, op: eq, value: 'archi  ' }", false),
    // numeric(10,2) holding 1.01
    ("{ column: n, op: eq, value: '1.005' }", false),
    ("{ column: n, op: neq, value: '1.005' }", true),
    ("{ column: n, op: in, value: ['1.005'] }", false),
    ("{ column: n, op: not_in, value: ['1.005'] }", true),
    ("{ column: n, op: lte, value: '1.005' }", false),
    ("{ column: n, op: gt, value: '1.005' }", true),
    (
        "{ column: n, op: between, value: ['1.005', '1.006'] }",
        false,
    ),
    ("{ column: n, op: eq, value: '123456789012' }", false),
    ("{ column: n, op: neq, value: '123456789012' }", true),
    ("{ column: n, op: eq, value: '1.010' }", true),
    // char(5) holding `abcde`, and `ab` (stored `ab   `)
    ("{ column: c, op: eq, value: 'abcdefg' }", false),
    ("{ column: c, op: neq, value: 'abcdefg' }", true),
    ("{ column: c, op: eq, value: 'abcde  ' }", true),
    ("{ column: cp, op: eq, value: 'ab' }", true),
    ("{ column: cp, op: eq, value: 'ab ' }", true),
    // bit(5) holding 10100
    ("{ column: b, op: eq, value: '101' }", false),
    ("{ column: b, op: eq, value: '1010011' }", false),
    ("{ column: b, op: neq, value: '101' }", true),
    ("{ column: b, op: eq, value: '10100' }", true),
    // varbit(3) holding 101
    ("{ column: vb, op: eq, value: '1010101' }", false),
    ("{ column: vb, op: not_in, value: ['1010101'] }", true),
    ("{ column: vb, op: eq, value: '101' }", true),
    // time(0) holding 12:00:01
    ("{ column: t, op: eq, value: '12:00:00.6' }", false),
    ("{ column: t, op: lte, value: '12:00:00.6' }", false),
    ("{ column: t, op: eq, value: '12:00:01' }", true),
    // timestamp(0), timestamptz(0), interval(0): one second past a whole one
    (
        "{ column: ts, op: eq, value: '2020-01-01 12:00:00.6' }",
        false,
    ),
    (
        "{ column: tz, op: eq, value: '2020-01-01 12:00:00.6+00' }",
        false,
    ),
    ("{ column: iv, op: eq, value: '0.6 seconds' }", false),
    ("{ column: ts, op: eq, value: '2020-01-01 12:00:01' }", true),
    (
        "{ column: tz, op: eq, value: '2020-01-01 12:00:01+00' }",
        true,
    ),
    ("{ column: iv, op: eq, value: '1 second' }", true),
    // interval year holding 1 year
    ("{ column: iy, op: eq, value: '1 year 2 months' }", false),
    ("{ column: iy, op: neq, value: '1 year 2 months' }", true),
    ("{ column: iy, op: eq, value: '1 year' }", true),
    // domains over varchar(5) and numeric(10,2)
    ("{ column: dv, op: eq, value: 'archived' }", false),
    ("{ column: dv, op: neq, value: 'archived' }", true),
    ("{ column: dn, op: eq, value: '1.005' }", false),
    ("{ column: dv, op: eq, value: 'archi' }", true),
    ("{ column: dn, op: eq, value: '1.01' }", true),
    // a domain whose CHECK refuses the value
    ("{ column: dp, op: eq, value: '-1' }", false),
    ("{ column: dp, op: neq, value: '-1' }", true),
    ("{ column: dp, op: eq, value: '5' }", true),
];

/// `(filter, matches)` for the related rows, every one holding `archi` and
/// 1.01.
const PLACED_CASES: &[(&str, bool)] = &[
    ("{ column: s, op: eq, value: 'archived' }", false),
    ("{ column: s, op: neq, value: 'archived' }", true),
    ("{ column: s, op: not_in, value: ['archived'] }", true),
    ("{ column: n, op: eq, value: '1.005' }", false),
    ("{ column: n, op: neq, value: '1.005' }", true),
    ("{ column: n, op: gt, value: '1.005' }", true),
];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_filter_value_that_does_not_fit_its_column_is_compared_as_written() {
    let container = test_postgres::image().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE DOMAIN short_text AS varchar(5)",
        "CREATE DOMAIN money2 AS numeric(10,2)",
        "CREATE DOMAIN positive AS int CHECK (VALUE > 0)",
        "CREATE TABLE sized (id int PRIMARY KEY, s varchar(5), n numeric(10,2), c char(5), \
         cp char(5), b bit(5), vb varbit(3), t time(0), ts timestamp(0), tz timestamptz(0), \
         iv interval(0), iy interval year, dv short_text, dn money2, dp positive)",
        "INSERT INTO sized VALUES (1, 'archi', 1.01, 'abcde', 'ab', B'10100', B'101', \
         '12:00:01', '2020-01-01 12:00:01', '2020-01-01 12:00:01+00', '1 second', '1 year', \
         'archi', 1.01, 5)",
        "CREATE TABLE teams (id int PRIMARY KEY, s varchar(5), n numeric(10,2))",
        "CREATE TABLE owners (id int PRIMARY KEY, team_id int, deleted boolean, \
         s varchar(5), n numeric(10,2))",
        "CREATE TABLE pets (id int PRIMARY KEY, owner_id int, s varchar(5), n numeric(10,2))",
        "CREATE TABLE toys (id int PRIMARY KEY, pet_id int, s varchar(5), n numeric(10,2))",
        "CREATE TABLE tags (id int PRIMARY KEY, s varchar(5), n numeric(10,2))",
        "CREATE TABLE owner_tags (owner_id int, tag_id int, s varchar(5), n numeric(10,2), \
         PRIMARY KEY (owner_id, tag_id))",
        "INSERT INTO teams VALUES (1, 'archi', 1.01)",
        "INSERT INTO owners VALUES (1, 1, true, 'archi', 1.01)",
        "INSERT INTO pets VALUES (1, 1, 'archi', 1.01)",
        "INSERT INTO toys VALUES (1, 1, 'archi', 1.01)",
        "INSERT INTO tags VALUES (1, 'archi', 1.01)",
        "INSERT INTO owner_tags VALUES (1, 1, 'archi', 1.01)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let mut indexes = BTreeMap::new();
    for (n, (filter, _)) in ROOT_CASES.iter().enumerate() {
        indexes.insert(index_name(&format!("root_{n}")), root_filtered(filter));
    }
    for (n, (filter, _)) in PLACED_CASES.iter().enumerate() {
        indexes.insert(
            index_name(&format!("owner_{n}")),
            owner_root_filtered(filter),
        );
        indexes.insert(index_name(&format!("soft_{n}")), owner_soft_deleted(filter));
        indexes.insert(index_name(&format!("rel_{n}")), owner_related(filter));
    }
    let builder = PgDocumentBuilder::connect(&url, Arc::new(SourceSpec::new(indexes)))
        .await
        .unwrap();

    let mut wrong = Vec::new();
    for (n, (filter, matches)) in ROOT_CASES.iter().enumerate() {
        for body in built(&builder, &format!("root_{n}")).await {
            match body {
                Ok(body) if body.is_some() == *matches => {}
                other => wrong.push(format!("root `{filter}`: got {other:?}, matches {matches}")),
            }
        }
    }
    for (n, (filter, matches)) in PLACED_CASES.iter().enumerate() {
        for body in built(&builder, &format!("owner_{n}")).await {
            match body {
                Ok(body) if body.is_some() == *matches => {}
                other => wrong.push(format!("root `{filter}`: got {other:?}, matches {matches}")),
            }
        }
        for body in built(&builder, &format!("soft_{n}")).await {
            match body {
                Ok(body) if body.is_none() == *matches => {}
                other => wrong.push(format!(
                    "`when` `{filter}`: got {other:?}, matches {matches}"
                )),
            }
        }
        for body in built(&builder, &format!("rel_{n}")).await {
            let Ok(Some(GenericValue::Map(fields))) = body else {
                wrong.push(format!("relations `{filter}`: got {body:?}"));
                continue;
            };
            for (field, value) in &fields {
                if field != "id" && rows(value) != usize::from(*matches) {
                    wrong.push(format!(
                        "{field} `{filter}`: got {value:?}, matches {matches}"
                    ));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A role that may not run `has_schema_privilege` casts to the declared type
/// where it can't tell the base type's schema is usable. A domain over a
/// built-in sized type still compares as written: its base type is in
/// `pg_catalog`, which every role may use.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_role_that_may_not_probe_schema_privileges_compares_a_domain_filter_as_written() {
    let container = test_postgres::image().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "REVOKE EXECUTE ON FUNCTION has_schema_privilege(oid, text) FROM PUBLIC",
        "CREATE DOMAIN short_text AS varchar(5)",
        "CREATE DOMAIN money2 AS numeric(10,2)",
        "CREATE TABLE domains (id int PRIMARY KEY, dv short_text, dn money2)",
        "INSERT INTO domains VALUES (1, 'archi', 1.01)",
        "CREATE ROLE reader LOGIN PASSWORD 'reader'",
        "GRANT SELECT ON domains TO reader",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let cases = [
        ("{ column: dv, op: eq, value: 'archived' }", false),
        ("{ column: dv, op: neq, value: 'archived' }", true),
        ("{ column: dn, op: eq, value: '1.005' }", false),
        ("{ column: dn, op: eq, value: '1.01' }", true),
    ];
    let indexes = cases
        .iter()
        .enumerate()
        .map(|(n, (filter, _))| {
            let yaml = format!(
                "
version: 1
table: domains
primary_key: id
filters:
  - {filter}
fields:
  - {{ integer: id, required: true }}
"
            );
            (index_name(&format!("domain_{n}")), schema(&yaml))
        })
        .collect();
    let reader = url.replace("postgres:postgres@", "reader:reader@");
    let builder = PgDocumentBuilder::connect(&reader, Arc::new(SourceSpec::new(indexes)))
        .await
        .unwrap();
    let mut wrong = Vec::new();
    for (n, (filter, matches)) in cases.iter().enumerate() {
        for body in built(&builder, &format!("domain_{n}")).await {
            match body {
                Ok(body) if body.is_some() == *matches => {}
                other => wrong.push(format!("`{filter}`: got {other:?}, matches {matches}")),
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// What a single build and a batched build give document 1 of `index`: its
/// body, `None` for a tombstone, or why it failed.
async fn built(
    builder: &PgDocumentBuilder,
    index: &str,
) -> [Result<Option<GenericValue>, String>; 2] {
    let id = DocumentId {
        index: index_name(index),
        key: RowKey(vec![(column("id"), GenericValue::Int(1))]),
    };
    let single = builder.build(&id).await.map_err(|error| error.to_string());
    let batched = builder
        .build_many(std::slice::from_ref(&id))
        .await
        .map_err(|error| error.to_string())
        .map(|mut documents| documents.pop().unwrap());
    [single.and_then(body), batched.and_then(body)]
}

fn body(document: Document) -> Result<Option<GenericValue>, String> {
    match document {
        Document::Upsert { body, .. } => Ok(Some(body)),
        Document::Delete { .. } => Ok(None),
        Document::Failed { failure, .. } => Err(format!("{failure:?}")),
    }
}

/// How many related rows a field holds: an array's length, a count, or
/// whether a to-one object is there.
fn rows(value: &GenericValue) -> usize {
    match value {
        GenericValue::Array(items) => items.len(),
        GenericValue::BigInt(count) => usize::try_from(*count).unwrap(),
        GenericValue::Int(count) => usize::try_from(*count).unwrap(),
        GenericValue::Null => 0,
        GenericValue::Map(_) => 1,
        other => panic!("unexpected value {other:?}"),
    }
}

fn root_filtered(filter: &str) -> IndexSchema {
    schema(&format!(
        "
version: 1
table: sized
primary_key: id
filters:
  - {filter}
fields:
  - {{ integer: id, required: true }}
"
    ))
}

fn owner_root_filtered(filter: &str) -> IndexSchema {
    schema(&format!(
        "
version: 1
table: owners
primary_key: id
filters:
  - {filter}
fields:
  - {{ integer: id, required: true }}
"
    ))
}

fn owner_soft_deleted(filter: &str) -> IndexSchema {
    schema(&format!(
        "
version: 1
table: owners
primary_key: id
soft_delete:
  column: deleted
  when:
    - {filter}
fields:
  - {{ integer: id, required: true }}
"
    ))
}

/// Every place a relation takes a filter, each holding one related row
/// that `filter` keeps or drops.
fn owner_related(filter: &str) -> IndexSchema {
    schema(&format!(
        "
version: 1
table: owners
primary_key: id
fields:
  - {{ integer: id, required: true }}
  - has_many: pets
    table: pets
    foreign_key: owner_id
    primary_key: id
    filters: [{filter}]
    fields:
      - {{ integer: id, required: true }}
  - belongs_to: team
    table: teams
    column: team_id
    primary_key: id
    filters: [{filter}]
    fields:
      - {{ integer: id, required: true }}
  - many_to_many: tags
    table: tags
    primary_key: id
    filters: [{filter}]
    through: {{ table: owner_tags, left_key: owner_id, right_key: tag_id }}
    fields:
      - {{ integer: id, required: true }}
  - many_to_many: linkedTags
    table: tags
    primary_key: id
    through:
      table: owner_tags
      left_key: owner_id
      right_key: tag_id
      filters: [{filter}]
    fields:
      - {{ integer: id, required: true }}
  - count: petCount
    table: pets
    foreign_key: owner_id
    filters: [{filter}]
  - count: linkedTagCount
    table: tags
    through:
      table: owner_tags
      left_key: owner_id
      right_key: tag_id
      filters: [{filter}]
  - count: toyCount
    table: toys
    foreign_key: pet_id
    via:
      - table: pets
        foreign_key: owner_id
        filters: [{filter}]
"
    ))
}

fn schema(yaml: &str) -> IndexSchema {
    IndexSchema::try_from(SchemaYaml::try_parse(yaml).unwrap())
        .unwrap_or_else(|error| panic!("{error}\n{yaml}"))
}

fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}

fn index_name(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}
