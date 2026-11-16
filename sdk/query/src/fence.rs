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
//! # Riding out a restart
//!
//! flusso runs as one process, so a deploy leaves a gap where `/wait` is
//! refused or answers `503`. A fence keeps asking until its deadline
//! ([`Client::fence_timeout`], 30 s by default), counted once across every
//! attempt:
//!
//! - `200`: done.
//! - `408`: flusso's own wait ended (it caps one at 60 s); ask again with what
//!   is left, at once.
//! - `503`, a refused, reset or dropped connection: flusso is unavailable;
//!   back off (100 ms doubling to 2 s, or the `Retry-After` it sent) and ask
//!   again. A `Retry-After` past the deadline fails at once.
//! - `400`, `404`: refused, never retried.
//!
//! The source position is the database's own, so the next flusso process
//! answers the same question. Dropping the read's future stops the loop.
//!
//! A fence never falls back to a stale read. When the deadline passes, the
//! last answer decides the error: [`Error::FenceTimeout`] if flusso was
//! waiting on the position, [`Error::FenceUnavailable`] if it could not be
//! asked.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use tokio::time::Instant;
use url::Url;

use crate::client::Client;
use crate::error::{Error, Result};

/// How long a fence waits unless [`Client::fence_timeout`] says otherwise:
/// long enough to ride out a restart of flusso.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Slack on top of the wait for the daemon's answer to travel back.
const RESPONSE_SLACK: Duration = Duration::from_secs(2);

/// The first pause after flusso could not be asked; it doubles up to
/// [`MAX_BACKOFF`].
const FIRST_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(2);

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

/// What one `/wait` request came back with, short of a refusal.
enum Attempt {
    Reached,
    /// flusso waited and the position was not readable yet.
    TimedOut {
        reached: Option<String>,
    },
    /// flusso could not be asked, or was not ready to answer.
    Unavailable {
        reason: String,
        retry_after: Option<Duration>,
    },
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

    /// The whole budget of one fence (default 30 s), counted once across
    /// every request it makes. Within it a fence rides out a restart of
    /// flusso, and asks again past the daemon's 60 s cap on a single wait.
    /// Past it the read fails with [`Error::FenceTimeout`] or
    /// [`Error::FenceUnavailable`].
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
        fields(position, until = level.as_str(), attempts = tracing::field::Empty),
        err,
    )]
    async fn wait_for(&self, position: &str, level: Level) -> Result<()> {
        let Some(endpoint) = &self.fence.endpoint else {
            return Err(Error::FenceNotConfigured);
        };
        let deadline = Instant::now() + self.fence.timeout;
        let mut backoff = FIRST_BACKOFF;
        let mut attempts = 0_u32;
        loop {
            attempts += 1;
            tracing::Span::current().record("attempts", attempts);
            let attempt = self.ask(endpoint, position, level, left(deadline)).await?;
            let left = left(deadline);
            match attempt {
                Attempt::Reached => return Ok(()),
                Attempt::TimedOut { reached } => {
                    if left.is_zero() {
                        return Err(Error::FenceTimeout { reached });
                    }
                    backoff = FIRST_BACKOFF;
                }
                Attempt::Unavailable {
                    reason,
                    retry_after,
                } => {
                    let pause = match retry_after {
                        Some(asked) if asked >= left => {
                            return Err(Error::FenceUnavailable(reason));
                        }
                        Some(asked) => asked,
                        None => backoff.min(left),
                    };
                    tracing::debug!(%reason, ?pause, "flusso is unavailable; asking again");
                    tokio::time::sleep(pause).await;
                    if Instant::now() >= deadline {
                        return Err(Error::FenceUnavailable(reason));
                    }
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    }

    /// One `GET /wait`, asking flusso to wait at most `left`. A refusal is the
    /// `Err`; everything else is for the caller's loop to decide.
    async fn ask(
        &self,
        endpoint: &str,
        position: &str,
        level: Level,
        left: Duration,
    ) -> Result<Attempt> {
        let mut url = Url::parse(&format!("{endpoint}/wait"))
            .map_err(|error| Error::Url(format!("{endpoint}: {error}")))?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("position", position)
                .append_pair("until", level.as_str())
                .append_pair("timeout_ms", &left.as_millis().to_string());
            if let Some(sink) = &self.fence.sink {
                query.append_pair("sink", sink);
            }
        }
        tracing::debug!(%url, "GET /wait");
        let response = match self
            .http
            .get(url)
            .timeout(left + RESPONSE_SLACK)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return Ok(Attempt::Unavailable {
                    reason: format!("flusso is unreachable: {}", chain(&error)),
                    retry_after: None,
                });
            }
        };
        let status = response.status();
        if status.is_success() {
            return Ok(Attempt::Reached);
        }
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let text = match response.text().await {
            Ok(text) => text,
            Err(error) => {
                return Ok(Attempt::Unavailable {
                    reason: format!("flusso dropped the connection: {}", chain(&error)),
                    retry_after: None,
                });
            }
        };
        let body: Option<WaitBody> = serde_json::from_str(&text).ok();
        match status {
            StatusCode::REQUEST_TIMEOUT => Ok(Attempt::TimedOut {
                reached: body.and_then(|body| body.position),
            }),
            StatusCode::SERVICE_UNAVAILABLE => Ok(Attempt::Unavailable {
                reason: body.and_then(|body| body.error).unwrap_or(text),
                retry_after,
            }),
            _ => Err(Error::FenceRefused {
                status: status.as_u16(),
                reason: body.and_then(|body| body.error).unwrap_or(text),
            }),
        }
    }
}

fn left(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// An error and its sources, so "connection refused" survives reqwest's
/// one-line summary.
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}
