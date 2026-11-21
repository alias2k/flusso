//! Lifting the parsed `flusso.toml` ([`ConfigToml`]) into the assembled
//! [`Config`].
//!
//! The toml parser ([`crate::toml`]) produces neutral entity types that mirror
//! the file; turning those into a `Config` is a composition step, so it lives
//! here next to `Config` rather than in the parser. The port entries pass
//! through untouched: their options are the adapter's to interpret, in the
//! composition root. An omitted `[stream]` becomes the in-process channel with
//! its defaults. The `index` entries are left empty; the loader fills them in
//! by reading each referenced YAML schema.

use std::collections::BTreeMap;

use kernel::PortEntry;

use crate::toml::ConfigToml;

use super::{
    BackoffConfig, BackoffValue, BatchConfig, ConcurrencyConfig, Config, CountValue,
    DEFAULT_STREAM_KIND, Origin, ServerConfig, ShareValue,
};

/// Infallible (nothing is resolved or interpreted here), so this is a `From`;
/// the blanket impl still gives callers a `TryFrom<ConfigToml>`.
impl From<ConfigToml> for Config {
    fn from(toml: ConfigToml) -> Self {
        Config {
            source: toml.source,
            stream: toml
                .stream
                .unwrap_or_else(|| PortEntry::new(DEFAULT_STREAM_KIND)),
            sinks: toml.sinks,
            indexes: BTreeMap::new(),
            on_error: toml.on_error,
            batch: BatchConfig {
                max_changes: toml.batch.max_changes,
                max_delay_ms: toml.batch.max_delay_ms,
            },
            concurrency: ConcurrencyConfig {
                writes: toml.concurrency.writes,
                builds: toml.concurrency.builds,
                max_built_mib: toml.concurrency.max_built_mib,
                index_budget_share: toml.concurrency.index_budget_share.map(|share| ShareValue {
                    share,
                    origin: Origin::File,
                }),
                max_waiting_per_index: toml
                    .concurrency
                    .max_waiting_per_index
                    .map(CountValue::from_file),
                max_waiting: toml.concurrency.max_waiting.map(CountValue::from_file),
            },
            backoff: BackoffConfig {
                initial_ms: toml.backoff.initial_ms.map(BackoffValue::from_file),
                max_ms: toml.backoff.max_ms.map(BackoffValue::from_file),
            },
            server: ServerConfig {
                public_address: toml.server.public_address,
                private_address: toml.server.private_address,
            },
            prefix: toml.prefix,
        }
    }
}
