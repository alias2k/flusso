//! The `[concurrency]` table: how much the pipeline does at once.

use std::num::NonZeroUsize;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// The most writes a sink may run at once. Each holds a delivered batch and
/// a connection, and on OpenSearch the gain stops by 2 to 4 (8 measured
/// slower), so the cap only stops a typo from holding hundreds of batches.
pub const MAX_WRITES: usize = 64;

/// How much the pipeline does at once, from a `flusso.toml` `[concurrency]`
/// table. A property of the machine that runs flusso, not of the deployment:
/// it is never written into `flusso.lock`, so `run --locked` takes it from
/// `--writes` / `FLUSSO_WRITES` or the default. Every key is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Concurrency {
    /// Bulk writes each sink runs at once (default 4). Two writes never hold
    /// the same document, and live changes go before backfill pages. `1`
    /// writes one lane item at a time. At most 64.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "capped_writes"
    )]
    #[schemars(range(min = 1, max = 64))]
    pub writes: Option<NonZeroUsize>,
}

/// `writes`, refused past [`MAX_WRITES`].
fn capped_writes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NonZeroUsize>, D::Error> {
    let writes = Option::<NonZeroUsize>::deserialize(deserializer)?;
    match writes {
        Some(writes) if writes.get() > MAX_WRITES => Err(serde::de::Error::custom(format!(
            "writes = {writes} is past the cap of {MAX_WRITES} writes per sink"
        ))),
        writes => Ok(writes),
    }
}

/// `writes` from a flag or an env var, refused past [`MAX_WRITES`] as the
/// file's key is.
pub fn parse_writes(text: &str) -> Result<NonZeroUsize, String> {
    let writes: NonZeroUsize = text
        .parse()
        .map_err(|_| format!("`{text}` is not a whole number of writes, at least 1"))?;
    if writes.get() > MAX_WRITES {
        return Err(format!(
            "{writes} is past the cap of {MAX_WRITES} writes per sink"
        ));
    }
    Ok(writes)
}

impl Concurrency {
    /// True when no key is set, so serialization can omit the table.
    pub fn is_empty(&self) -> bool {
        self.writes.is_none()
    }
}
