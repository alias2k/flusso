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
        })
    }

    /// Attach HTTP basic-auth credentials, applied to every request.
    #[must_use]
    pub fn basic_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.auth = Some((username.into(), password.into()));
        self
    }

    /// Apply auth to a request builder, if configured.
    fn authed(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            Some((user, pass)) => builder.basic_auth(user, Some(pass)),
            None => builder,
        }
    }

    /// POST a search body to `<index>_<hash>/_search` and return the parsed response
    /// JSON. Crate-internal: [`crate::Search::send`] drives this.
    #[tracing::instrument(
        name = "search.request",
        level = "debug",
        skip_all,
        fields(index, hash, status = tracing::field::Empty),
        err,
    )]
    pub(crate) async fn search(&self, index: &str, hash: &str, body: &Value) -> Result<Value> {
        let endpoint = format!("{}/{index}_{hash}/_search", self.base);
        tracing::debug!(%endpoint, "POST _search");
        let response = self
            .authed(self.http.post(&endpoint).json(body))
            .send()
            .await?;
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
        let endpoint = format!("{}/{index}_{hash}/_doc/{id}", self.base);
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
