use super::*;

fn connect(server: &str) -> ConnectArgs {
    ConnectArgs {
        server: server.to_owned(),
        admin_user: "admin".to_owned(),
        admin_password: "flusso".to_owned(),
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
