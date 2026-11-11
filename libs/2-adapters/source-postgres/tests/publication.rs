//! End-to-end tests for publication management (the [`CaptureProvisioning`]
//! impl) against a real Postgres in a container. These exercise the coverage
//! inspection, the privilege verdict, and the actual `CREATE`/`ALTER PUBLICATION`
//! provisioning that unit tests can only check by generated-string assertion —
//! plus the replica-identity half: the report (`inspect_pre_image`) across
//! `DEFAULT`, `FULL`, `USING INDEX`, a composite-key junction, and a
//! partitioned table across its partitions, and
//! `ensure_pre_image` setting `FULL` when privileged and refusing when not —
//! on every table an index reads, the root and a `belongs_to` target included.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p sources-postgres --test publication -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::{BTreeMap, BTreeSet};

use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, IndexName,
    IndexSchema, Join, JoinKind, Relation, TableName,
};
use source::{CaptureProvisioning, PreImageColumns, QualifiedTable, SourceSpec};
use source_postgres::{ReplicationConfig, WalChangeCapture};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ContainerAsync;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const PUBLICATION: &str = "flusso";

/// Postgres 16: flusso's supported range starts at 14, and the publication
/// option for partitioned tables needs 13.
async fn postgres() -> ContainerAsync<Postgres> {
    Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .unwrap()
}

/// Whether the publication streams partitioned tables under their own name.
async fn via_root(pool: &PgPool) -> bool {
    sqlx::query_scalar("SELECT pubviaroot FROM pg_publication WHERE pubname = $1")
        .bind(PUBLICATION)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A capture pointed at `db` as `user`/`password`, with publication management
/// left to the per-call `manage` argument. The slot name is irrelevant —
/// provisioning never touches the slot.
fn capture(port: u16, user: &str, password: &str, db: &str) -> WalChangeCapture {
    let config =
        ReplicationConfig::new("127.0.0.1", user, password, db, "flusso-test", PUBLICATION)
            .with_port(port);
    let url = format!("postgres://{user}:{password}@127.0.0.1:{port}/{db}");
    WalChangeCapture::new(config, url)
}

fn required(tables: &[&str]) -> BTreeSet<QualifiedTable> {
    tables
        .iter()
        .map(|t| {
            QualifiedTable::new(
                DatabaseSchema::try_new("public").unwrap(),
                TableName::try_new(*t).unwrap(),
            )
        })
        .collect()
}

/// Tables the publication currently streams, as `schema.table` strings.
async fn published_tables(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT schemaname || '.' || tablename FROM pg_publication_tables \
         WHERE pubname = $1 ORDER BY 1",
    )
    .bind(PUBLICATION)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn privileged_role_creates_extends_and_respects_opt_out() {
    let container = postgres().await;
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY)",
        "CREATE TABLE orders (id int PRIMARY KEY)",
        "CREATE TABLE items (id int PRIMARY KEY)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let cap = capture(port, "postgres", "postgres", "postgres");
    let two = required(&["users", "orders"]);

    // No publication yet: a gap, manageable (superuser), with CREATE remediation.
    let report = cap.inspect_coverage(&two).await.unwrap();
    assert!(!report.satisfied);
    assert_eq!(report.missing.len(), 2);
    assert!(report.manageable);
    assert!(report.remediation[0].contains("CREATE PUBLICATION"));

    // Opt-out (manage = false): inspect-only, nothing is created.
    cap.ensure_coverage(&two, false).await.unwrap();
    assert!(published_tables(&pool).await.is_empty());

    // manage = true: the publication is created covering both tables.
    cap.ensure_coverage(&two, true).await.unwrap();
    assert_eq!(
        published_tables(&pool).await,
        ["public.orders", "public.users"]
    );
    assert!(cap.inspect_coverage(&two).await.unwrap().satisfied);

    // A newly-referenced table is a partial gap → ALTER ADD, not re-CREATE.
    let three = required(&["users", "orders", "items"]);
    let report = cap.inspect_coverage(&three).await.unwrap();
    assert_eq!(
        report.missing,
        required(&["items"]).into_iter().collect::<Vec<_>>()
    );
    assert!(report.remediation[0].contains("ALTER PUBLICATION"));

    cap.ensure_coverage(&three, true).await.unwrap();
    assert_eq!(
        published_tables(&pool).await,
        ["public.items", "public.orders", "public.users"]
    );

    // Idempotent: ensuring an already-covered set is a no-op.
    cap.ensure_coverage(&three, true).await.unwrap();
    assert!(cap.inspect_coverage(&three).await.unwrap().satisfied);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn read_only_role_reports_gap_without_creating() {
    let container = postgres().await;
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY)",
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int NOT NULL)",
        // A least-privilege streaming role: can read, but owns nothing and
        // cannot create publications.
        "CREATE ROLE reader LOGIN PASSWORD 'reader'",
        "GRANT SELECT ON ALL TABLES IN SCHEMA public TO reader",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let cap = capture(port, "reader", "reader", "postgres");
    let one = required(&["users"]);

    let report = cap.inspect_coverage(&one).await.unwrap();
    assert!(!report.satisfied);
    assert!(
        !report.manageable,
        "a non-owner read-only role can't manage"
    );
    assert!(
        !report.blockers.is_empty(),
        "a non-manageable verdict must explain why"
    );
    assert!(report.remediation[0].contains("CREATE PUBLICATION"));

    // Even asked to manage, a read-only role must not (and cannot) create it.
    cap.ensure_coverage(&one, true).await.unwrap();
    assert!(
        published_tables(&pool).await.is_empty(),
        "nothing should have been created"
    );

    // Nor may it set a replica identity on a table it doesn't own.
    let child = links(&[("orders", "user_id")]);
    let report = cap.ensure_pre_image(&child, true).await.unwrap();
    let gap = report.gaps.first().unwrap();
    assert!(!gap.manageable);
    assert!(gap.blockers.iter().any(|b| b.contains("does not own")));
    assert!(!cap.inspect_pre_image(&child).await.unwrap().satisfied());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn partitioned_tables_are_published_via_their_root() {
    let container = postgres().await;
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY)",
        "CREATE TABLE parted (id int PRIMARY KEY) PARTITION BY RANGE (id)",
        "CREATE TABLE parted_low PARTITION OF parted FOR VALUES FROM (0) TO (1000)",
        "CREATE ROLE reader LOGIN PASSWORD 'reader'",
        "GRANT SELECT ON ALL TABLES IN SCHEMA public TO reader",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let admin = capture(port, "postgres", "postgres", "postgres");
    let reader = capture(port, "reader", "reader", "postgres");
    let both = required(&["users", "parted"]);

    // A publication flusso creates publishes via the root.
    let report = admin.inspect_coverage(&both).await.unwrap();
    assert_eq!(report.missing.len(), 2);
    assert!(report.misrouted.is_empty());
    assert!(report.remediation[0].ends_with("WITH (publish_via_partition_root = true);"));
    admin.ensure_coverage(&both, true).await.unwrap();
    assert!(via_root(&pool).await);
    assert_eq!(
        published_tables(&pool).await,
        ["public.parted", "public.users"]
    );
    assert!(admin.inspect_coverage(&both).await.unwrap().satisfied);

    // Someone else's publication streams the partitions under their own names.
    sqlx::query("ALTER PUBLICATION flusso SET (publish_via_partition_root = false)")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        published_tables(&pool).await,
        ["public.parted_low", "public.users"]
    );

    // Without a partitioned table, it needs nothing.
    assert!(
        admin
            .inspect_coverage(&required(&["users"]))
            .await
            .unwrap()
            .satisfied
    );

    // With one, it is misrouted: present through its partition, not missing.
    let report = reader.inspect_coverage(&both).await.unwrap();
    assert!(!report.satisfied);
    assert!(report.missing.is_empty());
    assert_eq!(
        report.misrouted,
        required(&["parted"]).into_iter().collect::<Vec<_>>()
    );
    assert_eq!(
        report.remediation,
        ["ALTER PUBLICATION \"flusso\" SET (publish_via_partition_root = true);"]
    );
    assert!(!report.manageable);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.contains("does not own publication"))
    );

    // A role that doesn't own the publication leaves it alone.
    reader.ensure_coverage(&both, true).await.unwrap();
    assert!(!via_root(&pool).await);

    // Opted out: reported, left alone.
    admin.ensure_coverage(&both, false).await.unwrap();
    assert!(!via_root(&pool).await);

    // Managed: rerouted in place.
    admin.ensure_coverage(&both, true).await.unwrap();
    assert!(via_root(&pool).await);
    assert!(admin.inspect_coverage(&both).await.unwrap().satisfied);

    // A lone partition published on its own doesn't make its parent a member:
    // the parent is added, and the option set, in one go.
    for statement in [
        "CREATE TABLE solo (id int PRIMARY KEY) PARTITION BY RANGE (id)",
        "CREATE TABLE solo_only PARTITION OF solo FOR VALUES FROM (0) TO (1000)",
        "ALTER PUBLICATION flusso ADD TABLE solo_only",
        "ALTER PUBLICATION flusso SET (publish_via_partition_root = false)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let all = required(&["users", "parted", "solo"]);
    admin.ensure_coverage(&all, true).await.unwrap();
    assert!(via_root(&pool).await);
    assert!(
        published_tables(&pool)
            .await
            .contains(&"public.solo".to_owned())
    );
    assert!(admin.inspect_coverage(&all).await.unwrap().satisfied);

    // A member with no partitions yet (pg_partman creates them later) is still
    // a member: only the option is set, no ADD TABLE that would fail.
    for statement in [
        "CREATE TABLE bare (id int PRIMARY KEY) PARTITION BY RANGE (id)",
        "ALTER PUBLICATION flusso ADD TABLE bare",
        "ALTER PUBLICATION flusso SET (publish_via_partition_root = false)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let with_bare = required(&["users", "parted", "bare"]);
    let report = admin.inspect_coverage(&with_bare).await.unwrap();
    assert!(report.missing.is_empty(), "{:?}", report.missing);
    assert_eq!(
        report.remediation,
        ["ALTER PUBLICATION \"flusso\" SET (publish_via_partition_root = true);"]
    );
    admin.ensure_coverage(&with_bare, true).await.unwrap();
    assert!(via_root(&pool).await);
    assert!(admin.inspect_coverage(&with_bare).await.unwrap().satisfied);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_for_all_tables_publication_covers_partitioned_tables() {
    let container = postgres().await;
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY)",
        "CREATE TABLE parted (id int PRIMARY KEY) PARTITION BY RANGE (id)",
        "CREATE TABLE parted_low PARTITION OF parted FOR VALUES FROM (0) TO (1000)",
        "CREATE TABLE bare (id int PRIMARY KEY) PARTITION BY RANGE (id)",
        "CREATE PUBLICATION flusso FOR ALL TABLES",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let admin = capture(port, "postgres", "postgres", "postgres");
    let tables = required(&["users", "parted", "bare"]);

    let report = admin.inspect_coverage(&tables).await.unwrap();
    assert!(report.missing.is_empty(), "{:?}", report.missing);
    assert_eq!(
        report.misrouted,
        required(&["bare", "parted"])
            .into_iter()
            .collect::<Vec<_>>()
    );
    admin.ensure_coverage(&tables, true).await.unwrap();
    assert!(via_root(&pool).await);
    assert!(admin.inspect_coverage(&tables).await.unwrap().satisfied);
}

/// `table → [link columns]` under `public`, as `SourceSpec::pre_image_columns`
/// would name them.
fn links(entries: &[(&str, &str)]) -> PreImageColumns {
    entries
        .iter()
        .map(|(table, column)| {
            (
                required(&[table]).into_iter().next().unwrap(),
                vec![ColumnName::try_new(*column).unwrap()],
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn pre_image_gaps_follow_each_tables_replica_identity() {
    let container = postgres().await;
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY)",
        "CREATE TABLE by_default (id int PRIMARY KEY, user_id int NOT NULL)",
        "CREATE TABLE by_full (id int PRIMARY KEY, user_id int NOT NULL)",
        "ALTER TABLE by_full REPLICA IDENTITY FULL",
        "CREATE TABLE by_index (id int PRIMARY KEY, user_id int NOT NULL)",
        "CREATE UNIQUE INDEX by_index_identity ON by_index (id, user_id)",
        "ALTER TABLE by_index REPLICA IDENTITY USING INDEX by_index_identity",
        // An identity index on the link alone: changes key by it, so the
        // pre-image never carries more than the key — still a gap.
        "CREATE TABLE by_link_index (id int PRIMARY KEY, user_id int NOT NULL)",
        "CREATE UNIQUE INDEX by_link_index_identity ON by_link_index (user_id)",
        "ALTER TABLE by_link_index REPLICA IDENTITY USING INDEX by_link_index_identity",
        "CREATE TABLE junction (user_id int, tag_id int, PRIMARY KEY (user_id, tag_id))",
        "CREATE TABLE parted (id int, user_id int NOT NULL, PRIMARY KEY (id)) PARTITION BY RANGE (id)",
        "CREATE TABLE parted_low PARTITION OF parted FOR VALUES FROM (0) TO (1000)",
        "CREATE TABLE parted_high PARTITION OF parted FOR VALUES FROM (1000) TO (2000)",
        // Already carries the link: left as is.
        "ALTER TABLE parted_high REPLICA IDENTITY FULL",
        // A foreign partition (its parent can't have a unique index) has no
        // identity to set: it is left out rather than failing every start.
        "CREATE EXTENSION postgres_fdw",
        "CREATE SERVER remote FOREIGN DATA WRAPPER postgres_fdw",
        "CREATE TABLE spread (id int, user_id int NOT NULL) PARTITION BY RANGE (id)",
        "CREATE TABLE spread_near PARTITION OF spread FOR VALUES FROM (0) TO (1000)",
        "CREATE FOREIGN TABLE spread_far PARTITION OF spread \
         FOR VALUES FROM (1000) TO (2000) SERVER remote",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let cap = capture(port, "postgres", "postgres", "postgres");
    let report = cap
        .inspect_pre_image(&links(&[
            ("by_default", "user_id"),
            ("by_full", "user_id"),
            ("by_index", "user_id"),
            ("by_link_index", "user_id"),
            ("junction", "user_id"),
            ("missing_table", "user_id"),
        ]))
        .await
        .unwrap();

    let gaps: Vec<String> = report.gaps.iter().map(|g| g.table.to_string()).collect();
    assert_eq!(gaps, ["public.by_default", "public.by_link_index"]);
    let partial: Vec<String> = report.partial.iter().map(|g| g.table.to_string()).collect();
    assert_eq!(
        partial,
        ["public.by_index", "public.junction"],
        "they carry the link, but not the whole row"
    );
    assert!(report.partial.iter().all(|g| g.missing.is_empty()));
    let remediation = report.gaps.first().unwrap().remediation.clone();
    assert_eq!(
        remediation,
        "ALTER TABLE \"public\".\"by_default\" REPLICA IDENTITY FULL;"
    );

    assert!(
        report.gaps.first().unwrap().manageable,
        "superuser can set it"
    );

    // Opted out: reported, left alone.
    let default_only = links(&[("by_default", "user_id")]);
    cap.ensure_pre_image(&default_only, false).await.unwrap();
    assert!(
        !cap.inspect_pre_image(&default_only)
            .await
            .unwrap()
            .satisfied()
    );

    // Managed: flusso sets REPLICA IDENTITY FULL itself.
    let before = cap.ensure_pre_image(&default_only, true).await.unwrap();
    assert!(!before.satisfied(), "ensure returns what it found");
    assert!(
        cap.inspect_pre_image(&default_only)
            .await
            .unwrap()
            .satisfied()
    );
    let identity: String =
        sqlx::query_scalar("SELECT relreplident::text FROM pg_class WHERE relname = 'by_default'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(identity, "f");

    // A partitioned table is checked across its tree, and every member that
    // drops the link is set — the parent included.
    let parted = links(&[("parted", "user_id")]);
    let report = cap.inspect_pre_image(&parted).await.unwrap();
    let gap = report.gaps.first().unwrap();
    assert!(gap.manageable);
    assert_eq!(
        gap.remediation,
        "ALTER TABLE \"public\".\"parted\" REPLICA IDENTITY FULL; \
         ALTER TABLE \"public\".\"parted_low\" REPLICA IDENTITY FULL;"
    );
    cap.ensure_pre_image(&parted, true).await.unwrap();
    assert!(cap.inspect_pre_image(&parted).await.unwrap().satisfied());
    let identities: Vec<(String, String)> = sqlx::query_as(
        "SELECT relname::text, relreplident::text FROM pg_class \
         WHERE relname LIKE 'parted%' AND relkind IN ('r', 'p') ORDER BY relname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let identities: Vec<(&str, &str)> = identities
        .iter()
        .map(|(name, identity)| (name.as_str(), identity.as_str()))
        .collect();
    assert_eq!(
        identities,
        [("parted", "f"), ("parted_high", "f"), ("parted_low", "f")]
    );

    let spread = links(&[("spread", "user_id")]);
    let report = cap.inspect_pre_image(&spread).await.unwrap();
    assert_eq!(
        report.gaps.first().unwrap().remediation,
        "ALTER TABLE \"public\".\"spread\" REPLICA IDENTITY FULL; \
         ALTER TABLE \"public\".\"spread_near\" REPLICA IDENTITY FULL;"
    );
    cap.ensure_pre_image(&spread, true).await.unwrap();
    let near: String =
        sqlx::query_scalar("SELECT relreplident::text FROM pg_class WHERE relname = 'spread_near'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(near, "f");

    // A partition created later doesn't inherit the parent's identity: the
    // next inspection reports it.
    sqlx::query("CREATE TABLE parted_late PARTITION OF parted FOR VALUES FROM (2000) TO (3000)")
        .execute(&pool)
        .await
        .unwrap();
    let report = cap.inspect_pre_image(&parted).await.unwrap();
    assert_eq!(
        report.gaps.first().unwrap().remediation,
        "ALTER TABLE \"public\".\"parted_late\" REPLICA IDENTITY FULL;"
    );
}

fn column_field(name: &str) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        source: FieldSource::Column(Column {
            column: ColumnName::try_new(name).unwrap(),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
        }),
    }
}

fn join_field(name: &str, table: &str, kind: JoinKind) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new(table).unwrap(),
            kind,
            primary_key: ColumnName::try_new("id").unwrap(),
            nullable: true,
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![column_field("id")],
            on: vec![],
            indexed: true,
        })),
    }
}

/// `users` (the root) with its `belongs_to` team and its `has_many` orders.
fn users_spec() -> SourceSpec {
    let schema = IndexSchema {
        version: 1,
        table: TableName::try_new("users").unwrap(),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(ColumnName::try_new("id").unwrap()),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_field("name"),
            join_field(
                "team",
                "teams",
                JoinKind::BelongsTo {
                    column: ColumnName::try_new("team_id").unwrap(),
                },
            ),
            join_field(
                "orders",
                "orders",
                JoinKind::HasMany {
                    foreign_key: ColumnName::try_new("user_id").unwrap(),
                },
            ),
        ],
    };
    SourceSpec::new(BTreeMap::from([(
        IndexName::try_new("users").unwrap(),
        schema,
    )]))
}

const USERS_TABLES: &[&str] = &[
    "CREATE TABLE users (id int PRIMARY KEY, name text, team_id int)",
    "CREATE TABLE teams (id int PRIMARY KEY, name text)",
    "CREATE TABLE orders (id int PRIMARY KEY, user_id int)",
];

/// `relreplident` of each of `users`, `teams`, `orders`, by name.
async fn identities(pool: &PgPool) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT relname::text, relreplident::text FROM pg_class \
         WHERE relname IN ('users', 'teams', 'orders') ORDER BY relname",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(name, identity)| ((*name).to_owned(), (*identity).to_owned()))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn every_table_an_index_reads_is_set_to_full() {
    let container = postgres().await;
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in USERS_TABLES {
        sqlx::query(*statement).execute(&pool).await.unwrap();
    }
    let cap = capture(port, "postgres", "postgres", "postgres");
    let required = users_spec().pre_images();

    let report = cap.inspect_pre_image(&required).await.unwrap();
    let tables = |gaps: &[source::PreImageGap]| -> Vec<String> {
        gaps.iter().map(|gap| gap.table.to_string()).collect()
    };
    assert_eq!(tables(&report.gaps), ["public.orders"], "the has_many link");
    assert_eq!(
        tables(&report.partial),
        ["public.teams", "public.users"],
        "the belongs_to target and the root"
    );
    assert!(report.partial.iter().all(|gap| gap.manageable));

    // Opted out: reported, left alone.
    cap.ensure_pre_image(&required, false).await.unwrap();
    assert_eq!(
        identities(&pool).await,
        pairs(&[("orders", "d"), ("teams", "d"), ("users", "d")])
    );

    // A table held by a long transaction times out; the others are still set.
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT count(*) FROM users")
        .execute(&mut *blocker)
        .await
        .unwrap();
    cap.ensure_pre_image(&required, true).await.unwrap();
    assert_eq!(
        identities(&pool).await,
        pairs(&[("orders", "f"), ("teams", "f"), ("users", "d")])
    );
    blocker.rollback().await.unwrap();

    // The next start sets the one it missed.
    cap.ensure_pre_image(&required, true).await.unwrap();
    assert_eq!(
        identities(&pool).await,
        pairs(&[("orders", "f"), ("teams", "f"), ("users", "f")])
    );
    assert!(cap.inspect_pre_image(&required).await.unwrap().complete());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_table_the_role_does_not_own_is_reported_and_the_rest_are_set() {
    let container = postgres().await;
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in USERS_TABLES.iter().copied().chain([
        "CREATE ROLE app LOGIN PASSWORD 'app'",
        "GRANT SELECT ON ALL TABLES IN SCHEMA public TO app",
        "ALTER TABLE teams OWNER TO app",
        "ALTER TABLE orders OWNER TO app",
    ]) {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let cap = capture(port, "app", "app", "postgres");
    let required = users_spec().pre_images();

    let before = cap.ensure_pre_image(&required, true).await.unwrap();
    let users = before
        .partial
        .iter()
        .find(|gap| gap.table.to_string() == "public.users");
    let users = users.unwrap();
    assert!(!users.manageable);
    assert_eq!(users.blockers, ["role does not own table public.users"]);
    assert_eq!(
        identities(&pool).await,
        pairs(&[("orders", "f"), ("teams", "f"), ("users", "d")])
    );

    let after = cap.inspect_pre_image(&required).await.unwrap();
    assert!(after.satisfied(), "every link is carried");
    assert!(!after.complete());
    assert_eq!(after.partial.len(), 1);
}
