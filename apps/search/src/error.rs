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

    /// A response could not be decoded into the expected shape.
    #[error("decoding response: {0}")]
    Decode(#[from] serde_json::Error),
}

/// A `Result` whose error is this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
