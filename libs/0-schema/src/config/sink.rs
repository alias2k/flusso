use serde::{Deserialize, Serialize};

use crate::common;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sink {
    #[serde(rename = "type")]
    pub sink_type: common::SinkType,
}
