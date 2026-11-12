use super::*;

fn connect(server: &str) -> ConnectArgs {
    ConnectArgs {
        server: server.to_owned(),
        admin_user: "admin".to_owned(),
        admin_password: "flusso".to_owned(),
        ca_file: None,
    }
}

#[test]
fn base_url_defaults_a_bare_host_port_to_http() {
    assert_eq!(
        connect("127.0.0.1:9465").base_url(),
        "http://127.0.0.1:9465"
    );
}

#[test]
fn base_url_keeps_an_explicit_scheme_and_trims_a_trailing_slash() {
    assert_eq!(
        connect("https://flusso.internal:9465/").base_url(),
        "https://flusso.internal:9465"
    );
    assert_eq!(connect("http://host:9465").base_url(), "http://host:9465");
}

#[test]
fn a_missing_ca_file_fails_before_any_request() {
    let args = ConnectArgs {
        ca_file: Some(PathBuf::from("/no/such/flusso-ca.pem")),
        ..connect("https://flusso.internal:9465")
    };
    let error = format!(
        "{:#}",
        args.request(reqwest::Method::GET, "/indexes").unwrap_err()
    );
    assert!(
        error.contains("reading --ca-file /no/such/flusso-ca.pem"),
        "{error}"
    );
}

#[test]
fn a_ca_file_without_a_certificate_is_refused() {
    let path = std::env::temp_dir().join(format!("flusso-admin-ca-{}.pem", std::process::id()));
    std::fs::write(&path, "not a certificate\n").unwrap();
    let args = ConnectArgs {
        ca_file: Some(path.clone()),
        ..connect("https://flusso.internal:9465")
    };
    let error = format!(
        "{:#}",
        args.request(reqwest::Method::GET, "/indexes").unwrap_err()
    );
    let _ = std::fs::remove_file(path);
    assert!(error.contains("holds no PEM certificate"), "{error}");
}
