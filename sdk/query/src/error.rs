//! The client's error type.

use thiserror::Error;

/// Anything that can go wrong building a request, talking to OpenSearch, or
/// decoding a response.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The base URL handed to [`crate::Client::connect`] was not a usable
    /// `http`/`https` URL.
    #[error("invalid base url: {0}")]
    Url(String),

    /// A CA certificate handed to [`crate::Client::ca_pem`] or
    /// [`crate::Client::ca_file`] could not be used: the file is unreadable,
    /// holds no PEM certificate, or a certificate in it does not parse.
    #[error("CA certificate: {0}")]
    Ca(String),

    /// The underlying HTTP transport failed (connection, timeout, TLS, …).
    #[error("http transport error: {0}")]
    Http(#[from] reqwest::Error),

    /// OpenSearch answered with a non-success status. `body` is the raw payload,
    /// captured for diagnostics.
    #[error("opensearch returned status {status}: {body}")]
    Status {
        /// The HTTP status code.
        status: u16,
        /// The response body, as text.
        body: String,
    },

    /// One slot of a [`crate::Client::msearch`] bundle failed. `_msearch`
    /// reports errors per slot; the whole call fails on the first one — there
    /// are no partial results.
    #[error("msearch slot {slot} failed (status {status}): {body}")]
    Msearch {
        /// Zero-based position of the failed search in the bundle.
        slot: usize,
        /// The per-slot status OpenSearch reported (`0` if absent).
        status: u16,
        /// The slot's error object, serialized for diagnostics.
        body: String,
    },

    /// A combined-search hit came from an index no
    /// [`FlussoMultiDocument`](crate::FlussoMultiDocument) variant claims.
    #[error("combined-search hit from unexpected index `{index}`")]
    UnexpectedIndex {
        /// The physical index name the hit reported.
        index: String,
    },

    /// A [`PageCursor`](crate::PageCursor) couldn't be used: the token is
    /// malformed, or the search it was handed to also sets `from` or a raw
    /// `search_after` (a cursor owns the page position).
    #[error("page cursor: {0}")]
    Cursor(String),

    /// A [`PageCursor`](crate::PageCursor) was handed back with a different
    /// query, sort, or index set than the one that produced it.
    #[error("page cursor belongs to a different query")]
    CursorMismatch,

    /// A pinned walk's point-in-time is gone: its keep-alive lapsed, it was
    /// closed, or a reindex dropped the generation it pinned. Restart the walk
    /// from page one. A live walk never expires.
    #[error("page cursor expired: {body}")]
    CursorExpired {
        /// OpenSearch's error body, for diagnostics.
        body: String,
    },

    /// A fence ran out of time while flusso was waiting on the position: the
    /// last answer before the [`fence_timeout`](crate::Client::fence_timeout)
    /// passed was "not readable yet". flusso is behind, or a sink is stalled.
    /// Nothing was read. `reached` is how far the sink had got, when flusso
    /// reported it.
    #[error("fence timed out before the position was readable (reached {reached:?})")]
    FenceTimeout {
        /// The source position the sink had reached, in the source's text form.
        reached: Option<String>,
    },

    /// flusso could not be asked until the
    /// [`fence_timeout`](crate::Client::fence_timeout) passed. The fence
    /// retried the whole time; `cause` and `reason` are the last answer's.
    /// Nothing was read.
    #[error("fence unavailable ({cause}): {reason}")]
    FenceUnavailable {
        /// Why, as a category a caller can branch on.
        cause: UnavailableCause,
        /// Why, in words: flusso's own, or the transport's.
        reason: String,
    },

    /// The fence cannot talk to flusso as configured, so it fails at once:
    /// no address ([`Client::flusso`](crate::Client::flusso) was not called),
    /// a certificate the client refuses, or a redirect it cannot follow.
    /// Retrying would get the same answer. Nothing was read.
    #[error("fence misconfigured: {0}")]
    FenceMisconfigured(String),

    /// flusso refused the wait: a position it cannot read, or a sink it does
    /// not have.
    #[error("flusso refused the fence (status {status}): {reason}")]
    FenceRefused {
        /// The HTTP status flusso answered with.
        status: u16,
        /// Why, in flusso's words.
        reason: String,
    },

    /// A read-time field cannot be evaluated: a rule compares a date with
    /// text that is not one, a combination of rule-field sorts cannot run, a
    /// computed field's input does not read from the document. Raised before
    /// the request is sent, or while a fetched document is decoded.
    #[error("read-time field: {0}")]
    ReadTimeField(String),

    /// A response could not be decoded into the expected shape.
    #[error("decoding response: {0}")]
    Decode(#[from] serde_json::Error),
}

/// Why a fence could not ask flusso, in [`Error::FenceUnavailable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnavailableCause {
    /// flusso is stopping (a deploy, a restart).
    Stopping,
    /// flusso is not following the source yet: it is starting, or its ingest
    /// engine is restarting. A daemon that names no reason (0.17) means this.
    Starting,
    /// A sink is rebuilding the index the fence covers, so flusso cannot vouch
    /// for it yet.
    Rebuilding,
    /// flusso could not be reached: refused, reset or dropped connection, no
    /// answer in time, or a gateway in front of it answering `502`/`503`/`504`.
    Unreachable,
    /// Something in front of flusso answered `429 Too Many Requests`.
    Throttled,
    /// flusso answered `503` with a reason this client does not know.
    Unusable,
}

impl std::fmt::Display for UnavailableCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            UnavailableCause::Stopping => "stopping",
            UnavailableCause::Starting => "starting",
            UnavailableCause::Rebuilding => "rebuilding",
            UnavailableCause::Unreachable => "unreachable",
            UnavailableCause::Throttled => "throttled",
            UnavailableCause::Unusable => "unusable answer",
        })
    }
}

/// A `Result` whose error is this crate's [`Error`](enum@Error).
pub type Result<T> = std::result::Result<T, Error>;
