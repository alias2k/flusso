//! Reading your own writes: wait on flusso until a commit is readable, then
//! read.
//!
//! OpenSearch is fed asynchronously, so a read right after a database commit
//! can miss it. A **fence** closes that gap. After the commit, read the
//! source's position ([`crate::position`]), then read through
//! [`Client::after`]: each read first asks the flusso daemon to hold the
//! request until that position is readable, at the level the read needs.
//!
//! - a fetch by id ([`FlussoRoot::get`](crate::FlussoRoot::get)) waits until
//!   the write is **applied**;
//! - a search, count, or multi-search waits until it is **searchable**, which
//!   can trail by the index's refresh interval while a backlog drains.
//!
//! The daemon only waits; it never refreshes on a client's behalf. Reads still
//! go straight to OpenSearch, so an unfenced read never depends on flusso.
//!
//! ```no_run
//! # async fn demo(position: String) -> flusso_query::Result<()> {
//! use flusso_query::Client;
//!
//! let client = Client::connect("http://localhost:9200")?
//!     .flusso("http://localhost:9464")?;
//!
//! // Wait once, then read however often.
//! client.wait(&position).await?;
//!
//! // Or let each read wait for what it needs.
//! let fresh = client.after(&position);
//! # let _ = fresh;
//! # Ok(())
//! # }
//! ```
//!
//! A fence never falls back to a stale read: when the wait runs out the read
//! fails with [`Error::FenceTimeout`], and when the daemon cannot be asked,
//! with [`Error::FenceUnavailable`].

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;
use url::Url;

use crate::client::Client;
use crate::error::{Error, Result};

/// How long a fence waits unless [`Client::fence_timeout`] says otherwise.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Slack on top of the wait for the daemon's answer to travel back.
const RESPONSE_SLACK: Duration = Duration::from_secs(2);

/// Where and how a client fences.
#[derive(Debug, Clone)]
pub(crate) struct FenceConfig {
    /// The flusso daemon's public address, trailing slash trimmed.
    endpoint: Option<String>,
    /// The sink this client reads from; `None` waits for every sink.
    sink: Option<String>,
    timeout: Duration,
}

impl Default for FenceConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            sink: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

/// How far a read needs the write to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    Applied = 1,
    Searchable = 2,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Applied => "applied",
            Level::Searchable => "searchable",
        }
    }
}

/// The position a fenced client reads after, and the highest level already
/// waited for (shared by its clones, so several reads wait once).
#[derive(Debug)]
pub(crate) struct After {
    position: String,
    passed: AtomicU8,
}

#[derive(Deserialize)]
struct WaitBody {
    #[serde(default)]
    position: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

impl Client {
    /// Point the client at a flusso daemon's public address (the one serving
    /// `/status`), so it can fence. Validates the URL; performs no I/O.
    pub fn flusso(mut self, url: impl AsRef<str>) -> Result<Self> {
        let raw = url.as_ref();
        let parsed = Url::parse(raw).map_err(|error| Error::Url(format!("{raw}: {error}")))?;
        match parsed.scheme() {
            "http" | "https" => {}
            other => return Err(Error::Url(format!("unsupported scheme `{other}` in {raw}"))),
        }
        self.fence.endpoint = Some(raw.trim_end_matches('/').to_owned());
        Ok(self)
    }

    /// Fence on one sink: the `[sinks.<name>]` this client's cluster is fed by.
    /// Without it a fence waits for every sink of the deployment, so a stalled
    /// sibling sink would hold up reads from a healthy one.
    #[must_use]
    pub fn fence_sink(mut self, sink: impl Into<String>) -> Self {
        self.fence.sink = Some(sink.into());
        self
    }

    /// How long a fence waits before failing with [`Error::FenceTimeout`]
    /// (default 5 s; the daemon caps a single wait at 60 s).
    #[must_use]
    pub fn fence_timeout(mut self, timeout: Duration) -> Self {
        self.fence.timeout = timeout;
        self
    }

    /// A client whose reads wait until `position` is readable: a fetch by id
    /// until it is applied, a search until it is searchable. `position` is
    /// what the source reported after the commit (see [`crate::position`]).
    ///
    /// The returned client shares this one's connection pool. Reads through it
    /// (or its clones) wait once per level, not once per request.
    #[must_use]
    pub fn after(&self, position: impl Into<String>) -> Client {
        let mut fenced = self.clone();
        fenced.after = Some(Arc::new(After {
            position: position.into(),
            passed: AtomicU8::new(0),
        }));
        fenced
    }

    /// Wait until `position` is searchable: every read sees the commit.
    pub async fn wait(&self, position: &str) -> Result<()> {
        self.wait_for(position, Level::Searchable).await
    }

    /// Wait until `position` is applied: a fetch by id sees the commit, a
    /// search may not yet. Returns sooner than [`wait`](Self::wait) while the
    /// index has a backlog.
    pub async fn wait_applied(&self, position: &str) -> Result<()> {
        self.wait_for(position, Level::Applied).await
    }

    /// Before a read: if this client reads after a position, wait for the
    /// level the read needs, unless this client already did.
    pub(crate) async fn fence_before(&self, level: Level) -> Result<()> {
        let Some(after) = &self.after else {
            return Ok(());
        };
        if after.passed.load(Ordering::Acquire) >= level as u8 {
            return Ok(());
        }
        self.wait_for(&after.position, level).await?;
        after.passed.fetch_max(level as u8, Ordering::AcqRel);
        Ok(())
    }

    #[tracing::instrument(
        name = "fence.wait",
        level = "debug",
        skip_all,
        fields(position, until = level.as_str(), status = tracing::field::Empty),
        err,
    )]
    async fn wait_for(&self, position: &str, level: Level) -> Result<()> {
        let Some(endpoint) = &self.fence.endpoint else {
            return Err(Error::FenceUnavailable(
                "no flusso endpoint configured; call Client::flusso(url) first".to_owned(),
            ));
        };
        let mut url = Url::parse(&format!("{endpoint}/wait"))
            .map_err(|error| Error::Url(format!("{endpoint}: {error}")))?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("position", position)
                .append_pair("until", level.as_str())
                .append_pair("timeout_ms", &self.fence.timeout.as_millis().to_string());
            if let Some(sink) = &self.fence.sink {
                query.append_pair("sink", sink);
            }
        }
        tracing::debug!(%url, "GET /wait");
        let response = self
            .http
            .get(url)
            .timeout(self.fence.timeout + RESPONSE_SLACK)
            .send()
            .await
            .map_err(|error| Error::FenceUnavailable(format!("flusso is unreachable: {error}")))?;
        let status = response.status();
        tracing::Span::current().record("status", status.as_u16());
        if status.is_success() {
            return Ok(());
        }
        let text = response.text().await.unwrap_or_default();
        let body: Option<WaitBody> = serde_json::from_str(&text).ok();
        match status {
            StatusCode::REQUEST_TIMEOUT => Err(Error::FenceTimeout {
                reached: body.and_then(|body| body.position),
            }),
            StatusCode::SERVICE_UNAVAILABLE => Err(Error::FenceUnavailable(
                body.and_then(|body| body.error).unwrap_or(text),
            )),
            _ => Err(Error::FenceRefused {
                status: status.as_u16(),
                reason: body.and_then(|body| body.error).unwrap_or(text),
            }),
        }
    }
}
