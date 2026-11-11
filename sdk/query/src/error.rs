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

    /// A fence ran out of time: the position was not readable within the
    /// client's [`fence_timeout`](crate::Client::fence_timeout). Nothing was
    /// read. `reached` is how far the sink had got, when flusso reported it.
    #[error("fence timed out before the position was readable (reached {reached:?})")]
    FenceTimeout {
        /// The source position the sink had reached, in the source's text form.
        reached: Option<String>,
    },

    /// A fence could not be asked: no flusso endpoint is configured
    /// ([`Client::flusso`](crate::Client::flusso)), flusso is unreachable, or
    /// it is not following the source yet. Nothing was read and nothing is
    /// retried.
    #[error("fence unavailable: {0}")]
    FenceUnavailable(String),

    /// flusso refused the wait: a position it cannot read, or a sink it does
    /// not have.
    #[error("flusso refused the fence (status {status}): {reason}")]
    FenceRefused {
        /// The HTTP status flusso answered with.
        status: u16,
        /// Why, in flusso's words.
        reason: String,
    },

    /// A response could not be decoded into the expected shape.
    #[error("decoding response: {0}")]
    Decode(#[from] serde_json::Error),
}

/// A `Result` whose error is this crate's [`Error`](enum@Error).
pub type Result<T> = std::result::Result<T, Error>;
