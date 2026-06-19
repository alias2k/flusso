use super::*;

// --- try_new: valid inputs ---

#[test]
fn valid_full_url() {
    assert!(ConnectionUrl::try_new("postgresql://user:pass@localhost:5432/mydb").is_ok());
}

#[test]
fn valid_minimal_url() {
    assert!(ConnectionUrl::try_new("postgresql://user@localhost").is_ok());
}

#[test]
fn valid_postgres_alias_scheme() {
    assert!(ConnectionUrl::try_new("postgres://user@localhost/db").is_ok());
}

#[test]
fn valid_no_port() {
    assert!(ConnectionUrl::try_new("postgresql://user:pass@db.example.com/mydb").is_ok());
}

#[test]
fn valid_no_database() {
    assert!(ConnectionUrl::try_new("postgresql://user@localhost:5432").is_ok());
}

#[test]
fn sanitizes_surrounding_whitespace() {
    let url = ConnectionUrl::try_new("  postgresql://user@localhost  ").unwrap();
    assert_eq!(url.to_string(), "postgresql://user@localhost");
}

// --- try_new: invalid inputs ---

#[test]
fn invalid_empty_string() {
    assert!(ConnectionUrl::try_new("").is_err());
}

#[test]
fn invalid_no_scheme() {
    assert!(ConnectionUrl::try_new("user:pass@localhost/db").is_err());
}

#[test]
fn invalid_unsupported_scheme() {
    assert!(ConnectionUrl::try_new("mysql://user@localhost/db").is_err());
}

#[test]
fn invalid_http_scheme() {
    assert!(ConnectionUrl::try_new("http://user@localhost").is_err());
}

#[test]
fn invalid_scheme_only() {
    assert!(ConnectionUrl::try_new("postgresql://").is_err());
}

#[test]
fn invalid_whitespace_inside_url() {
    assert!(ConnectionUrl::try_new("postgresql://user @localhost").is_err());
}

// --- builder: valid combinations ---

#[test]
fn builder_full_url() {
    let url = ConnectionUrl::from_parts()
        .scheme(Scheme::Postgresql)
        .username("user")
        .password("s3cr3t")
        .host("db.example.com")
        .port(5432_u16)
        .database("mydb")
        .call()
        .unwrap();
    // The real value is read through `AsRef`; `Display` redacts.
    assert_eq!(
        url.as_ref(),
        "postgresql://user:s3cr3t@db.example.com:5432/mydb"
    );
    assert_eq!(
        url.to_string(),
        "postgresql://user:***@db.example.com:5432/mydb"
    );
}

#[test]
fn builder_minimal_url() {
    let url = ConnectionUrl::from_parts()
        .username("user")
        .host("localhost")
        .call()
        .unwrap();
    assert_eq!(url.to_string(), "postgresql://user@localhost");
}

#[test]
fn builder_default_scheme_is_postgresql() {
    let url = ConnectionUrl::from_parts()
        .username("user")
        .host("localhost")
        .call()
        .unwrap();
    assert!(url.to_string().starts_with("postgresql://"));
}

#[test]
fn builder_postgres_scheme() {
    let url = ConnectionUrl::from_parts()
        .scheme(Scheme::Postgres)
        .username("user")
        .host("localhost")
        .call()
        .unwrap();
    assert!(url.to_string().starts_with("postgres://"));
}

#[test]
fn builder_omits_optional_parts_when_absent() {
    let url = ConnectionUrl::from_parts()
        .username("user")
        .host("localhost")
        .call()
        .unwrap()
        .to_string();
    assert_eq!(url, "postgresql://user@localhost");
}
