//! The `[batch]` table: how the ingest engine groups live changes.

use std::num::NonZeroUsize;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How live changes are grouped into one batch, from a `flusso.toml` `[batch]`
/// table. Both keys are optional; an omitted key keeps the engine's default.
/// `--batch-max-changes` / `--batch-max-delay-ms` (and their `FLUSSO_*` env
/// vars) override them at run time (flag > env > file).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    /// Commit a batch once this many changes have accumulated (default 256).
    /// A batch is also committed the moment the live stream has nothing more
    /// ready, so a lone change never waits for a full batch; this caps a
    /// burst. `1` flushes per change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_changes: Option<NonZeroUsize>,
    /// The longest a batch stays open after its first change while changes
    /// keep arriving, in milliseconds (default 50). Also the window a backfill
    /// snapshot waits for straggling requests from other sinks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_delay_ms: Option<u64>,
}

impl Batch {
    /// True when no key is set, so serialization can omit the whole `[batch]`
    /// table rather than emit an empty one.
    pub fn is_empty(&self) -> bool {
        self.max_changes.is_none() && self.max_delay_ms.is_none()
    }
}
