#![allow(
    unsafe_code,
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used
)]

//! Resolution of the source connection in the running environment: the
//! `SOURCE_POSTGRES_CONNECTION_URL` override and the `{ env = "VAR" }` form.
//!
//! One test, run as steps, because every step reads or writes the same
//! process-wide variable; separate `#[test]`s would race under `cargo test`.

use kernel::{AdapterConfig, Options, ResolveError};
use source_postgres::PostgresConfig;

const OVERRIDE: &str = "SOURCE_POSTGRES_CONNECTION_URL";

fn config(toml: &str) -> PostgresConfig {
    let options: Options = toml::from_str(toml).unwrap();
    PostgresConfig::from_options(options).unwrap()
}

fn set(var: &str, value: &str) {
    // SAFETY: this binary's single test mutates the environment sequentially.
    unsafe { std::env::set_var(var, value) };
}

fn unset(var: &str) {
    // SAFETY: see `set`.
    unsafe { std::env::remove_var(var) };
}

#[test]
fn connection_url_resolution_precedence() {
    unset(OVERRIDE);
    assert_eq!(PostgresConfig::connection_url_var(), OVERRIDE);

    // A literal URL resolves to itself.
    let literal = config("connection_url = \"postgres://app@db.internal/shop\"");
    assert_eq!(
        literal.resolve_connection_url().unwrap().as_ref(),
        "postgres://app@db.internal/shop"
    );

    // Parts assemble a URL; the password may come from its own variable.
    set("TEST_PG_PARTS_PW", "s3cret");
    let parts = config(
        "connection_url = { host = \"db\", port = 5433, user = \"app\", password = { env = \"TEST_PG_PARTS_PW\" }, database = \"shop\" }",
    );
    assert_eq!(
        parts.resolve_connection_url().unwrap().as_ref(),
        "postgresql://app:s3cret@db:5433/shop"
    );
    unset("TEST_PG_PARTS_PW");

    // The parts' password can also come from the nested override variable.
    set("SOURCE_POSTGRES_CONNECTION_URL_PASSWORD", "from-override");
    let parts = config("connection_url = { host = \"db\", user = \"app\", database = \"shop\" }");
    assert_eq!(
        parts.resolve_connection_url().unwrap().as_ref(),
        "postgresql://app:from-override@db:5432/shop"
    );
    unset("SOURCE_POSTGRES_CONNECTION_URL_PASSWORD");

    // Omitted with no override: a clear error naming the variable.
    let omitted = config("");
    match omitted.resolve_connection_url() {
        Err(ResolveError::Missing(message)) => assert!(message.contains(OVERRIDE), "{message}"),
        other => panic!("expected a missing-connection error, got {other:?}"),
    }

    // An `{ env }` URL whose variable is unset names that variable.
    let env = config("connection_url = { env = \"TEST_PG_UNSET_URL_XYZ\" }");
    match env.resolve_connection_url() {
        Err(ResolveError::EnvNotSet(var)) => assert_eq!(var, "TEST_PG_UNSET_URL_XYZ"),
        other => panic!("expected an unset-variable error, got {other:?}"),
    }

    // An invalid resolved URL is rejected.
    let bad = config("connection_url = \"mysql://nope\"");
    assert!(matches!(
        bad.resolve_connection_url(),
        Err(ResolveError::Invalid(_))
    ));

    // The override wins over a literal, over parts, and fills an omission.
    set(OVERRIDE, "postgres://env@envhost/envdb");
    for cfg in [&literal, &parts, &omitted] {
        assert_eq!(
            cfg.resolve_connection_url().unwrap().as_ref(),
            "postgres://env@envhost/envdb"
        );
    }

    // …but an explicit `{ env }` reference names its own source and beats it.
    set("TEST_PG_EXPLICIT_URL", "postgres://explicit@host/db");
    let explicit = config("connection_url = { env = \"TEST_PG_EXPLICIT_URL\" }");
    assert_eq!(
        explicit.resolve_connection_url().unwrap().as_ref(),
        "postgres://explicit@host/db"
    );
    unset("TEST_PG_EXPLICIT_URL");
    unset(OVERRIDE);
}
