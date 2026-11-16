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
//! ([`Client::fence_timeout`], 20 s by default), counted once across every
//! attempt, and returns by it:
//!
//! | Answer | Then |
//! | --- | --- |
//! | `200` | done |
//! | `408` | flusso's own wait ended (it caps one at 60 s): ask again with what is left, at once if flusso held the request about as long as asked, else after a pause |
//! | `503`, `502`, `504`, `429`, a refused, reset or dropped connection | flusso is unavailable; pause, ask again |
//! | `400`, `404`, anything else | [`Error::FenceRefused`], at once |
//! | a refused certificate, a redirect loop | [`Error::FenceMisconfigured`], at once |
//!
//! A pause is the backoff (100 ms doubling to 2 s) or the `Retry-After` flusso
//! sent, whichever is longer, plus up to half again at random, so clients that
//! lost flusso at the same instant don't come back in step. A `Retry-After`
//! past the deadline fails at once. Each request asks flusso to wait a little
//! less than the time left, so its `408` arrives before the deadline.
//!
//! After an ordinary restart the next flusso process answers the same
//! question: the position is the database's own. Dropping the read's future
//! stops the loop.
//!
//! A fence never falls back to a stale read. When the deadline passes, the
//! last answer decides the error: [`Error::FenceTimeout`] if flusso was
//! waiting on the position, [`Error::FenceUnavailable`] if it could not be
//! asked.

use std::future::Future;
use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use tokio::time::Instant;
use url::Url;

use crate::client::Client;
use crate::error::{Error, Result, UnavailableCause};

/// How long a fence waits unless [`Client::fence_timeout`] says otherwise:
/// above a typical restart of flusso, below common gateway timeouts.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// How much sooner than the time left flusso is asked to answer, so its
/// `408` travels back before the deadline (a quarter of a short timeout).
const RESPONSE_MARGIN: Duration = Duration::from_millis(250);

/// Less time left than this is no time left: never ask flusso to wait 0 ms.
const MIN_LEFT: Duration = Duration::from_millis(10);

/// The first pause after an answer that wasn't `200`; it doubles up to
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

/// What one `/wait` request came back with, short of an error that ends the
/// fence at once.
#[derive(Debug)]
pub(crate) enum Attempt {
    Reached,
    /// flusso waited and the position was not readable yet.
    TimedOut {
        reached: Option<String>,
    },
    /// flusso could not be asked, or was not ready to answer.
    Unavailable {
        cause: UnavailableCause,
        reason: String,
        retry_after: Option<Duration>,
    },
}

/// One request's limits: how long flusso is asked to wait, and how long the
/// whole request (connect included) may take.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ask {
    pub(crate) wait: Duration,
    pub(crate) budget: Duration,
}

#[derive(Deserialize)]
struct WaitBody {
    #[serde(default)]
    position: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    reason: Option<String>,
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

    /// The whole budget of one fence (default 20 s), counted once across
    /// every request it makes; the fence returns by it. Within it a fence
    /// rides out a restart of flusso, and asks again past the daemon's 60 s
    /// cap on a single wait. Past it the read fails with
    /// [`Error::FenceTimeout`] or [`Error::FenceUnavailable`]. Size it above a
    /// typical restart and below the caller's own request timeout.
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
        fields(position, until = level.as_str()),
        err,
    )]
    async fn wait_for(&self, position: &str, level: Level) -> Result<()> {
        let Some(endpoint) = &self.fence.endpoint else {
            return Err(Error::FenceMisconfigured(
                "no flusso endpoint configured; call Client::flusso(url) first".to_owned(),
            ));
        };
        run(self.fence.timeout, Some(random_seed()), |ask| {
            self.ask(endpoint, position, level, ask)
        })
        .await
    }

    /// One `GET /wait`. An error that ends the fence at once is the `Err`;
    /// everything else is for [`run`] to decide.
    async fn ask(&self, endpoint: &str, position: &str, level: Level, ask: Ask) -> Result<Attempt> {
        let mut url = Url::parse(&format!("{endpoint}/wait"))
            .map_err(|error| Error::FenceMisconfigured(format!("{endpoint}: {error}")))?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("position", position)
                .append_pair("until", level.as_str())
                .append_pair("timeout_ms", &ceil_millis(ask.wait).to_string());
            if let Some(sink) = &self.fence.sink {
                query.append_pair("sink", sink);
            }
        }
        tracing::debug!(%url, "GET /wait");
        let response = match self.http.get(url).timeout(ask.budget).send().await {
            Ok(response) => response,
            Err(error) => return transport_failure(&error),
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
        let body = response.text().await;
        classify(status, retry_after, body)
    }
}

/// Ask until reached, refused, or out of time. `ask` makes one request within
/// the limits it is handed; `seed` drives the jitter (`None`: no jitter).
pub(crate) async fn run<F, Fut>(timeout: Duration, seed: Option<u64>, mut ask: F) -> Result<()>
where
    F: FnMut(Ask) -> Fut,
    Fut: Future<Output = Result<Attempt>>,
{
    let deadline = Instant::now() + timeout;
    let margin = RESPONSE_MARGIN.min(timeout / 4);
    let mut jitter = seed;
    let mut backoff = FIRST_BACKOFF;
    let mut attempts = 0_u32;
    loop {
        attempts += 1;
        let budget = left(deadline).max(MIN_LEFT);
        let wait = budget.saturating_sub(margin).max(Duration::from_millis(1));
        let asked_at = Instant::now();
        let attempt = ask(Ask { wait, budget }).await?;
        let left = left(deadline);
        let (failure, hint) = match attempt {
            Attempt::Reached => return Ok(()),
            Attempt::TimedOut { reached } => {
                let failure = Error::FenceTimeout { reached };
                if left < (margin * 2).max(MIN_LEFT) {
                    return Err(failure);
                }
                if held_as_asked(asked_at.elapsed(), wait) {
                    continue;
                }
                (failure, None)
            }
            Attempt::Unavailable {
                cause,
                reason,
                retry_after,
            } => {
                let failure = Error::FenceUnavailable { cause, reason };
                if retry_after.is_some_and(|hint| hint >= left) {
                    return Err(failure);
                }
                (failure, retry_after)
            }
        };
        let pause = jittered(backoff.max(hint.unwrap_or_default()), &mut jitter);
        tracing::debug!(attempts, error = %failure, ?pause, "fence not passed; asking again");
        if pause + MIN_LEFT >= left {
            tokio::time::sleep(left).await;
            return Err(failure);
        }
        tokio::time::sleep(pause).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// What a non-`200` answer means. The status decides the kind; a body that
/// could not be read only loses the details.
fn classify(
    status: StatusCode,
    retry_after: Option<Duration>,
    body: reqwest::Result<String>,
) -> Result<Attempt> {
    let (text, lost) = match body {
        Ok(text) => (text, None),
        Err(error) => (String::new(), Some(chain(&error))),
    };
    let parsed: Option<WaitBody> = serde_json::from_str(&text).ok();
    let said = parsed
        .as_ref()
        .and_then(|body| body.error.clone())
        .filter(|error| !error.is_empty());
    let unreachable = || match &lost {
        Some(lost) => format!("flusso dropped the connection: {lost}"),
        None => gateway(status, &text),
    };
    let unavailable = |cause, reason: String| {
        Ok(Attempt::Unavailable {
            cause,
            reason,
            retry_after,
        })
    };
    match status {
        StatusCode::REQUEST_TIMEOUT => Ok(Attempt::TimedOut {
            reached: parsed.and_then(|body| body.position),
        }),
        StatusCode::SERVICE_UNAVAILABLE => match (parsed.as_ref(), said) {
            (Some(answer), Some(error)) => {
                let cause = match answer.reason.as_deref() {
                    Some("stopping") => UnavailableCause::Stopping,
                    Some("starting") | None => UnavailableCause::Starting,
                    Some("rebuilding") => UnavailableCause::Rebuilding,
                    Some(_) => UnavailableCause::Unusable,
                };
                unavailable(cause, error)
            }
            _ => unavailable(UnavailableCause::Unreachable, unreachable()),
        },
        StatusCode::BAD_GATEWAY | StatusCode::GATEWAY_TIMEOUT => {
            unavailable(UnavailableCause::Unreachable, unreachable())
        }
        StatusCode::TOO_MANY_REQUESTS => unavailable(
            UnavailableCause::Throttled,
            said.unwrap_or_else(unreachable),
        ),
        _ => Err(Error::FenceRefused {
            status: status.as_u16(),
            reason: said.unwrap_or_else(|| match &lost {
                Some(lost) => format!("(the body was lost: {lost})"),
                None => text,
            }),
        }),
    }
}

/// The reason for an answer that isn't flusso's own JSON.
fn gateway(status: StatusCode, text: &str) -> String {
    let excerpt: String = text.trim().chars().take(200).collect();
    if excerpt.is_empty() {
        format!("answered {status} with no body")
    } else {
        format!("answered {status}: {excerpt}")
    }
}

/// A request that got no answer: retried, unless asking again cannot help.
fn transport_failure(error: &reqwest::Error) -> Result<Attempt> {
    if error.is_redirect() || error.is_builder() || is_certificate_error(error) {
        return Err(Error::FenceMisconfigured(chain(error)));
    }
    let reason = if error.is_timeout() {
        format!("flusso did not answer in time: {}", chain(error))
    } else {
        format!("flusso is unreachable: {}", chain(error))
    };
    Ok(Attempt::Unavailable {
        cause: UnavailableCause::Unreachable,
        reason,
        retry_after: None,
    })
}

/// Whether `error` is a certificate this client refuses, or a server that
/// demanded one and got none. Typed, not by message.
fn is_certificate_error(error: &reqwest::Error) -> bool {
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if matches!(
            rustls_error(cause),
            Some(rustls::Error::InvalidCertificate(_) | rustls::Error::NoCertificatesPresented)
        ) {
            return true;
        }
        source = cause.source();
    }
    false
}

/// The rustls error `cause` is or wraps. tokio-rustls hands it up inside an
/// `io::Error`, which hyper may wrap in another, and `io::Error::source` skips
/// the error it wraps, so each one is opened with `get_ref`.
fn rustls_error<'a>(cause: &'a (dyn std::error::Error + 'static)) -> Option<&'a rustls::Error> {
    if let Some(tls) = cause.downcast_ref::<rustls::Error>() {
        return Some(tls);
    }
    let inner = cause.downcast_ref::<std::io::Error>()?.get_ref()?;
    rustls_error(inner)
}

/// Whether flusso held a request about as long as it was asked to: a `408`
/// after that is the end of an honest wait, asked again at once. Any sooner
/// (a proxy, a daemon answering at once) is paced like an unavailable answer.
fn held_as_asked(held: Duration, asked: Duration) -> bool {
    held >= asked.mul_f64(0.9)
}

fn left(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// Whole milliseconds, rounded up, so flusso never waits less than asked.
fn ceil_millis(duration: Duration) -> u128 {
    duration.as_nanos().div_ceil(1_000_000)
}

/// `base` plus up to half of it again, drawn from `state` (splitmix64); `base`
/// as is without a state.
fn jittered(base: Duration, state: &mut Option<u64>) -> Duration {
    let Some(state) = state else {
        return base;
    };
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    #[allow(clippy::cast_precision_loss)]
    let fraction = (z >> 11) as f64 / (1_u64 << 53) as f64;
    base + base.mul_f64(fraction / 2.0)
}

/// A seed no two clients share: std's per-process random keys, mixed per call.
fn random_seed() -> u64 {
    RandomState::new().hash_one(std::time::SystemTime::now())
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
