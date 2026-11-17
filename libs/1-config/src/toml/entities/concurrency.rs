//! The `[concurrency]` table: how much the pipeline does at once.

use std::num::NonZeroUsize;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How much the pipeline does at once, from a `flusso.toml` `[concurrency]`
/// table. A property of the machine that runs flusso, not of the deployment:
/// it is never written into `flusso.lock`, so `run --locked` takes it from
/// `--writes` / `FLUSSO_WRITES` or the default. Every key is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Concurrency {
    /// Bulk writes each sink runs at once (default 4). Two writes never hold
    /// the same document, and live changes go before backfill pages. `1`
    /// writes one lane item at a time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writes: Option<NonZeroUsize>,
}

impl Concurrency {
    /// True when no key is set, so serialization can omit the table.
    pub fn is_empty(&self) -> bool {
        self.writes.is_none()
    }
}
