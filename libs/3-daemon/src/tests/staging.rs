//! Many indexes on several sinks (issue #398): each sink stages its indexes
//! concurrently, and still every sink's first stage, rebuilds included, ends
//! before the source prepares its resume point (#120).

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{ContentHash, Envelope, GenericValue, IndexMapping, IndexName};
use kernel::{SinkName, TableName};
use sink::{Sink, SinkOptions, WriteReport};
use source::cdc::Continuity;
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowImage, RowKey};

use super::{MockBackends, ScriptedSource, backendless_config, primary};
use crate::Daemon;

const INDEXES: usize = 12;

fn index(i: usize) -> IndexName {
    IndexName::try_new(format!("index_{i:02}")).unwrap()
}

/// [`INDEXES`] indexes, each rooted at `users`.
#[derive(Debug)]
struct ManyIndexes;

#[async_trait]
impl DocumentBuilder for ManyIndexes {
    async fn resolve(
        &self,
        _: &TableName,
        _: &RowKey,
        _: Option<&RowImage>,
        _: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok(Vec::new())
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Map(Default::default()),
        })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        (0..INDEXES).map(index).collect()
    }

    async fn keys(
        &self,
        _: &IndexName,
        _: Option<&RowKey>,
        _: usize,
    ) -> source::Result<Vec<RowKey>> {
        Ok(Vec::new())
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        Ok((0..INDEXES)
            .map(|i| IndexMapping {
                index: index(i),
                hash: ContentHash::new(1),
                fields: Vec::new(),
            })
            .collect())
    }
}

/// Every index seeded until rebuilt; each staging call takes a moment and is
/// recorded in the shared `events` as `"<call> <sink> <index>"` once done.
#[derive(Debug)]
struct SlowStaging {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
    unseeded: Mutex<BTreeSet<IndexName>>,
    in_flight: AtomicUsize,
    peak: AtomicUsize,
}

impl SlowStaging {
    fn new(name: &'static str, events: &Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            name,
            events: Arc::clone(events),
            unseeded: Mutex::new(BTreeSet::new()),
            in_flight: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }

    async fn call(&self, call: &str, index: &IndexName) {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(5)).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.events
            .lock()
            .unwrap()
            .push(format!("{call} {} {index}", self.name));
    }
}

#[async_trait]
impl Sink for SlowStaging {
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<()> {
        self.call("ensure", &mapping.index).await;
        Ok(())
    }

    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(!self.unseeded.lock().unwrap().contains(index))
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.unseeded.lock().unwrap().remove(index);
        Ok(())
    }

    async fn reindex(&self, mapping: &IndexMapping) -> sink::Result<()> {
        self.call("reindex", &mapping.index).await;
        self.unseeded.lock().unwrap().insert(mapping.index.clone());
        Ok(())
    }
}

#[tokio::test]
async fn every_index_of_every_sink_is_staged_before_the_source_prepares() {
    let source = ScriptedSource::new(Vec::new()).with_continuity(Continuity::Fresh);
    let events = Arc::clone(&source.events);
    let first = Arc::new(SlowStaging::new("primary", &events));
    let second = Arc::new(SlowStaging::new("audit", &events));
    let backends = Arc::new(MockBackends {
        capture: Arc::new(source),
        documents: Arc::new(ManyIndexes),
        sinks: vec![
            (
                primary(),
                Arc::clone(&first) as Arc<dyn Sink>,
                SinkOptions::default(),
            ),
            (
                SinkName::try_new("audit").unwrap(),
                Arc::clone(&second) as Arc<dyn Sink>,
                SinkOptions::default(),
            ),
        ],
        built: Arc::new(Mutex::new(Vec::new())),
    });

    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(10),
        running.run(std::future::pending::<()>()),
    )
    .await
    .unwrap()
    .unwrap();

    let events = events.lock().unwrap().clone();
    let prepare = events.iter().position(|e| e == "prepare").unwrap();
    for sink in ["primary", "audit"] {
        for i in 0..INDEXES {
            for call in ["ensure", "reindex"] {
                let staged = format!("{call} {sink} {}", index(i));
                let at = events.iter().position(|e| *e == staged);
                assert!(
                    at.is_some_and(|at| at < prepare),
                    "{staged} before prepare: {events:?}"
                );
            }
        }
    }
    assert!(first.peak.load(Ordering::SeqCst) > 1, "indexes overlapped");
    assert!(second.peak.load(Ordering::SeqCst) > 1, "indexes overlapped");
    assert!(
        first.unseeded.lock().unwrap().is_empty(),
        "every rebuild seeded"
    );
    assert!(
        second.unseeded.lock().unwrap().is_empty(),
        "every rebuild seeded"
    );
}
