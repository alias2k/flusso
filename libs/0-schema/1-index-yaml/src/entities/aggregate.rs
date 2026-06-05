use serde::{Deserialize, Serialize};

/// The aggregate operation — written as the field's type key
/// (`count:` / `sum:` / `avg:` / `min:` / `max:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateOp {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}
