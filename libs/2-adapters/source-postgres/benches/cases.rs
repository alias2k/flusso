//! Review cases for the batched document build, against a live Postgres.
//!
//! `build_many` over the ids a reviewer of a source change keeps asking about:
//! how many, how many exist, which key type, which root shape. Each case's
//! ids come from keys read back the way the WAL decoder reads them (the key's
//! text, typed by its column type) and then resolved, so a key whose type the
//! build does not match shows up as tombstones instead of being measured as a
//! hit.
//!
//! | Group | Cases |
//! | --- | --- |
//! | `batch` | 1, 10, 100, 1000, 10000 ids of a plain `int` root |
//! | `hit_ratio` | 0, 50, 100 % of 1000 ids exist |
//! | `key` | `int`, `uuid`, `varchar`, `numeric`, `float8`, `char(n)`, `citext` |
//! | `root` | hash- and range-partitioned on the key, composite key, keyless, wide unread column, wide with a composite key, never analysed (see [`Shape`]), and `plain_baseline`, a plain 1,000-row root to read them against |
//!
//! Every root has a `has_many` child keyed by the root's key type, two rows
//! per root. Before measuring, each case is built once and checked: a case
//! whose build returns another number of documents than its rows is skipped
//! with a message, so an A/B comparison lists it on one side only.
//!
//! Not part of the PR gate nor the `main` history: `just bench-components` and
//! `just review-ab` run it. Requires Docker; `just review-ab` instead hands it a
//! fresh database through `REVIEW_AB_PG_URL`, a variable only that command sets
//! (the bench creates `citext` in it). Run with:
//!
//! ```text
//! cargo bench -p flusso-source-postgres --bench cases
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stderr,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, GenericValue,
    IndexName, IndexSchema, Join, JoinKind, Relation, TableName,
};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowKey, SourceSpec};
use source_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Row};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::runtime::Runtime;

/// Rows in the `plain` root, the largest batch.
const PLAIN_ROWS: i32 = 10_000;
/// Rows in every other root.
const ROWS: i32 = 1_000;
const BATCHES: &[usize] = &[1, 10, 100, 1_000, 10_000];
const HIT_PERCENTS: &[usize] = &[0, 50, 100];

/// How a root key's text becomes a value, as `text_value::typed_value` does it
/// for the WAL.
#[derive(Clone, Copy)]
enum KeyType {
    Int,
    Uuid,
    Numeric,
    Float8,
    Text,
}

impl KeyType {
    fn value(self, text: &str) -> GenericValue {
        match self {
            Self::Int => GenericValue::Int(text.parse().unwrap()),
            Self::Uuid => GenericValue::Uuid(uuid::Uuid::parse_str(text).unwrap()),
            Self::Numeric => {
                GenericValue::Decimal(rust_decimal::Decimal::from_str_exact(text).unwrap())
            }
            Self::Float8 => GenericValue::Double(text.parse().unwrap()),
            Self::Text => GenericValue::String(text.to_owned()),
        }
    }
}

/// How a root table is laid out.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Plain,
    HashPartitioned,
    RangePartitioned,
    /// `PRIMARY KEY (tenant, id)`. The index declares `primary_key: id`, which
    /// is not the table's key: `run` warns (`RootPrimaryKey`) and starts. That
    /// warned-but-running shape is what is under test.
    Composite,
    /// No primary key, `REPLICA IDENTITY FULL`, so a change's key is every
    /// column. The index declares `primary_key: id`: `run` warns and starts,
    /// and that is the shape under test.
    Keyless,
    /// A ~19 KiB incompressible `payload` per row, TOASTed, that the document
    /// never reads.
    Wide,
    /// [`Shape::Wide`] with [`Shape::Composite`]'s key.
    WideComposite,
    /// Autovacuum off on the root and its child and never vacuumed or
    /// analysed: the planner has no statistics for either.
    NeverAnalysed,
}

/// One root table and the case it stands for.
struct Root {
    table: &'static str,
    /// `key/<case>` or `root/<case>`; `None` for the `plain` root the `batch`
    /// and `hit_ratio` groups use.
    case: Option<(&'static str, &'static str)>,
    sql_type: &'static str,
    key_type: KeyType,
    /// The `id` of row `g` of `generate_series(1, $1) g`.
    key_expr: &'static str,
    shape: Shape,
    rows: i32,
}

const PAYLOAD: &str =
    "(SELECT string_agg(md5(g::text || i::text), '') FROM generate_series(1, 600) i)";

impl Root {
    const fn new(table: &'static str, case: (&'static str, &'static str), shape: Shape) -> Self {
        Self {
            table,
            case: Some(case),
            sql_type: "int",
            key_type: KeyType::Int,
            key_expr: "g",
            shape,
            rows: ROWS,
        }
    }

    const fn keyed(
        table: &'static str,
        case: &'static str,
        sql_type: &'static str,
        key_type: KeyType,
        key_expr: &'static str,
    ) -> Self {
        Self {
            sql_type,
            key_type,
            key_expr,
            ..Self::new(table, ("key", case), Shape::Plain)
        }
    }

    fn composite(&self) -> bool {
        matches!(self.shape, Shape::Composite | Shape::WideComposite)
    }

    fn wide(&self) -> bool {
        matches!(self.shape, Shape::Wide | Shape::WideComposite)
    }

    /// The columns of the key a change carries: the primary key, or every
    /// column of a table without one.
    fn wal_key(&self) -> &'static [&'static str] {
        match self.shape {
            Shape::Composite | Shape::WideComposite => &["tenant", "id"],
            Shape::Keyless => &["id", "name"],
            _ => &["id"],
        }
    }

    /// `WITH (…)` storage options for the root and its child.
    fn options(&self) -> &'static str {
        if self.shape == Shape::NeverAnalysed {
            " WITH (autovacuum_enabled = false)"
        } else {
            ""
        }
    }

    fn ddl(&self) -> Vec<String> {
        let (t, ty) = (self.table, self.sql_type);
        let tenant = if self.composite() { "tenant int, " } else { "" };
        let payload = if self.wide() { ", payload text" } else { "" };
        let key = match self.shape {
            Shape::Keyless => String::new(),
            _ if self.composite() => ", PRIMARY KEY (tenant, id)".to_owned(),
            _ => ", PRIMARY KEY (id)".to_owned(),
        };
        let partition = match self.shape {
            Shape::HashPartitioned => " PARTITION BY HASH (id)",
            Shape::RangePartitioned => " PARTITION BY RANGE (id)",
            _ => "",
        };
        let mut ddl = vec![format!(
            "CREATE TABLE {t} ({tenant}id {ty} NOT NULL, name text NOT NULL{payload}{key})\
             {partition}{}",
            self.options()
        )];
        for n in 0..4 {
            match self.shape {
                Shape::HashPartitioned => ddl.push(format!(
                    "CREATE TABLE {t}_{n} PARTITION OF {t} \
                     FOR VALUES WITH (MODULUS 4, REMAINDER {n})"
                )),
                Shape::RangePartitioned => {
                    let bound = |n: i32| match n {
                        0 => "MINVALUE".to_owned(),
                        4 => "MAXVALUE".to_owned(),
                        n => (n * ROWS / 4 + 1).to_string(),
                    };
                    ddl.push(format!(
                        "CREATE TABLE {t}_{n} PARTITION OF {t} FOR VALUES FROM ({}) TO ({})",
                        bound(n),
                        bound(n + 1)
                    ));
                }
                _ => {}
            }
        }
        if self.shape == Shape::Keyless {
            ddl.push(format!("ALTER TABLE {t} REPLICA IDENTITY FULL"));
        }
        ddl
    }

    fn insert(&self) -> String {
        let (tenant, tenant_value) = if self.composite() {
            ("tenant, ", "g % 7, ")
        } else {
            ("", "")
        };
        let (payload, payload_value) = if self.wide() {
            (", payload", format!(", {PAYLOAD}"))
        } else {
            ("", String::new())
        };
        format!(
            "INSERT INTO {} ({tenant}id, name{payload}) \
             SELECT {tenant_value}{}, 'name ' || g{payload_value} FROM generate_series(1, $1) g",
            self.table, self.key_expr
        )
    }
}

fn roots() -> Vec<Root> {
    use KeyType::{Float8, Int, Numeric, Text, Uuid};
    vec![
        Root {
            case: None,
            rows: PLAIN_ROWS,
            ..Root::new("plain", ("", ""), Shape::Plain)
        },
        Root::keyed("k_int", "int", "int", Int, "g"),
        Root::keyed("k_uuid", "uuid", "uuid", Uuid, "md5(g::text)::uuid"),
        Root::keyed("k_varchar", "varchar", "varchar(40)", Text, "'key-' || g"),
        Root::keyed("k_numeric", "numeric", "numeric(12,2)", Numeric, "g + 0.25"),
        Root::keyed("k_float8", "float8", "float8", Float8, "g + 0.5"),
        Root::keyed("k_char", "char", "char(12)", Text, "lpad(g::text, 12, 'k')"),
        Root::keyed("k_citext", "citext", "citext", Text, "'Key-' || g"),
        // The baseline for the other root shapes: the same 1,000 rows and
        // child, where `batch/1000` reads 1,000 of the 10,000-row `plain` table.
        Root::new("s_plain", ("root", "plain_baseline"), Shape::Plain),
        Root::new(
            "s_hash",
            ("root", "hash_partitioned"),
            Shape::HashPartitioned,
        ),
        Root::new(
            "s_range",
            ("root", "range_partitioned"),
            Shape::RangePartitioned,
        ),
        Root::new("s_composite", ("root", "composite_key"), Shape::Composite),
        Root::new("s_keyless", ("root", "keyless"), Shape::Keyless),
        Root::new("s_wide", ("root", "wide_unread"), Shape::Wide),
        Root::new(
            "s_wide_composite",
            ("root", "wide_composite_key"),
            Shape::WideComposite,
        ),
        Root::new(
            "s_unanalysed",
            ("root", "never_analysed"),
            Shape::NeverAnalysed,
        ),
    ]
}

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
}

async fn connect() -> (Option<ContainerAsync<Postgres>>, String) {
    if let Some(url) = std::env::var("REVIEW_AB_PG_URL")
        .ok()
        .filter(|url| !url.is_empty())
    {
        return (None, url);
    }
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    (
        Some(container),
        format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres"),
    )
}

async fn seed(pool: &sqlx::PgPool, roots: &[Root]) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS citext")
        .execute(pool)
        .await
        .unwrap();
    for root in roots {
        for ddl in root.ddl() {
            sqlx::query(AssertSqlSafe(ddl)).execute(pool).await.unwrap();
        }
        sqlx::query(AssertSqlSafe(root.insert()))
            .bind(root.rows)
            .execute(pool)
            .await
            .unwrap();
        let (table, items, options) = (root.table, format!("{}_items", root.table), root.options());
        for sql in [
            format!(
                "CREATE TABLE {items} (id serial PRIMARY KEY, parent {} NOT NULL, label text)\
                 {options}",
                root.sql_type
            ),
            format!("CREATE INDEX ON {items} (parent)"),
            format!(
                "INSERT INTO {items} (parent, label) \
                 SELECT id, 'item ' || n FROM {table}, generate_series(1, 2) n"
            ),
        ] {
            sqlx::query(AssertSqlSafe(sql)).execute(pool).await.unwrap();
        }
        if root.shape != Shape::NeverAnalysed {
            for table in [table, &items] {
                sqlx::query(AssertSqlSafe(format!("VACUUM ANALYZE {table}")))
                    .execute(pool)
                    .await
                    .unwrap();
            }
        }
    }
}

/// Every root's change keys, in key order, as the WAL decoder would read them.
async fn wal_keys(pool: &sqlx::PgPool, root: &Root) -> Vec<RowKey> {
    let columns = root
        .wal_key()
        .iter()
        .map(|c| format!("{c}::text"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("SELECT {columns} FROM {} ORDER BY id", root.table);
    let rows = sqlx::query(AssertSqlSafe(sql))
        .fetch_all(pool)
        .await
        .unwrap();
    rows.iter()
        .map(|row| {
            RowKey(
                root.wal_key()
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        let text: String = row.get(i);
                        let ty = match *name {
                            "id" => root.key_type,
                            "name" => KeyType::Text,
                            _ => KeyType::Int,
                        };
                        (column(name), ty.value(&text))
                    })
                    .collect(),
            )
        })
        .collect()
}

/// A key no `plain` row has, for the misses.
fn missing_key(n: usize) -> RowKey {
    RowKey(vec![(
        column("id"),
        GenericValue::Int(1_000_000 + i32::try_from(n).unwrap()),
    )])
}

async fn resolve(
    builder: &PgDocumentBuilder,
    table: &str,
    keys: &[RowKey],
) -> Option<Vec<DocumentId>> {
    let mut ids = Vec::with_capacity(keys.len());
    for key in keys {
        match builder.resolve(&self::table(table), key, None, None).await {
            Ok(resolved) => ids.extend(resolved),
            Err(error) => {
                eprintln!("skipping `{table}`: resolving a root key failed: {error}");
                return None;
            }
        }
    }
    Some(ids)
}

struct Case {
    group: &'static str,
    name: String,
    ids: Vec<DocumentId>,
}

/// Whether the case builds exactly `hits` documents; a case that does not
/// would measure something else.
async fn builds(builder: &PgDocumentBuilder, case: &Case, hits: usize) -> bool {
    match builder.build_many(&case.ids).await {
        Ok(documents) => {
            let upserts = documents
                .iter()
                .filter(|d| matches!(d, Document::Upsert { .. }))
                .count();
            if documents.len() == case.ids.len() && upserts == hits {
                return true;
            }
            eprintln!(
                "skipping `{}/{}`: {} ids built {} documents, {upserts} upserts, expected {hits}",
                case.group,
                case.name,
                case.ids.len(),
                documents.len()
            );
        }
        Err(error) => eprintln!(
            "skipping `{}/{}`: build_many failed: {error}",
            case.group, case.name
        ),
    }
    false
}

async fn setup() -> (
    Option<ContainerAsync<Postgres>>,
    PgDocumentBuilder,
    Vec<Case>,
) {
    let (container, url) = connect().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    let roots = roots();
    seed(&pool, &roots).await;
    let builder = PgDocumentBuilder::connect(&url, Arc::new(spec(&roots)))
        .await
        .unwrap();

    let mut keys = BTreeMap::new();
    for root in &roots {
        keys.insert(root.table, wal_keys(&pool, root).await);
    }
    let mut planned: Vec<(Case, usize)> = Vec::new();
    let plain = keys.get("plain").unwrap();
    for &n in BATCHES {
        if let Some(ids) = resolve(&builder, "plain", plain.get(..n).unwrap()).await {
            planned.push((
                Case {
                    group: "batch",
                    name: n.to_string(),
                    ids,
                },
                n,
            ));
        }
    }
    for &percent in HIT_PERCENTS {
        let hits = 1_000 * percent / 100;
        let mut mixed: Vec<RowKey> = plain.get(..hits).unwrap().to_vec();
        mixed.extend((hits..1_000).map(missing_key));
        if let Some(ids) = resolve(&builder, "plain", &mixed).await {
            planned.push((
                Case {
                    group: "hit_ratio",
                    name: percent.to_string(),
                    ids,
                },
                hits,
            ));
        }
    }
    for root in &roots {
        let Some((group, name)) = root.case else {
            continue;
        };
        let wanted = keys.get(root.table).unwrap().get(..ROWS as usize).unwrap();
        if let Some(ids) = resolve(&builder, root.table, wanted).await {
            planned.push((
                Case {
                    group,
                    name: name.to_owned(),
                    ids,
                },
                wanted.len(),
            ));
        }
    }
    let mut cases = Vec::with_capacity(planned.len());
    for (case, hits) in planned {
        if builds(&builder, &case, hits).await {
            cases.push(case);
        }
    }
    (container, builder, cases)
}

fn bench(c: &mut Criterion) {
    let rt = runtime();
    // Keep the runtime entered so the container's async teardown has one.
    let _guard = rt.enter();
    let (_container, builder, cases) = rt.block_on(setup());

    for group_name in ["batch", "hit_ratio", "key", "root"] {
        let mut group = c.benchmark_group(group_name);
        group.sample_size(20);
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(4));
        for case in cases.iter().filter(|case| case.group == group_name) {
            group.throughput(Throughput::Elements(case.ids.len() as u64));
            group.bench_with_input(
                BenchmarkId::from_parameter(&case.name),
                &case.ids,
                |b, ids| {
                    b.to_async(&rt)
                        .iter(|| async { builder.build_many(ids).await.unwrap() });
                },
            );
        }
        group.finish();
    }
}

/// One index per root: `id`, `name`, and the `items` it has many of.
fn spec(roots: &[Root]) -> SourceSpec {
    let indexes = roots
        .iter()
        .map(|root| {
            let items = Field {
                field: field("items"),
                options: Default::default(),
                emit: true,
                source: FieldSource::Relation(Relation::Join(Join {
                    table: table(&format!("{}_items", root.table)),
                    primary_key: column("id"),
                    nullable: false,
                    kind: JoinKind::HasMany {
                        foreign_key: column("parent"),
                    },
                    filters: None,
                    order_by: None,
                    limit: None,
                    fields: vec![column_field("id"), column_field("label")],
                    indexed: true,
                    on: vec![],
                })),
            };
            let schema = IndexSchema {
                version: 1,
                table: table(root.table),
                db_schema: DatabaseSchema::try_new("public").unwrap(),
                primary_key: Some(column("id")),
                doc_id: None,
                soft_delete: None,
                filters: None,
                fields: vec![column_field("id"), column_field("name"), items],
            };
            (IndexName::try_new(root.table).unwrap(), schema)
        })
        .collect();
    SourceSpec::new(indexes)
}

fn column_field(name: &str) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: column(name),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
    }
}

fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}
fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}
fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}

criterion_group!(benches, bench);
criterion_main!(benches);
