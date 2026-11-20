//! The `[concurrency]` table: how much the pipeline does at once.

use std::num::NonZeroUsize;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// The most writes a sink may run at once. Each holds a delivered batch and
/// a connection, and on OpenSearch the gain stops by 2 to 4 (8 measured
/// slower), so the cap only stops a typo from holding hundreds of batches.
pub const MAX_WRITES: usize = 64;

/// The most document builds the ingest engine may run at once. Each holds a
/// Postgres connection (the source keeps `builds + 2`: `builds + 1` building,
/// one resolving changes), so the cap keeps
/// a typo from opening hundreds.
pub const MAX_BUILDS: usize = 32;

/// The least `max_built_mib` takes: below it a single `users`-like batch
/// (about 4 MiB) fills the budget, and every build waits for the last.
pub const MIN_BUILT_MIB: usize = 16;

/// The most `max_built_mib` takes: a TiB, so a typo can't overflow a count.
pub const MAX_BUILT_MIB: usize = 1 << 20;

/// How much the pipeline does at once, from a `flusso.toml` `[concurrency]`
/// table. A property of the machine that runs flusso, not of the deployment:
/// it is never written into `flusso.lock`, so `run --locked` takes it from
/// `--writes` / `FLUSSO_WRITES` and `--builds` / `FLUSSO_BUILDS` or the
/// defaults. Every key is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// builds one batch at a time. At most 32; the source holds `builds + 2`
    /// connections (`builds + 1` building, one resolving changes).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "capped_builds"
    )]
    #[schemars(range(min = 1, max = 32))]
    pub builds: Option<NonZeroUsize>,
    /// MiB of built documents held before the ingest engine waits (default
    /// 512): what it built and every sink has not yet acknowledged. A
    /// backfill holds at most half of it, so live changes always have room,
    /// and a batch larger than the whole budget still builds, alone. Counts
    /// heap, not JSON: a document holds 8–10× its JSON size. At least 16, at
    /// most 1,048,576.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "bounded_built_mib"
    )]
    #[schemars(range(min = 16, max = 1_048_576))]
    pub max_built_mib: Option<NonZeroUsize>,
    /// The share of `max_built_mib` one index's live documents may hold
    /// while two or more indexes run (default 0.5): one index bursting fills
    /// at most this much, and the others keep building beside it. Its
    /// backfill counts with every backfill against half the budget instead.
    /// Above 0, at most 1; with one index it does nothing. `check`, `build`
    /// and `run` refuse a value outside (0, 1].
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "any_number"
    )]
    #[schemars(extend("exclusiveMinimum" = 0, "maximum" = 1))]
    pub index_budget_share: Option<f64>,
}

/// A number as written, whole or not: the range is the daemon's to judge,
/// with where the value came from.
fn any_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number {
        Whole(i64),
        Fraction(f64),
    }
    Ok(
        Option::<Number>::deserialize(deserializer)?.map(|number| match number {
            Number::Whole(whole) => whole as f64,
            Number::Fraction(fraction) => fraction,
        }),
    )
}

/// `index_budget_share` from a flag or an env var: a number. Its range is
/// judged with the file's value, naming where it came from.
pub fn parse_index_budget_share(text: &str) -> Result<f64, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("`{text}` is not a number; give a share above 0, at most 1"))
}

/// `max_built_mib`, refused outside [`MIN_BUILT_MIB`]..=[`MAX_BUILT_MIB`].
fn bounded_built_mib<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NonZeroUsize>, D::Error> {
    match Option::<NonZeroUsize>::deserialize(deserializer)? {
        Some(mib) => built_mib_in_bounds(mib)
            .map(Some)
            .map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

fn built_mib_in_bounds(mib: NonZeroUsize) -> Result<NonZeroUsize, String> {
    if (MIN_BUILT_MIB..=MAX_BUILT_MIB).contains(&mib.get()) {
        Ok(mib)
    } else {
        Err(format!(
            "max_built_mib = {mib} is outside {MIN_BUILT_MIB}..={MAX_BUILT_MIB} MiB"
        ))
    }
}

/// `max_built_mib` from a flag or an env var, with the file's bounds.
pub fn parse_max_built_mib(text: &str) -> Result<NonZeroUsize, String> {
    let mib: NonZeroUsize = text
        .parse()
        .map_err(|_| format!("`{text}` is not a whole number of MiB, at least 1"))?;
    built_mib_in_bounds(mib)
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
        self.writes.is_none()
            && self.builds.is_none()
            && self.max_built_mib.is_none()
            && self.index_budget_share.is_none()
    }
}
