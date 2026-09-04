use nutype::nutype;
use std::fmt;

// `Display` and `Debug` are hand-written below to redact the password, so a
// printed or logged `ConnectionUrl` never leaks it. `Serialize` emits the real
// value: a resolved URL is only serialized into artifacts the operator already
// holds, and the real value is needed to reconstruct it. Read the real value
// through `AsRef`/`Deref` (`.as_ref()`), never through `Display`.
#[nutype(
    sanitize(trim),
    validate(regex = r"^(postgresql|postgres)://\S+$"),
    derive(Clone, AsRef, Deref, Hash, Eq, PartialEq, Serialize, Deserialize)
)]
pub struct ConnectionUrl(String);

/// Mask the password in a `scheme://user:password@host…` URL, leaving the rest
/// intact: `postgres://user:s3cr3t@host/db` → `postgres://user:***@host/db`. A
/// URL with no password (or no userinfo) is returned unchanged.
fn redact_password(url: &str) -> String {
    let Some(after_scheme) = url.find("://").map(|i| i + 3) else {
        return url.to_owned();
    };
    let Some(at) = url[after_scheme..].find('@').map(|i| after_scheme + i) else {
        return url.to_owned();
    };
    let userinfo = &url[after_scheme..at];
    match userinfo.find(':') {
        Some(colon) => format!(
            "{}{}:***{}",
            &url[..after_scheme],
            &userinfo[..colon],
            &url[at..]
        ),
        None => url.to_owned(),
    }
}

/// Redacts the password — safe to log or print.
impl fmt::Display for ConnectionUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&redact_password(self.as_ref()))
    }
}

/// Redacts the password — safe to log or print.
impl fmt::Debug for ConnectionUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConnectionUrl({})", redact_password(self.as_ref()))
    }
}

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
mod tests;
