//! The in-process stream's own configuration: the `[stream]` table with
//! `type = "channel"` (the default when `[stream]` is omitted).
//!
//! ```
//! use kernel::{AdapterConfig, Options};
//! use stream_channel::ChannelConfig;
//!
//! let config = ChannelConfig::from_options(Options::empty()).unwrap();
//! assert_eq!(config.capacity, 1024);
//! ```

use kernel::AdapterConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The `[stream]` options for `type = "channel"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, AdapterConfig)]
#[serde(deny_unknown_fields)]
#[adapter(port = stream, kind = "channel")]
pub struct ChannelConfig {
    /// Changes buffered between capture and processing before capture waits.
    /// A bounded buffer is what makes the slowest sink pace ingest.
    #[serde(default = "default_capacity")]
    pub capacity: usize,
}

fn default_capacity() -> usize {
    1024
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            capacity: default_capacity(),
        }
    }
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
        assert_eq!(
            description.example.get("capacity").and_then(|v| v.as_i64()),
            Some(1024)
        );
    }
}
