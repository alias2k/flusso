use std::sync::Arc;

use async_trait::async_trait;
use schema_core::{GenericValue, IndexMapping, IndexName};

use crate::{FlushReport, Result, Sink};

/// Dispatches every sink operation to a set of inner sinks in declaration order.
///
/// `ensure_index`, `upsert`, `delete`, and `flush` are sent to each sink
/// sequentially; the first error short-circuits. `is_seeded` returns `true` only when **all**
/// inner sinks report the index as seeded (AND semantics: every destination
/// must hold the data before a backfill is considered complete). `mark_seeded`
/// is called on all inner sinks.
#[derive(Debug, Clone)]
pub struct FanOutSink {
    sinks: Vec<Arc<dyn Sink>>,
}

impl FanOutSink {
    pub fn new(sinks: Vec<Arc<dyn Sink>>) -> Self {
        Self { sinks }
    }
}

#[async_trait]
impl Sink for FanOutSink {
    async fn ensure_index(&self, mapping: &IndexMapping) -> Result<()> {
        for sink in &self.sinks {
            sink.ensure_index(mapping).await?;
        }
        Ok(())
    }

    async fn upsert(&self, index: &IndexName, id: &str, document: &GenericValue) -> Result<()> {
        for sink in &self.sinks {
            sink.upsert(index, id, document).await?;
        }
        Ok(())
    }

    async fn delete(&self, index: &IndexName, id: &str) -> Result<()> {
        for sink in &self.sinks {
            sink.delete(index, id).await?;
        }
        Ok(())
    }

    async fn flush(&self, caught_up: bool) -> Result<FlushReport> {
        let mut report = FlushReport::default();
        for sink in &self.sinks {
            report.rejected.extend(sink.flush(caught_up).await?.rejected);
        }
        Ok(report)
    }

    async fn is_seeded(&self, index: &IndexName) -> Result<bool> {
        for sink in &self.sinks {
            if !sink.is_seeded(index).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn mark_seeded(&self, index: &IndexName) -> Result<()> {
        for sink in &self.sinks {
            sink.mark_seeded(index).await?;
        }
        Ok(())
    }
}
