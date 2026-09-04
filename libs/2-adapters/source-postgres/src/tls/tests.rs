use crate::config::{SslMode, Tls};
use pgwire_replication::SslMode as PgSslMode;

use super::*;

const URL: &str = "postgres://app:pw@db.example.com:5433/appdb";

fn tls(config: Tls, url: &str) -> TlsConfig {
    replication_config(url, &config, "slot", "pub").unwrap().tls
}

#[test]
fn replication_config_keeps_the_url_parts() {
    let config = replication_config(URL, &Tls::default(), "myslot", "mypub").unwrap();
    assert_eq!(config.host, "db.example.com");
    assert_eq!(config.port, 5433);
    assert_eq!(config.user, "app");
    assert_eq!(config.password, "pw");
    assert_eq!(config.database, "appdb");
    assert_eq!(config.slot, "myslot");
    assert_eq!(config.publication, "mypub");
}

#[test]
fn database_defaults_to_the_user() {
    let config =
        replication_config("postgres://app@localhost", &Tls::default(), "slot", "pub").unwrap();
    assert_eq!(config.database, "app");
}

#[test]
fn missing_user_is_an_error() {
    let err =
        replication_config("postgres://localhost/db", &Tls::default(), "slot", "pub").unwrap_err();
    assert!(err.to_string().contains("no user"), "{err}");
}

#[test]
fn default_mode_is_prefer() {
    assert_eq!(tls(Tls::default(), URL).mode, PgSslMode::Prefer);
}

#[test]
fn url_sslmode_is_honored() {
    for (token, expected) in [
        ("disable", PgSslMode::Disable),
        ("prefer", PgSslMode::Prefer),
        ("require", PgSslMode::Require),
        ("verify-ca", PgSslMode::VerifyCa),
        ("verify-full", PgSslMode::VerifyFull),
    ] {
        let url = format!("{URL}?sslmode={token}");
        assert_eq!(tls(Tls::default(), &url).mode, expected, "{token}");
    }
}

#[test]
fn url_sslmode_allow_maps_to_prefer() {
    let url = format!("{URL}?sslmode=allow");
    assert_eq!(tls(Tls::default(), &url).mode, PgSslMode::Prefer);
}

#[test]
fn url_invalid_sslmode_is_an_error() {
    let url = format!("{URL}?sslmode=sideways");
    let err = replication_config(&url, &Tls::default(), "slot", "pub").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("sideways") && msg.contains("verify-full"),
        "{msg}"
    );
}

#[test]
fn url_cert_params_are_honored() {
    let url = format!("{URL}?sslmode=verify-ca&sslrootcert=/ca.pem&sslcert=/c.pem&sslkey=/k.pem");
    let tls = tls(Tls::default(), &url);
    assert_eq!(tls.ca_pem_path.as_deref(), Some("/ca.pem".as_ref()));
    assert_eq!(tls.client_cert_pem_path.as_deref(), Some("/c.pem".as_ref()));
    assert_eq!(tls.client_key_pem_path.as_deref(), Some("/k.pem".as_ref()));
}

#[test]
fn config_keys_win_over_url_params() {
    let url = format!("{URL}?sslmode=require&sslrootcert=/url-ca.pem");
    let config = Tls {
        mode: Some(SslMode::VerifyFull),
        root_cert: Some("/config-ca.pem".into()),
        ..Tls::default()
    };
    let tls = tls(config, &url);
    assert_eq!(tls.mode, PgSslMode::VerifyFull);
    assert_eq!(tls.ca_pem_path.as_deref(), Some("/config-ca.pem".as_ref()));
}

#[test]
fn sni_hostname_comes_from_config_only() {
    let config = Tls {
        sni_hostname: Some("db.internal".to_owned()),
        ..Tls::default()
    };
    assert_eq!(
        tls(config, URL).sni_hostname.as_deref(),
        Some("db.internal")
    );
    assert_eq!(tls(Tls::default(), URL).sni_hostname, None);
}

#[test]
fn client_cert_without_key_is_an_error() {
    let config = Tls {
        client_cert: Some("/c.pem".into()),
        ..Tls::default()
    };
    let err = replication_config(URL, &config, "slot", "pub").unwrap_err();
    assert!(err.to_string().contains("mutual TLS"), "{err}");
}

#[test]
fn client_cert_and_key_may_come_from_different_surfaces() {
    // The pairing is judged after the merge: a key from the URL completes a
    // cert from the config.
    let url = format!("{URL}?sslkey=/k.pem");
    let config = Tls {
        client_cert: Some("/c.pem".into()),
        ..Tls::default()
    };
    let tls = tls(config, &url);
    assert_eq!(tls.client_cert_pem_path.as_deref(), Some("/c.pem".as_ref()));
    assert_eq!(tls.client_key_pem_path.as_deref(), Some("/k.pem".as_ref()));
}

#[test]
fn repeated_url_param_last_wins() {
    let url = format!("{URL}?sslmode=disable&sslmode=require");
    assert_eq!(tls(Tls::default(), &url).mode, PgSslMode::Require);
}

#[test]
fn sql_url_unchanged_without_config_keys() {
    let url = format!("{URL}?sslmode=verify-full&application_name=flusso");
    assert_eq!(sql_connection_url(&url, &Tls::default()).unwrap(), url);
}

#[test]
fn sql_url_unchanged_when_only_sni_is_set() {
    // SNI has no sqlx-side representation, so it alone must not rewrite the URL.
    let config = Tls {
        sni_hostname: Some("db.internal".to_owned()),
        ..Tls::default()
    };
    assert_eq!(sql_connection_url(URL, &config).unwrap(), URL);
}

#[test]
fn sql_url_appends_config_keys() {
    let config = Tls {
        mode: Some(SslMode::VerifyFull),
        root_cert: Some("/ca.pem".into()),
        ..Tls::default()
    };
    let url = sql_connection_url(URL, &config).unwrap();
    assert!(url.contains("sslmode=verify-full"), "{url}");
    assert!(url.contains("sslrootcert=%2Fca.pem"), "{url}");
}

#[test]
fn sql_url_overrides_existing_params_and_keeps_others() {
    let url = format!("{URL}?sslmode=disable&application_name=flusso");
    let config = Tls {
        mode: Some(SslMode::Require),
        ..Tls::default()
    };
    let rewritten = sql_connection_url(&url, &config).unwrap();
    assert!(rewritten.contains("sslmode=require"), "{rewritten}");
    assert!(!rewritten.contains("sslmode=disable"), "{rewritten}");
    assert!(rewritten.contains("application_name=flusso"), "{rewritten}");
}

#[test]
fn sql_url_rejects_the_same_bad_merge_as_the_stream() {
    let config = Tls {
        mode: Some(SslMode::Require),
        client_cert: Some("/c.pem".into()),
        ..Tls::default()
    };
    let err = sql_connection_url(URL, &config).unwrap_err();
    assert!(err.to_string().contains("mutual TLS"), "{err}");
}

#[test]
fn sql_url_roundtrips_through_sqlx_param_names() {
    let config = Tls {
        mode: Some(SslMode::VerifyCa),
        root_cert: Some("/ca.pem".into()),
        client_cert: Some("/c.pem".into()),
        client_key: Some("/k.pem".into()),
        ..Tls::default()
    };
    let rewritten = sql_connection_url(URL, &config).unwrap();
    // The rewritten URL feeds resolve_tls again with an unset config and lands
    // on the same effective decision — the two surfaces agree.
    let tls = tls(Tls::default(), &rewritten);
    assert_eq!(tls.mode, PgSslMode::VerifyCa);
    assert_eq!(tls.ca_pem_path.as_deref(), Some("/ca.pem".as_ref()));
    assert_eq!(tls.client_cert_pem_path.as_deref(), Some("/c.pem".as_ref()));
    assert_eq!(tls.client_key_pem_path.as_deref(), Some("/k.pem".as_ref()));
}
