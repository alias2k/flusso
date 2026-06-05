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

    /// A response could not be decoded into the expected shape.
    #[error("decoding response: {0}")]
    Decode(#[from] serde_json::Error),

    /// A response was valid JSON but did not have the structure we expect from
    /// the OpenSearch search/get APIs.
    #[error("unexpected response shape: {0}")]
    Shape(String),
}

/// A `Result` whose error is this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
