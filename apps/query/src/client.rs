//! The [`Client`] transport. Points at OpenSearch (not at flusso — the engine
//! is write-only; reads go straight to the index it maintains).

use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use crate::error::{Error, Result};

/// A connection to an OpenSearch cluster.
///
/// Cheap to clone (it shares one connection pool). Construct with
/// [`Client::connect`], optionally adding credentials with
/// [`Client::basic_auth`].
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    /// Base URL with any trailing slash trimmed.
    base: String,
    auth: Option<(String, String)>,
    /// Literal prefix prepended to every index name a request addresses, so a
    /// consumer can read a prefixed deployment's indexes (`dev_users_<hash>`).
    /// Empty by default; set with [`Client::index_prefix`]. Must match the
    /// `prefix` the writing flusso instance is running with.
    pub(crate) index_prefix: String,
}

impl Client {
    /// Connect to the cluster at `url` (`http` or `https`). This validates the
    /// URL and builds the HTTP client; it does not perform any I/O.
    pub fn connect(url: impl AsRef<str>) -> Result<Self> {
        let raw = url.as_ref();
        let parsed = Url::parse(raw).map_err(|error| Error::Url(format!("{raw}: {error}")))?;
        match parsed.scheme() {
            "http" | "https" => {}
            other => return Err(Error::Url(format!("unsupported scheme `{other}` in {raw}"))),
        }
        let http = reqwest::Client::builder().build()?;
        Ok(Self {
            http,
            base: raw.trim_end_matches('/').to_string(),
            auth: None,
            index_prefix: String::new(),
        })
    }

    /// Attach HTTP basic-auth credentials, applied to every request.
    #[must_use]
    pub fn basic_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.auth = Some((username.into(), password.into()));
        self
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
        let endpoint = format!("{}/{}/_count", self.base, self.prefixed(path));
        tracing::debug!(%endpoint, query = %body, "POST _count");
        self.post_json(&endpoint, body).await
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
            (true, Some(source)) => Ok(Some(source)),
            _ => Ok(None),
        }
    }
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
