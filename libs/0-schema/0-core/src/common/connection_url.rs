use nutype::nutype;
use std::fmt;

#[nutype(
    sanitize(trim),
    validate(regex = r"^(postgresql|postgres)://\S+$"),
    derive(
        Debug,
        Clone,
        Display,
        AsRef,
        Deref,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    )
)]
pub struct ConnectionUrl(String);

#[derive(Debug, Clone, Default)]
pub enum Scheme {
    #[default]
    Postgresql,
    Postgres,
}

impl fmt::Display for Scheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Postgresql => write!(f, "postgresql"),
            Self::Postgres => write!(f, "postgres"),
        }
    }
}

#[bon::bon]
impl ConnectionUrl {
    #[builder]
    pub fn from_parts(
        #[builder(default)] scheme: Scheme,
        #[builder(into)] username: String,
        #[builder(into)] password: Option<String>,
        #[builder(into)] host: String,
        port: Option<u16>,
        #[builder(into)] database: Option<String>,
    ) -> Result<Self, ConnectionUrlError> {
        let mut url = format!("{}://{}", scheme, username);

        if let Some(pwd) = password {
            url.push(':');
            url.push_str(&pwd);
        }

        url.push('@');
        url.push_str(&host);

        if let Some(p) = port {
            url.push(':');
            url.push_str(&p.to_string());
        }

        if let Some(db) = database {
            url.push('/');
            url.push_str(&db);
        }

        Self::try_new(url)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
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
        assert_eq!(
            url.to_string(),
            "postgresql://user:s3cr3t@db.example.com:5432/mydb"
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
}
