//! Reading what the sink keeps about detaching an index (ADR 0012 stage 10,
//! slice 1).

use kernel::IndexMapping;
use sink::{Detachment, Result};

use crate::OpensearchSink;

impl OpensearchSink {
    /// What the meta index keeps about detaching `mapping`'s index.
    pub(crate) async fn read_detachment(&self, _mapping: &IndexMapping) -> Result<Detachment> {
        Ok(Detachment::default())
    }
}

#[cfg(test)]
mod tests;
