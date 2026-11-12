//! The [`Client`] transport. Reads go straight to OpenSearch, to the index
//! flusso maintains. The one thing a client asks flusso itself is to wait: a
//! fence, so a read sees a commit that was just made (see [`crate::fence`]).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use crate::error::{Error, Result};
use crate::fence::{After, FenceConfig, Level};

/// A connection to an OpenSearch cluster.
///
/// Cheap to clone (it shares one connection pool). Construct with
/// [`Client::connect`], optionally adding credentials with
/// [`Client::basic_auth`] and a private CA with [`Client::ca_file`].
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) http: reqwest::Client,
    /// CA certificates trusted on top of the Mozilla roots; see
    /// [`Client::ca_pem`].
    private_roots: Vec<reqwest::Certificate>,
    /// Base URL with any trailing slash trimmed.
    base: String,
    auth: Option<(String, String)>,
    /// Literal prefix prepended to every index name a request addresses, so a
    /// consumer can read a prefixed deployment's indexes (`dev_users_<hash>`).
    /// Empty by default; set with [`Client::index_prefix`]. Must match the
    /// `prefix` the writing flusso instance is running with.
    pub(crate) index_prefix: String,
    /// Where and how this client fences; see [`Client::flusso`].
    pub(crate) fence: FenceConfig,
    /// The position this client's reads wait for; set by [`Client::after`].
    pub(crate) after: Option<Arc<After>>,
}

/// The Mozilla root bundle, which the client has always verified against:
/// reqwest 0.13 would otherwise read the OS trust store.
fn mozilla_roots() -> impl Iterator<Item = reqwest::Certificate> {
    webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|der| reqwest::Certificate::from_der(der).ok())
}

/// `error` and each cause in its source chain, joined with `": "`: reqwest's
/// own line names no certificate.
fn chain(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

impl Client {
    /// Connect to the cluster at `url` (`http` or `https`). This validates the
    /// URL and builds the HTTP client; it does not perform any I/O. An `https`
    /// certificate is verified against the Mozilla root bundle built into the
    /// client, not the OS trust store; add a private CA with
    /// [`Client::ca_file`]. There is no switch to skip verification.
    pub fn connect(url: impl AsRef<str>) -> Result<Self> {
        let raw = url.as_ref();
        let parsed = Url::parse(raw).map_err(|error| Error::Url(format!("{raw}: {error}")))?;
        match parsed.scheme() {
            "http" | "https" => {}
            other => return Err(Error::Url(format!("unsupported scheme `{other}` in {raw}"))),
        }
        Ok(Self {
            http: reqwest::Client::builder()
                .tls_certs_only(mozilla_roots())
                .build()?,
            private_roots: Vec::new(),
            base: raw.trim_end_matches('/').to_string(),
            auth: None,
            index_prefix: String::new(),
            fence: FenceConfig::default(),
            after: None,
        })
    }

    /// Attach HTTP basic-auth credentials, applied to every request.
    #[must_use]
    pub fn basic_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.auth = Some((username.into(), password.into()));
        self
    }

    /// Also trust the CA certificates in the PEM bundle at `path`, for a
    /// cluster (or a flusso fence endpoint) behind a private CA. They are added
    /// to the Mozilla roots; the OS trust store stays unread. The file is read
    /// here, so a missing or broken one fails now, not at the first request.
    ///
    /// ```no_run
    /// # fn main() -> flusso_query::Result<()> {
    /// let client = flusso_query::Client::connect("https://search.internal:9200")?
    ///     .ca_file("/etc/flusso/opensearch-ca.pem")?;
    /// # Ok(()) }
    /// ```
    pub fn ca_file(self, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let pem = std::fs::read(path)
            .map_err(|error| Error::Ca(format!("reading {}: {error}", path.display())))?;
        self.trust(&pem)
            .map_err(|reason| Error::Ca(format!("{}: {reason}", path.display())))
    }

    /// Also trust the CA certificates in `pem`, a PEM bundle (one or more
    /// `-----BEGIN CERTIFICATE-----` blocks). Like [`Client::ca_file`], from
    /// bytes already in memory. Fails when `pem` holds no certificate or one
    /// does not parse.
    pub fn ca_pem(self, pem: impl AsRef<[u8]>) -> Result<Self> {
        self.trust(pem.as_ref()).map_err(Error::Ca)
    }

    fn trust(mut self, pem: &[u8]) -> std::result::Result<Self, String> {
        let certificates =
            reqwest::Certificate::from_pem_bundle(pem).map_err(|error| chain(&error))?;
        if certificates.is_empty() {
            return Err("no PEM certificate (`-----BEGIN CERTIFICATE-----`) found".to_owned());
        }
        self.private_roots.extend(certificates);
        self.http = reqwest::Client::builder()
            .tls_certs_only(mozilla_roots().chain(self.private_roots.iter().cloned()))
            .build()
            .map_err(|error| chain(&error))?;
        Ok(self)
    }

    /// Set the literal index prefix prepended to every index this client
    /// addresses. Use it to read a prefixed deployment — pass the same prefix
    /// the writing flusso instance runs with (typically from
    /// `FLUSSO_INDEX_PREFIX`). Empty (the default) addresses unprefixed indexes.
    #[must_use]
    pub fn index_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.index_prefix = prefix.into();
        self
    }

    /// Apply the configured index prefix to a request path: one physical index
    /// or a comma-joined list of them (combined search), prefixing each segment.
    /// A no-op when no prefix is set.
    pub(crate) fn prefixed(&self, path: &str) -> String {
        if self.index_prefix.is_empty() {
            return path.to_owned();
        }
        path.split(',')
            .map(|segment| format!("{}{segment}", self.index_prefix))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Apply auth to a request builder, if configured.
    fn authed(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            Some((user, pass)) => builder.basic_auth(user, Some(pass)),
            None => builder,
        }
    }

    /// POST a search body to `{path}/_search` and return the parsed response
    /// JSON. `path` is one physical index (`users_<hash>`) or a comma-joined
    /// list of them (combined search). Crate-internal: [`crate::Search::send`]
    /// and [`crate::MultiSearch::send`] drive this.
    #[tracing::instrument(
        name = "search.request",
        level = "debug",
        skip_all,
        fields(path, status = tracing::field::Empty),
        err,
    )]
    pub(crate) async fn search_at(&self, path: &str, body: &Value) -> Result<Value> {
        self.fence_before(Level::Searchable).await?;
        let endpoint = format!("{}/{}/_search", self.base, self.prefixed(path));
        tracing::debug!(%endpoint, query = %body, "POST _search");
        self.post_json(&endpoint, body).await
    }

    /// POST a query body to `{path}/_count` and return the parsed response
    /// JSON. `path` as in [`search_at`](Self::search_at). Crate-internal:
    /// [`crate::Search::count`] and [`crate::MultiSearch::count`] drive this.
    #[tracing::instrument(
        name = "count.request",
        level = "debug",
        skip_all,
        fields(path, status = tracing::field::Empty),
        err,
    )]
    pub(crate) async fn count_at(&self, path: &str, body: &Value) -> Result<Value> {
        self.fence_before(Level::Searchable).await?;
        let endpoint = format!("{}/{}/_count", self.base, self.prefixed(path));
        tracing::debug!(%endpoint, query = %body, "POST _count");
        self.post_json(&endpoint, body).await
    }

    /// Open a point-in-time over `path` (one physical index or a comma-joined
    /// list, prefixed here) and return its id. Crate-internal: the first page
    /// of a cursor walk drives this.
    #[tracing::instrument(
        name = "pit.open",
        level = "debug",
        skip_all,
        fields(path, status = tracing::field::Empty),
        err,
    )]
    pub(crate) async fn open_pit(&self, path: &str, keep_alive: Duration) -> Result<String> {
        self.fence_before(Level::Searchable).await?;
        let endpoint = format!(
            "{}/{}/_search/point_in_time?keep_alive={}",
            self.base,
            self.prefixed(path),
            crate::cursor::keep_alive_param(keep_alive),
        );
        tracing::debug!(%endpoint, "POST point_in_time");
        let response = self.execute_json(self.http.post(&endpoint)).await?;
        let opened: OpenedPit = serde_json::from_value(response)?;
        Ok(opened.pit_id)
    }

    /// POST a point-in-time search to `/_search` (no index path: the PIT names
    /// the indexes, and OpenSearch refuses both). A gone point-in-time — expired,
    /// closed, or its generation dropped by a reindex — is
    /// [`Error::CursorExpired`].
    #[tracing::instrument(
        name = "pit.search",
        level = "debug",
        skip_all,
        fields(status = tracing::field::Empty),
        err,
    )]
    pub(crate) async fn search_pit(&self, body: &Value) -> Result<Value> {
        self.fence_before(Level::Searchable).await?;
        let endpoint = format!("{}/_search", self.base);
        tracing::debug!(%endpoint, query = %body, "POST _search (point in time)");
        self.post_json(&endpoint, body)
            .await
            .map_err(|error| match error {
                Error::Status { body, .. } if pit_gone(&body) => Error::CursorExpired { body },
                other => other,
            })
    }

    /// Which concrete index each alias on `path` (prefixed here) points at
    /// now, keyed by the prefixed alias name: `users_x` → `users_x_2`. A live
    /// union walk reads it to follow a reindex.
    #[tracing::instrument(
        name = "alias.current",
        level = "debug",
        skip_all,
        fields(path, status = tracing::field::Empty),
        err,
    )]
    pub(crate) async fn current_generations(&self, path: &str) -> Result<HashMap<String, String>> {
        let endpoint = format!("{}/{}/_alias", self.base, self.prefixed(path));
        tracing::debug!(%endpoint, "GET _alias");
        let response = self.execute_json(self.http.get(&endpoint)).await?;
        let indexes: HashMap<String, IndexAliases> = serde_json::from_value(response)?;
        Ok(indexes
            .into_iter()
            .flat_map(|(concrete, entry)| {
                entry
                    .aliases
                    .into_keys()
                    .map(move |alias| (alias, concrete.clone()))
            })
            .collect())
    }

    /// Close a point-in-time. Closing one that already expired is not an
    /// error.
    #[tracing::instrument(
        name = "pit.close",
        level = "debug",
        skip_all,
        fields(status = tracing::field::Empty),
        err,
    )]
    pub(crate) async fn close_pit(&self, pit: &str) -> Result<()> {
        let endpoint = format!("{}/_search/point_in_time", self.base);
        let body = serde_json::json!({ "pit_id": [pit] });
        match self
            .execute_json(self.http.delete(&endpoint).json(&body))
            .await
        {
            Ok(_) => Ok(()),
            Err(Error::Status { status, body })
                if status == StatusCode::NOT_FOUND.as_u16() || pit_gone(&body) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// POST an NDJSON body to `/_msearch` and return the parsed response JSON
    /// (the `{"responses": […]}` envelope). Crate-internal:
    /// [`Client::msearch`](Self::msearch) drives this.
    #[tracing::instrument(
        name = "msearch.request",
        level = "debug",
        skip_all,
        fields(bytes = ndjson.len(), status = tracing::field::Empty),
        err,
    )]
    pub(crate) async fn msearch_raw(&self, ndjson: String) -> Result<Value> {
        self.fence_before(Level::Searchable).await?;
        let endpoint = format!("{}/_msearch", self.base);
        tracing::debug!(%endpoint, query = %ndjson, "POST _msearch");
        let builder = self
            .http
            .post(&endpoint)
            .header("Content-Type", "application/x-ndjson")
            .body(ndjson);
        self.execute_json(builder).await
    }

    /// POST a JSON body, require a 2xx status (recorded on the current span),
    /// and parse the response as JSON.
    async fn post_json(&self, endpoint: &str, body: &Value) -> Result<Value> {
        self.execute_json(self.http.post(endpoint).json(body)).await
    }

    /// Send a prepared request (with auth applied), require a 2xx status
    /// (recorded on the current span), and parse the response as JSON.
    async fn execute_json(&self, builder: reqwest::RequestBuilder) -> Result<Value> {
        let response = self.authed(builder).send().await?;
        let status = response.status();
        tracing::Span::current().record("status", status.as_u16());
        if !status.is_success() {
            return Err(Error::Status {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            });
        }
        Ok(response.json::<Value>().await?)
    }

    /// Fetch a single document by id from `<index>_<hash>/_doc/<id>`. Returns `None`
    /// when the document does not exist.
    ///
    /// Until the derive generates `Type::get`, callers invoke this directly with
    /// the document type as `T`.
    #[tracing::instrument(
        name = "search.get",
        level = "debug",
        skip_all,
        fields(index, hash, id = %id, status = tracing::field::Empty),
        err,
    )]
    pub async fn get_one<T>(
        &self,
        index: &str,
        hash: &str,
        id: impl std::fmt::Display,
    ) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.fence_before(Level::Applied).await?;
        let endpoint = format!(
            "{}/{}/_doc/{id}",
            self.base,
            self.prefixed(&format!("{index}_{hash}"))
        );
        tracing::debug!(%endpoint, "GET _doc");
        let response = self.authed(self.http.get(&endpoint)).send().await?;
        let status = response.status();
        tracing::Span::current().record("status", status.as_u16());
        if status == StatusCode::NOT_FOUND {
            tracing::debug!(found = false, "GET _doc resolved");
            return Ok(None);
        }
        if !status.is_success() {
            return Err(Error::Status {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            });
        }
        let doc: GetResponse<T> = response.json().await?;
        match (doc.found, doc.source) {
            (true, Some(source)) => {
                tracing::debug!(found = true, "GET _doc resolved");
                Ok(Some(source))
            }
            _ => {
                tracing::debug!(found = false, "GET _doc resolved");
                Ok(None)
            }
        }
    }
}

/// Whether an OpenSearch error says a point-in-time is gone: its search
/// context expired or was freed, or the index it pinned was deleted. Matched on
/// the exception type, not the status, so an unrelated 404 (a wrong base URL, a
/// proxy) stays a plain [`Error::Status`].
fn pit_gone(body: &str) -> bool {
    body.contains("search_context_missing_exception") || body.contains("index_not_found_exception")
}

#[derive(Deserialize)]
struct IndexAliases {
    #[serde(default)]
    aliases: HashMap<String, serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct OpenedPit {
    pit_id: String,
}

#[derive(Deserialize)]
struct GetResponse<T> {
    #[serde(default)]
    found: bool,
    #[serde(rename = "_source", default = "none")]
    source: Option<T>,
}

/// `Option::None` without requiring `T: Default` — which `#[serde(default)]`
/// would, but a missing `_source` should just be absent for any `T`.
fn none<T>() -> Option<T> {
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn refused(result: Result<Client>) -> String {
        match result {
            Err(Error::Ca(reason)) => reason,
            other => panic!("expected a CA error, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_ca_file_fails_when_it_is_added() {
        let reason = refused(
            Client::connect("https://localhost:9200")
                .unwrap()
                .ca_file("/no/such/flusso-ca.pem"),
        );
        assert!(reason.contains("/no/such/flusso-ca.pem"), "{reason}");
    }

    #[test]
    fn a_bundle_without_a_certificate_is_refused() {
        let reason = refused(
            Client::connect("https://localhost:9200")
                .unwrap()
                .ca_pem("not a certificate"),
        );
        assert!(reason.contains("no PEM certificate"), "{reason}");
    }

    #[test]
    fn a_certificate_that_does_not_parse_is_refused() {
        let broken = "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n";
        refused(
            Client::connect("https://localhost:9200")
                .unwrap()
                .ca_pem(broken),
        );
    }
}
