//! Ending the engines gracefully: a [`Stop`] request, and the lane drain both
//! ways of ending share.
//!
//! A stop is not an abort. The ingest engine, handed a [`Stop`] with
//! [`IngestEngine::with_stop`](crate::IngestEngine::with_stop), checks it
//! between the live stream's items and, once requested:
//!
//! 1. reads nothing more from the live stream and takes no new request
//!    (a snapshot in progress is abandoned at a row boundary, its request left
//!    unacknowledged, so the sink asks again on the next start);
//! 2. publishes the batch it was buffering, exactly as a normal commit;
//! 3. waits until every lane is acknowledged ([`drain_lanes`]), handing what
//!    landed to the source as it moves;
//! 4. asks the source to close its live stream once that final position
//!    reached it ([`ChangeCapture::close_live`]), and returns `Ok`.
//!
//! Nothing here bounds the wait: a stalled sink holds step 3. The caller
//! that asked for the stop owns the deadline.
//!
//! ```
//! use engine::Stop;
//!
//! let stop = Stop::new();
//! let seen_by_the_engine = stop.clone();
//! assert!(!seen_by_the_engine.is_requested());
//! stop.request();
//! assert!(seen_by_the_engine.is_requested());
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use source::cdc::{ChangeCapture, Landed};
use stream::Stream;
use tokio::sync::Notify;

use crate::progress::Progress;

/// How often [`drain_lanes`] looks at the lanes.
const DRAIN_POLL: Duration = Duration::from_millis(10);

/// A request to end the engines gracefully. Cheap to clone; every clone sees
/// the same request, and a request can't be withdrawn.
///
/// Once requested, the [`IngestEngine`](crate::IngestEngine) reads nothing
/// more and takes no new request (a snapshot in progress is abandoned
/// unacknowledged, so its sink asks again on the next start), publishes the
/// batch it was building, waits with [`drain_lanes`] until every lane is
/// acknowledged, then asks the source to close its stream
/// ([`ChangeCapture::close_live`]) and returns `Ok`. Nothing bounds the wait:
/// the caller that asked for the stop owns the deadline.
#[derive(Debug, Clone)]
pub struct Stop(Arc<Requested>);

#[derive(Debug, Default)]
struct Requested {
    flag: AtomicBool,
    notify: Notify,
}

impl Default for Stop {
    fn default() -> Self {
        Self::new()
    }
}

impl Stop {
    /// A stop nobody has requested yet.
    pub fn new() -> Self {
        Self(Arc::default())
    }

    /// Request the stop. Idempotent.
    pub fn request(&self) {
        self.0.flag.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }

    /// Whether the stop was requested. One atomic load: cheap enough to ask
    /// before every item of a stream.
    pub fn is_requested(&self) -> bool {
        self.0.flag.load(Ordering::SeqCst)
    }

    /// Resolves once the stop is requested (at once if it already was).
    pub async fn requested(&self) {
        let notified = self.notified();
        if self.is_requested() {
            return;
        }
        notified.await;
    }

    /// A future woken by a [`request`](Self::request) made after this call,
    /// even before it is first polled. Creating one takes no lock, so a
    /// caller can make one per item and poll it only when it would wait.
    pub(crate) fn notified(&self) -> tokio::sync::futures::Notified<'_> {
        self.0.notify.notified()
    }
}

/// Wait until every lane of `stream` is acknowledged, handing how far the
/// lanes landed (`progress`) to `source` each time it changes, and once more
/// once the lanes are idle. `last` is the answer last handed over, kept
/// across calls.
pub async fn drain_lanes(
    stream: &dyn Stream,
    source: &dyn ChangeCapture,
    progress: &Progress,
    last: &mut Option<Landed>,
) {
    loop {
        let idle = stream.is_idle();
        let landed = progress.landed();
        if idle || *last != Some(landed) {
            source.confirm(landed);
            *last = Some(landed);
        }
        if idle {
            return;
        }
        tokio::time::sleep(DRAIN_POLL).await;
    }
}
