//! The `[concurrency]` table: how much the pipeline does at once.

use std::num::NonZeroUsize;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// The most writes a sink may run at once. Each holds a delivered batch and
/// a connection, and on OpenSearch the gain stops by 2 to 4 (8 measured
/// slower), so the cap only stops a typo from holding hundreds of batches.
pub const MAX_WRITES: usize = 64;

/// The most document builds the ingest engine may run at once. Each holds a
/// Postgres connection (the builder's pool is `builds + 2`), so the cap keeps
/// a typo from opening hundreds.
pub const MAX_BUILDS: usize = 32;

/// How much the pipeline does at once, from a `flusso.toml` `[concurrency]`
/// table. A property of the machine that runs flusso, not of the deployment:
/// it is never written into `flusso.lock`, so `run --locked` takes it from
/// `--writes` / `FLUSSO_WRITES` and `--builds` / `FLUSSO_BUILDS` or the
/// defaults. Every key is optional.
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
    /// Document builds the ingest engine runs at once (default 4). A document
    /// is never built twice at once, and each lane still gets batches in the
    /// order they were started. Live batches go before backfill pages. `1`
    /// builds one batch at a time. At most 32; the source's builder pool
    /// holds `builds + 2` connections.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "capped_builds"
    )]
    #[schemars(range(min = 1, max = 32))]
    pub builds: Option<NonZeroUsize>,
}

/// `builds`, refused past [`MAX_BUILDS`].
fn capped_builds<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NonZeroUsize>, D::Error> {
    let builds = Option::<NonZeroUsize>::deserialize(deserializer)?;
    match builds {
        Some(builds) if builds.get() > MAX_BUILDS => Err(serde::de::Error::custom(format!(
            "builds = {builds} is past the cap of {MAX_BUILDS} builds at once"
        ))),
        builds => Ok(builds),
    }
}

/// `builds` from a flag or an env var, refused past [`MAX_BUILDS`] as the
/// file's key is.
pub fn parse_builds(text: &str) -> Result<NonZeroUsize, String> {
    let builds: NonZeroUsize = text
        .parse()
        .map_err(|_| format!("`{text}` is not a whole number of builds, at least 1"))?;
    if builds.get() > MAX_BUILDS {
        return Err(format!(
            "{builds} is past the cap of {MAX_BUILDS} builds at once"
        ));
    }
    Ok(builds)
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
        self.writes.is_none() && self.builds.is_none()
    }
}
