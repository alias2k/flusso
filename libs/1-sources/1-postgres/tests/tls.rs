//! End-to-end TLS test against a real Postgres that **only** accepts SSL
//! connections (a `hostssl`-only `pg_hba.conf`, like a managed provider's
//! default). Proves the replication stream and the SQL pool both negotiate
//! TLS from `sslmode=require` — and both fail without it — through the same
//! [`replication_config`]/[`sql_connection_url`] helpers the CLI uses.
//!
//! The server certificate under `tests/fixtures/tls/` is a **throwaway,
//! test-only** self-signed pair (its private key is deliberately public);
//! `require` performs no verification, so self-signed is fine.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p flusso-sources-postgres --test tls -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use futures::StreamExt;
use schema_core::{SourceTls, SslMode};
use sources_core::cdc::ChangeCapture;
use sources_postgres::{WalChangeCapture, replication_config, sql_connection_url};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::CopyDataSource;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

/// Reject every non-SSL TCP connection; local socket stays open for initdb.
const PG_HBA: &str = "local all all trust\n\
                      hostssl all all 0.0.0.0/0 scram-sha-256\n\
                      hostssl all all ::0/0 scram-sha-256\n";

/// The stock entrypoint can't run TLS out of the box: the key must be owned
/// by the server user with tight permissions, so a wrapper (running as root)
/// chowns it first, then hands over to the standard entrypoint with SSL
/// forced on and the hostssl-only HBA.
const BOOT: &str = "chown postgres:postgres /tls/server.key \
                    && chmod 600 /tls/server.key \
                    && exec docker-entrypoint.sh postgres \
                       -c wal_level=logical \
                       -c ssl=on \
                       -c ssl_cert_file=/tls/server.crt \
                       -c ssl_key_file=/tls/server.key \
                       -c hba_file=/tls/pg_hba.conf";

async fn ssl_only_postgres() -> ContainerAsync<Postgres> {
    // PG 14+ is required: the replication client requests the pgoutput
    // `messages` option, added in 14 (same as the engine's wal e2e).
    Postgres::default()
        .with_tag("16-alpine")
        .with_copy_to(
            "/tls/server.crt",
            CopyDataSource::Data(include_bytes!("fixtures/tls/server.crt").to_vec()),
        )
        .with_copy_to(
            "/tls/server.key",
            CopyDataSource::Data(include_bytes!("fixtures/tls/server.key").to_vec()),
        )
        .with_copy_to(
            "/tls/pg_hba.conf",
            CopyDataSource::Data(PG_HBA.as_bytes().to_vec()),
        )
        .with_cmd(["sh", "-c", BOOT])
        .start()
        .await
        .unwrap()
}

fn capture(url: &str, tls: &SourceTls) -> WalChangeCapture {
    let replication = replication_config(url, tls, "flusso_tls", "flusso_tls").unwrap();
    let sql_url = sql_connection_url(url, tls).unwrap();
    WalChangeCapture::new(replication, sql_url)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn tls_required_server_accepts_require_and_rejects_disable() {
    let container = ssl_only_postgres().await;
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let base = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    // The SQL side honors the URL's sslmode (sqlx): require connects,
    // disable is turned away by the hostssl-only HBA.
    let pool = PgPoolOptions::new()
        .connect(&format!("{base}?sslmode=require"))
        .await
        .unwrap();
    assert!(
        PgPoolOptions::new()
            .connect(&format!("{base}?sslmode=disable"))
            .await
            .is_err(),
        "a plaintext connection must be rejected by the hostssl-only server",
    );

    sqlx::query("CREATE TABLE t (id int PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE PUBLICATION flusso_tls FOR TABLE t")
        .execute(&pool)
        .await
        .unwrap();

    // The replication stream: `live` creates the slot over the (TLS'd) admin
    // pool and opens the pgwire replication connection — the handshake this
    // feature exists for. A URL-borne sslmode=require must carry through.
    let url = format!("{base}?sslmode=require");
    let mut stream = capture(&url, &SourceTls::default()).live().await.unwrap();

    // A change flows over the encrypted stream end-to-end.
    sqlx::query("INSERT INTO t (id) VALUES (1)")
        .execute(&pool)
        .await
        .unwrap();
    let change = tokio::time::timeout(std::time::Duration::from_secs(10), stream.next())
        .await
        .expect("a change should arrive over the TLS stream")
        .expect("stream should not end")
        .expect("change should decode");
    drop(change);
    drop(stream);

    // Config keys work without any URL parameter (the flat ssl_mode key).
    let config_tls = SourceTls {
        mode: Some(SslMode::Require),
        ..SourceTls::default()
    };
    let stream = capture(&base, &config_tls)
        .live()
        .await
        .expect("ssl_mode=require from config must open the stream");
    drop(stream);

    // And an explicit disable fails against this server, for both connections.
    let disabled = SourceTls {
        mode: Some(SslMode::Disable),
        ..SourceTls::default()
    };
    assert!(
        capture(&base, &disabled).live().await.is_err(),
        "ssl_mode=disable must be rejected by the hostssl-only server",
    );
}
