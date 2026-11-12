use super::request_error;

/// A real connection failure: a request to a local port nothing listens on.
async fn refused(url_for_port: impl Fn(u16) -> String) -> reqwest::Error {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    reqwest::Client::new()
        .get(url_for_port(port))
        .send()
        .await
        .expect_err("nothing listens on the port")
}

fn deepest_cause(error: &reqwest::Error) -> String {
    let mut cause: &dyn std::error::Error = error;
    while let Some(next) = cause.source() {
        cause = next;
    }
    cause.to_string()
}

#[tokio::test]
async fn the_source_chain_is_rendered_after_the_context() {
    let error = refused(|port| format!("http://127.0.0.1:{port}/_cat/indices")).await;
    let deepest = deepest_cause(&error);
    assert!(!error.to_string().contains(&deepest), "{error}");

    let message = request_error("listing indices failed", error).to_string();

    assert!(
        message
            .contains("listing indices failed: error sending request for url (http://127.0.0.1:"),
        "{message}"
    );
    assert!(message.ends_with(&deepest), "{message}");
}

#[tokio::test]
async fn credentials_in_the_url_never_reach_the_message() {
    let sent = refused(|port| format!("http://flusso-user:s3cret-pass@127.0.0.1:{port}/")).await;
    let message = request_error("index check failed", sent).to_string();
    assert!(!message.contains("s3cret-pass"), "{message}");
    assert!(!message.contains("flusso-user"), "{message}");

    let with_userinfo = refused(|port| format!("http://127.0.0.1:{port}/"))
        .await
        .with_url(
            "http://flusso-user:s3cret-pass@127.0.0.1:9200/"
                .parse()
                .unwrap(),
        );
    assert!(
        with_userinfo.to_string().contains("s3cret-pass"),
        "{with_userinfo}"
    );

    let message = request_error("index check failed", with_userinfo).to_string();

    assert!(!message.contains("s3cret-pass"), "{message}");
    assert!(!message.contains("flusso-user"), "{message}");
    assert!(message.contains("(http://127.0.0.1:9200/)"), "{message}");
}
