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
//! 3. waits until every lane is acknowledged ([`drain_lanes`]), handing the
//!    watermark to the source as it moves;
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
use std::time::Duration;

use kernel::Position;
use source::cdc::ChangeCapture;
use stream::Stream;
use tokio::sync::watch;

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
pub struct Stop(Arc<watch::Sender<bool>>);

impl Default for Stop {
    fn default() -> Self {
        Self::new()
    }
}

impl Stop {
    /// A stop nobody has requested yet.
    pub fn new() -> Self {
        Self(Arc::new(watch::Sender::new(false)))
    }

    /// Request the stop. Idempotent.
    pub fn request(&self) {
        self.0.send_replace(true);
    }

    /// Whether the stop was requested.
    pub fn is_requested(&self) -> bool {
        *self.0.borrow()
    }

    /// Resolves once the stop is requested (at once if it already was).
    pub async fn requested(&self) {
        let mut seen = self.0.subscribe();
        // The sender lives in `self`, so the channel never closes under us.
        let _ = seen.wait_for(|requested| *requested).await;
    }
}

/// Wait until every lane of `stream` is acknowledged, handing the watermark
/// to `source` each time it moves, the last time once the lanes are idle.
/// `last` is the position last handed over, kept across calls.
pub async fn drain_lanes(
    stream: &dyn Stream,
    source: &dyn ChangeCapture,
    last: &mut Option<Position>,
) {
    loop {
        let idle = stream.is_idle();
        if let Some(watermark) = stream.watermark()
            && *last != Some(watermark)
        {
            source.confirm(watermark);
            *last = Some(watermark);
        }
        if idle {
            return;
        }
        tokio::time::sleep(DRAIN_POLL).await;
    }
}
