//! The `[backoff]` table: how long flusso waits before trying again.

use std::num::NonZeroU64;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// How long flusso waits before trying again, from a `flusso.toml`
/// `[backoff]` table: a failed engine's restart (sink or ingest), a parked
/// lane's probe, a re-staged lane's next staging. Each wait starts at
/// `initial_ms` and doubles, up to `max_ms`; equal values wait a constant
/// interval. A property of the machine that runs flusso: never written into
/// `flusso.lock`, so `run --locked` takes it from `--backoff-initial-ms` /
/// `FLUSSO_BACKOFF_INITIAL_MS` and `--backoff-max-ms` / `FLUSSO_BACKOFF_MAX_MS`
/// or the defaults (flag > env > file). Every key is optional.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Backoff {
    /// The first wait, in milliseconds (default 1000). At least 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub initial_ms: Option<NonZeroU64>,
    /// The longest wait, in milliseconds (default 60000). At least
    /// `initial_ms`; equal to it, every wait is the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub max_ms: Option<NonZeroU64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBackoff {
    #[serde(default)]
    initial_ms: Option<u64>,
    #[serde(default)]
    max_ms: Option<u64>,
}

impl<'de> Deserialize<'de> for Backoff {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawBackoff::deserialize(deserializer)?;
        let at_least_one = |key: &str, ms: Option<u64>| match ms {
            None => Ok(None),
            Some(ms) => NonZeroU64::new(ms).map(Some).ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "{key} = 0 would try again without waiting; it must be at least 1"
                ))
            }),
        };
        let backoff = Backoff {
            initial_ms: at_least_one("initial_ms", raw.initial_ms)?,
            max_ms: at_least_one("max_ms", raw.max_ms)?,
        };
        if let (Some(initial), Some(max)) = (backoff.initial_ms, backoff.max_ms) {
            check_backoff_order(initial, max).map_err(serde::de::Error::custom)?;
        }
        Ok(backoff)
    }
}

/// `max_ms` must not be below `initial_ms`.
pub fn check_backoff_order(initial_ms: NonZeroU64, max_ms: NonZeroU64) -> Result<(), String> {
    if max_ms < initial_ms {
        return Err(format!(
            "max_ms = {max_ms} is below initial_ms = {initial_ms}; \
             set max_ms to at least initial_ms (equal values wait a constant interval)"
        ));
    }
    Ok(())
}

/// `initial_ms` or `max_ms` from a flag or an env var: a whole number of
/// milliseconds, at least 1.
pub fn parse_backoff_ms(text: &str) -> Result<NonZeroU64, String> {
    text.parse()
        .map_err(|_| format!("`{text}` is not a whole number of milliseconds, at least 1"))
}

impl Backoff {
    /// True when no key is set, so serialization can omit the table.
    pub fn is_empty(&self) -> bool {
        self.initial_ms.is_none() && self.max_ms.is_none()
    }
}
