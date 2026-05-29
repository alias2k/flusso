use std::sync::Arc;

/// Confirms that one [`Change`](crate::Change) was durably processed downstream
/// — written to the sink and checkpointed. Returned inside every change.
///
/// Each change carries a monotonically increasing sequence number. The
/// mechanism only advances its durable resume point (for WAL, a replication
/// slot's `confirmed_flush_lsn`) to the highest **contiguous** confirmed
/// sequence. Confirming out of order is therefore safe: a gap simply holds the
/// resume point back until it is filled, and nothing already confirmed is
/// re-sent.
///
/// Dropping an `Ack` without calling [`confirm`](Self::confirm) does *not*
/// confirm the change. It will be redelivered after a restart — this is the
/// at-least-once guarantee.
#[derive(Debug)]
pub struct Ack {
    seq: u64,
    sink: Arc<dyn AckSink>,
}

impl Ack {
    /// Build an `Ack` for the change with sequence number `seq`. Called by a
    /// mechanism implementation as it emits each change.
    pub fn new(seq: u64, sink: Arc<dyn AckSink>) -> Self {
        Self { seq, sink }
    }

    /// Confirm that this change was durably processed.
    pub fn confirm(self) {
        self.sink.confirm(self.seq);
    }
}

/// The mechanism-side endpoint an [`Ack`] reports back to.
///
/// Implemented in the source crate — for example, mapping a sequence number to
/// its LSN and advancing the replication slot once the confirmed sequences are
/// contiguous. Implementations must be cheap to call and safe to call out of
/// order.
pub trait AckSink: std::fmt::Debug + Send + Sync {
    /// Record that the change with this sequence number was durably processed.
    fn confirm(&self, seq: u64);
}

// The design depends on an `Ack` being movable across threads/tasks and held
// arbitrarily long before [`Ack::confirm`] is called — confirmation happens
// externally, asynchronously, and out of order. Lock that property in at
// compile time so it can never silently regress (e.g. if `AckSink` ever lost
// its `Send + Sync` bound).
const _: () = {
    const fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<Ack>();
};
