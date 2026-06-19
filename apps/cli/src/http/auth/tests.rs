use super::*;

fn header(user: &str, password: &str) -> HeaderValue {
    let token = STANDARD.encode(format!("{user}:{password}"));
    HeaderValue::from_str(&format!("Basic {token}")).unwrap()
}

fn auth() -> BasicAuth {
    BasicAuth::new("admin".to_owned(), "flusso".to_owned())
}

#[test]
fn accepts_correct_credentials() {
    assert!(auth().check(&header("admin", "flusso")));
}

#[test]
fn rejects_wrong_password() {
    assert!(!auth().check(&header("admin", "wrong")));
}

#[test]
fn rejects_wrong_user() {
    assert!(!auth().check(&header("root", "flusso")));
}

#[test]
fn rejects_non_basic_and_garbage() {
    assert!(!auth().check(&HeaderValue::from_static("Bearer abc")));
    assert!(!auth().check(&HeaderValue::from_static("Basic !!!not-base64")));
}

#[test]
fn flags_the_default_password() {
    assert!(
        BasicAuth::new("admin".to_owned(), DEFAULT_ADMIN_PASSWORD.to_owned())
            .uses_default_password()
    );
    assert!(!BasicAuth::new("admin".to_owned(), "changed".to_owned()).uses_default_password());
}
