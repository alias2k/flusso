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

/// `flusso reindex` posts to the index's own path (slice 7); a name that is
/// no index name is refused before any request.
#[test]
fn a_reindex_posts_to_the_index_path() {
    assert_eq!(
        index_path("users", "reindex").unwrap(),
        "/indexes/users/reindex"
    );
    assert!(index_path("no such/index", "reindex").is_err());
}

/// What `flusso reindex` prints for the server's `202` body.
#[test]
fn a_queued_reindex_is_printed_from_its_json() {
    assert_eq!(
        queued_message(r#"{"index":"users","sink":"primary","queued":true}"#),
        "reindex of users on primary queued"
    );
    assert_eq!(
        queued_message(r#"{"index":"users","sink":null,"queued":true}"#),
        "reindex of users on every sink queued"
    );
    assert_eq!(
        queued_message(
            r#"{"index":"users","sink":null,"queued":true,"queued_on":["audit","primary"]}"#
        ),
        "reindex of users queued on audit, primary"
    );
    // The docs pass of the closing round: the sinks left out are named, each
    // with its reason.
    assert_eq!(
        queued_message(
            r#"{"index":"users","sink":null,"queued":true,"queued_on":["primary"],
                "skipped":[{"sink":"audit","reason":"this sink can't rebuild the index"}]}"#
        ),
        "reindex of users queued on primary\n\
         skipped on audit: this sink can't rebuild the index"
    );
    assert_eq!(queued_message("plain text\n"), "plain text");
}

/// `flusso reattach` posts to the index's own path, and without
/// `--accept-gap` refuses before any request is made (O16).
#[tokio::test]
async fn a_reattach_without_accept_gap_is_refused_before_any_request() {
    assert_eq!(
        index_path("users", "reattach").unwrap(),
        "/indexes/users/reattach"
    );
    let error = reattach(ReattachArgs {
        index: "users".to_owned(),
        sink: "audit".to_owned(),
        accept_gap: false,
        connect: connect("127.0.0.1:1"),
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("--accept-gap"), "{error}");
}

/// Review of step 4, m5: `reattach --sink` is given on the command line
/// only; `FLUSSO_SINK` never picks the lane a reattach drops changes on.
#[test]
fn reattach_reads_its_sink_from_the_command_line_only() {
    use clap::CommandFactory;
    let cli = crate::Cli::command();
    let reattach = cli.find_subcommand("reattach").unwrap();
    let sink = reattach
        .get_arguments()
        .find(|arg| arg.get_id() == "sink")
        .unwrap();
    assert!(sink.get_env().is_none());
    assert!(sink.is_required_set());
}
