//! The review modes of `just review-ab`: the document digest and the plan
//! check, over the `cases` bench's roots (its fixtures, included as a module).
//!
//! Not a benchmark: `review-ab.sh --digest` / `--plans` (`just review-digest`,
//! `just review-plans`) build it and run it once per side with
//! `REVIEW_AB_MODE` set. Each mode writes a TSV to `REVIEW_AB_OUT` that a
//! script compares across base and head:
//!
//! | Mode | Writes, per line |
//! | --- | --- |
//! | `digest` | `<index> <form> <id> <op> <body>`: every document `build`, `build_many` (under 16 keys and 16+) and a backfill built, the body as `sink::to_json` with sorted keys; `#index <index>` declares each index it built for, `#note <index> <text>` says what it left out |
//! | `plans` | `<case> <root relations> <partitions> <statements> <calls> <plans> <calls of the second half> <plans of the second half>`, from `pg_stat_statements` |
//!
//! `REVIEW_AB_FILTER`, a regex, picks the indexes (`digest`) or the cases
//! (`plans`). `plans` opens each case with `SELECT 'flusso-review-case
//! <REVIEW_AB_RUN> <case>'` and closes it with the case `-`, so the case's
//! plans can be found in the server log.
//!
//! It is a target of its own so that `cases.rs` needs nothing a base older
//! than these modes lacks: `just review-ab` lays the head's `cases.rs` over
//! any base, this file only over the base of a review mode. What this file
//! needs from the crate beyond what `cases.rs` does is listed in the
//! `review-ab requires:` lines below; `review-ab.sh` looks each one up in the
//! base's source and refuses, naming it, a base that lacks one.
//!
//! ```text
//! REVIEW_AB_MODE=digest REVIEW_AB_PG_URL=… REVIEW_AB_OUT=out.tsv \
//!   cargo bench -p flusso-source-postgres --bench review
//! ```

// review-ab requires: fn connect_with_max_connections
// review-ab requires: fn backfill_indexes

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stderr,
    unused_crate_dependencies,
    missing_debug_implementations
)]

#[path = "cases.rs"]
#[allow(dead_code, unreachable_pub, unused_attributes)]
mod cases;

fn main() {
    match std::env::var("REVIEW_AB_MODE") {
        Ok(mode) if !mode.is_empty() => run(&mode),
        _ => eprintln!(
            "the `review` bench runs only with REVIEW_AB_MODE=digest|plans (`just review-digest`, `just review-plans`)"
        ),
    }
}

use std::fmt::Write as _;
use std::sync::Arc;

use kernel::GenericValue;
use regex::Regex;
use source::RowKey;
use source::document::{Document, DocumentBuilder, DocumentId};
use source_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool, Row};

use cases::{KeyType, Root, Shape, column, roots, runtime, seed, spec, wal_keys};

/// The batch sizes a plan case runs: under 16 keys (an array
/// constructor) and over it (array parameters).
const PLAN_BATCHES: &[usize] = &[1, 10, 100];
/// Calls per half of a plan case.
const PLAN_CALLS: usize = 10;
/// The backfill's build batch: the ingest engine's default `max_changes`.
const BACKFILL_BATCH: usize = 256;
/// The backfill's key page: the ingest engine's `KEY_PAGE` (a crate this one
/// can't depend on).
const KEY_PAGE: usize = 1024;

struct Settings {
    url: String,
    out: String,
    filter: Regex,
    run: String,
}

fn run(mode: &str) {
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    let settings = Settings {
        url: var("REVIEW_AB_PG_URL"),
        out: var("REVIEW_AB_OUT"),
        filter: Regex::new(&var("REVIEW_AB_FILTER")).expect("REVIEW_AB_FILTER is not a regex"),
        run: var("REVIEW_AB_RUN"),
    };
    assert!(
        !settings.url.is_empty() && !settings.out.is_empty(),
        "a review mode needs REVIEW_AB_PG_URL and REVIEW_AB_OUT"
    );
    let lines = runtime().block_on(async {
        match mode {
            "digest" => digest(&settings).await,
            "plans" => plans(&settings).await,
            other => panic!("unknown REVIEW_AB_MODE `{other}`: digest or plans"),
        }
    });
    std::fs::write(&settings.out, lines).unwrap();
}

fn case_name(root: &Root) -> String {
    root.case.map_or_else(
        || "plain".to_owned(),
        |(group, name)| format!("{group}/{name}"),
    )
}

/// A key no row of `root` has, typed as the root's key.
fn missing(root: &Root, n: usize) -> GenericValue {
    let text = match root.key_type {
        KeyType::Int => format!("{}", 1_000_000 + n),
        KeyType::Uuid => format!("00000000-0000-0000-0000-{n:012}"),
        KeyType::Numeric => format!("-{n}.25"),
        KeyType::Float8 => format!("-{n}.5"),
        KeyType::Text => format!("missing-{n}"),
    };
    root.key_type.value(&text)
}

async fn ids(builder: &PgDocumentBuilder, pool: &PgPool, root: &Root) -> Vec<DocumentId> {
    let keys = wal_keys(pool, root).await;
    cases::resolve(builder, root.table, &keys)
        .await
        .unwrap_or_else(|| panic!("resolving `{}` failed", root.table))
}

/// Whether a missing id can be written for `root`: only when a change's key
/// is the root's `id` alone. The harness makes up no value for the other
/// key columns of a composite-key or keyless root, so their tombstone path
/// is not compared.
fn keys_missing_ids(root: &Root) -> bool {
    root.wal_key() == ["id"]
}

fn with_missing(root: &Root, present: &[DocumentId], count: usize) -> Vec<DocumentId> {
    let index = kernel::IndexName::try_new(root.table).unwrap();
    let mut ids = present.to_vec();
    if !keys_missing_ids(root) {
        return ids;
    }
    ids.extend((1..=count).map(|n| DocumentId {
        index: index.clone(),
        key: RowKey(vec![(column("id"), missing(root, n))]),
    }));
    ids
}

async fn digest(settings: &Settings) -> String {
    let pool = PgPoolOptions::new().connect(&settings.url).await.unwrap();
    let roots = roots();
    seed(&pool, &roots).await;
    let builder = PgDocumentBuilder::connect(&settings.url, Arc::new(spec(&roots)))
        .await
        .unwrap();
    let mut out = String::new();
    for root in roots
        .iter()
        .filter(|root| settings.filter.is_match(root.table))
    {
        writeln!(out, "#index\t{}", root.table).unwrap();
        if !keys_missing_ids(root) {
            writeln!(
                out,
                "#note\t{}\tno missing ids built, so tombstones are not compared here: a change's key is {}, and the harness makes up `id` alone",
                root.table,
                root.wal_key().join(", ")
            )
            .unwrap();
        }
        let present = ids(&builder, &pool, root).await;
        let mut forms: Vec<(&str, Vec<Document>)> = Vec::new();
        let mut single = Vec::new();
        for id in with_missing(root, present.get(..20).unwrap(), 2) {
            single.push(builder.build(&id).await.unwrap());
        }
        forms.push(("build", single));
        let small = with_missing(root, present.get(..10).unwrap(), 1);
        forms.push(("build_many<16", builder.build_many(&small).await.unwrap()));
        let large = with_missing(root, &present, 5);
        forms.push(("build_many>=16", builder.build_many(&large).await.unwrap()));
        let backfilled = backfill(&builder, root).await;
        if backfilled.is_empty() {
            writeln!(
                out,
                "#note\t{}\tthe backfill built nothing: it skips a root without a single-column primary key",
                root.table
            )
            .unwrap();
        }
        forms.push(("backfill", backfilled));
        for (form, documents) in forms {
            for document in documents {
                line(&mut out, root.table, form, &document);
            }
        }
    }
    out
}

/// The root's documents as a backfill builds them: each key a page reads its
/// own document, built in batches as the ingest engine does.
async fn backfill(builder: &PgDocumentBuilder, root: &Root) -> Vec<Document> {
    let index = kernel::IndexName::try_new(root.table).unwrap();
    let mut keys: Vec<RowKey> = Vec::new();
    loop {
        let page = builder.keys(&index, keys.last(), KEY_PAGE).await.unwrap();
        let last = page.len() < KEY_PAGE;
        keys.extend(page);
        if last {
            break;
        }
    }
    let ids: Vec<DocumentId> = keys
        .into_iter()
        .map(|key| DocumentId {
            index: index.clone(),
            key,
        })
        .collect();
    let mut documents = Vec::with_capacity(ids.len());
    for batch in ids.chunks(BACKFILL_BATCH) {
        documents.extend(builder.build_many(batch).await.unwrap());
    }
    documents
}

fn line(out: &mut String, index: &str, form: &str, document: &Document) {
    let (id, op, body) = match document {
        Document::Upsert { id, body } => {
            let mut text = String::new();
            canonical(&sink::to_json(body), &mut text);
            (id, "upsert", text)
        }
        Document::Delete { id } => (id, "delete", String::new()),
        Document::Failed { id, failure } => (id, "failed", format!("{failure:?}")),
    };
    let id = id.rendered().replace(['\t', '\n'], " ");
    let body = body.replace(['\t', '\n'], " ");
    writeln!(out, "{index}\t{form}\t{id}\t{op}\t{body}").unwrap();
}

/// `value` as JSON with every object's keys sorted, whichever order
/// `serde_json` keeps them in.
fn canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (n, (key, value)) in entries.into_iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String(key.clone()).to_string());
                out.push(':');
                canonical(value, out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (n, item) in items.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// The relations a root's rows live in: the table, or its partitions.
fn root_relations(root: &Root) -> Vec<String> {
    match root.shape {
        Shape::HashPartitioned | Shape::RangePartitioned => {
            (0..4).map(|n| format!("{}_{n}", root.table)).collect()
        }
        _ => vec![root.table.to_owned()],
    }
}

/// Statements, calls and plans of every statement naming `table` in this
/// database, summed: Postgres 18 merges `ARRAY[…]` lists of different
/// lengths into one entry, so one entry is not one statement text.
async fn statements(pool: &PgPool, table: &str) -> (i64, i64, i64) {
    let row = sqlx::query(
        "SELECT count(*)::bigint, coalesce(sum(calls), 0)::bigint, \
         coalesce(sum(plans), 0)::bigint FROM pg_stat_statements \
         WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
         AND query LIKE '%' || $1 || '%' AND query NOT LIKE '%pg_stat_statements%' \
         AND query NOT LIKE '%flusso-review-case%'",
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .unwrap();
    (row.get(0), row.get(1), row.get(2))
}

async fn plans(settings: &Settings) -> String {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&settings.url)
        .await
        .unwrap();
    let roots = roots();
    seed(&pool, &roots).await;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(&pool)
        .await
        .unwrap();
    let spec = Arc::new(spec(&roots));
    let lookup = PgDocumentBuilder::connect(&settings.url, spec.clone())
        .await
        .unwrap();
    let mut out = String::new();
    for root in &roots {
        let all = ids(&lookup, &pool, root).await;
        for &size in PLAN_BATCHES {
            let case = format!("{}/{size}", case_name(root));
            if !settings.filter.is_match(&case) {
                continue;
            }
            let batch = all.get(..size).unwrap();
            // One connection: every call reuses the same prepared
            // statements, as a long-running daemon's pool does.
            let builder =
                PgDocumentBuilder::connect_with_max_connections(&settings.url, spec.clone(), 1)
                    .await
                    .unwrap();
            let marker = format!("SELECT 'flusso-review-case {} {case}'", settings.run);
            sqlx::query(AssertSqlSafe(marker))
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("SELECT pg_stat_statements_reset()")
                .execute(&pool)
                .await
                .unwrap();
            for _ in 0..PLAN_CALLS {
                builder.build_many(batch).await.unwrap();
            }
            let first = statements(&pool, root.table).await;
            for _ in 0..PLAN_CALLS {
                builder.build_many(batch).await.unwrap();
            }
            let (count, calls, plans) = statements(&pool, root.table).await;
            // Closes the case: what the server logs next (the next
            // run's seeding) belongs to no case.
            let closing = format!("SELECT 'flusso-review-case {} -'", settings.run);
            sqlx::query(AssertSqlSafe(closing))
                .execute(&pool)
                .await
                .unwrap();
            writeln!(
                out,
                "{case}\t{}\t{}\t{count}\t{calls}\t{plans}\t{}\t{}",
                root_relations(root).join(","),
                match root.shape {
                    Shape::HashPartitioned | Shape::RangePartitioned => 4,
                    _ => 0,
                },
                calls - first.1,
                plans - first.2,
            )
            .unwrap();
        }
    }
    out
}
