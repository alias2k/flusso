//! The live tests stage documents one at a time and send them together, as
//! `apply` + `flush` did before `Sink::write`: `apply` holds an envelope,
//! `flush` writes everything held as one write. Everything else reaches the
//! sink itself through `Deref`.

use std::ops::Deref;
use std::sync::Mutex;

use kernel::Envelope;
use sink::{Sink, WriteReport};
use sink_opensearch::OpensearchSink;

#[derive(Debug)]
pub(crate) struct Buffered {
    sink: OpensearchSink,
    held: Mutex<Vec<Envelope>>,
}

impl Buffered {
    pub(crate) fn new(sink: OpensearchSink) -> Self {
        Self {
            sink,
            held: Mutex::new(Vec::new()),
        }
    }

    pub(crate) async fn apply(&self, envelope: &Envelope) -> sink::Result<()> {
        self.held.lock().unwrap().push(envelope.clone());
        Ok(())
    }

    pub(crate) async fn flush(&self, caught_up: bool) -> sink::Result<WriteReport> {
        let held = std::mem::take(&mut *self.held.lock().unwrap());
        self.sink.write(&held, caught_up).await
    }
}

impl Deref for Buffered {
    type Target = OpensearchSink;

    fn deref(&self) -> &OpensearchSink {
        &self.sink
    }
}
