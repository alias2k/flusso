//! HTTP Basic authentication for the private control surface.
//!
//! The credentials default to `admin` / `flusso` so the surface works out of the
//! box; the binary logs a loud warning while the password is still the default
//! (see [`BasicAuth::uses_default_password`]). They are read from the CLI flags /
//! env vars only — never the config file — because they are secrets.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub(crate) const DEFAULT_ADMIN_USER: &str = "admin";
/// Default password for the private surface. Running with it unchanged triggers
/// a startup warning.
pub(crate) const DEFAULT_ADMIN_PASSWORD: &str = "flusso";

/// The credential the private surface checks each request against.
#[derive(Clone)]
pub(crate) struct BasicAuth {
    user: String,
    password: String,
}

impl std::fmt::Debug for BasicAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the password.
        f.debug_struct("BasicAuth")
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

impl BasicAuth {
    pub(crate) fn new(user: String, password: String) -> Self {
        Self { user, password }
    }

    /// Whether the password is still the built-in default — the binary warns on
    /// every start while this holds.
    pub(crate) fn uses_default_password(&self) -> bool {
        self.password == DEFAULT_ADMIN_PASSWORD
    }

    /// Validate an `Authorization` header value against the configured
    /// credentials. Returns `false` for anything that isn't a well-formed
    /// `Basic` header whose decoded `user:password` matches.
    fn check(&self, header: &HeaderValue) -> bool {
        let Some(encoded) = header
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix("Basic "))
        else {
            return false;
        };
        let Ok(decoded) = STANDARD.decode(encoded.trim()) else {
            return false;
        };
        let Ok(decoded) = String::from_utf8(decoded) else {
            return false;
        };
        let Some((user, password)) = decoded.split_once(':') else {
            return false;
        };
        // Non-short-circuiting `&` so both comparisons always run — neither the
        // username nor the password match leaks via response timing.
        ct_eq(user.as_bytes(), self.user.as_bytes())
            & ct_eq(password.as_bytes(), self.password.as_bytes())
    }
}

/// Constant-time byte-slice equality. Length difference is allowed to leak (it
/// short-circuits on mismatched lengths), as is standard for credential checks.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Axum middleware requiring valid Basic credentials, else `401` with a
/// `WWW-Authenticate` challenge. Wired with
/// [`from_fn_with_state`](axum::middleware::from_fn_with_state) so it carries the
/// [`BasicAuth`] independently of the router's own state.
pub(crate) async fn require_basic_auth(
    State(auth): State<Arc<BasicAuth>>,
    request: Request,
    next: Next,
) -> Response {
    match request.headers().get(header::AUTHORIZATION) {
        Some(value) if auth.check(value) => next.run(request).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                r#"Basic realm="flusso", charset="UTF-8""#,
            )],
            "unauthorized\n",
        )
            .into_response(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
