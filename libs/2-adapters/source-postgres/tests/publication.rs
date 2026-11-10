//! End-to-end tests for publication management (the [`CaptureProvisioning`]
//! impl) against a real Postgres in a container. These exercise the coverage
//! inspection, the privilege verdict, and the actual `CREATE`/`ALTER PUBLICATION`
//! provisioning that unit tests can only check by generated-string assertion —
//! plus the replica-identity half: the report (`inspect_pre_image`) across
//! `DEFAULT`, `FULL`, `USING INDEX`, a composite-key junction, and a
//! partitioned table across its partitions, and
//! `ensure_pre_image` setting `FULL` when privileged and refusing when not.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p sources-postgres --test publication -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeSet;

use kernel::{ColumnName, DatabaseSchema, TableName};
use source::{CaptureProvisioning, PreImageColumns, QualifiedTable};
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

    // A lone partition published on its own looks like its parent until the
    // option is set; then the parent is added too.
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
