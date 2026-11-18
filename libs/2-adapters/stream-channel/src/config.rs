//! The in-process stream's own configuration: the `[stream]` table with
//! `type = "channel"` (the default when `[stream]` is omitted).
//!
//! ```
//! use kernel::{AdapterConfig, Options};
//! use stream_channel::ChannelConfig;
//!
//! let config = ChannelConfig::from_options(Options::empty()).unwrap();
//! assert_eq!(config.capacity, None);
//! ```

use kernel::AdapterConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The `[stream]` options for `type = "channel"`.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, AdapterConfig,
)]
#[serde(deny_unknown_fields)]
#[adapter(port = stream, kind = "channel")]
pub struct ChannelConfig {
    /// Deprecated, and ignored: still accepted, with a warning at start.
    /// What the lanes hold is bounded in bytes by `[concurrency]
    /// max_built_mib`, which a sink that stops acknowledging fills, pacing
    /// ingest; each lane keeps a fixed backstop of 1,048,576 items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<usize>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use kernel::Options;

    #[test]
    fn unknown_option_is_rejected() {
        let options: Options = toml::from_str("capacty = 3").unwrap();
        let error = ChannelConfig::from_options(options)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown field `capacty`"), "{error}");
    }

    #[test]
    fn describes_itself() {
        let description = ChannelConfig::description();
        assert_eq!(description.kind, "channel");
        assert!(description.secrets.is_empty());
        assert_eq!(description.example.get("capacity"), None);
    }
}
