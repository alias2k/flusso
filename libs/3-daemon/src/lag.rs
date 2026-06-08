//! Background poller for source capture lag.
//!
//! Lag (a replication slot's distance behind the server's WAL) isn't on the
//! change path — it's sampled out-of-band on a timer and reported to the
//! [`Observer`]. The poll uses the source's own
//! [`ChangeCapture::lag`](sources_core::cdc::ChangeCapture::lag), so it stays
//! source-agnostic: a mechanism with no notion of lag returns `None` and this
//! loop simply records nothing.

use std::sync::Arc;
use std::time::Duration;

use engine::Observer;
use sources_core::cdc::ChangeCapture;
use tokio::time::{MissedTickBehavior, interval};

/// Poll `source.lag()` every `period`, reporting each known value to `observer`,
/// until the task is aborted. Errors are logged and the loop continues — a
/// failed lag probe must never take down the pipeline.
pub(crate) async fn poll(
    source: Arc<dyn ChangeCapture>,
    observer: Arc<dyn Observer>,
    period: Duration,
) {
    let mut ticker = interval(period);
    // If a probe runs long, skip missed ticks rather than bursting to catch up.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        match source.lag().await {
            Ok(Some(bytes)) => observer.on_slot_lag(bytes),
            Ok(None) => tracing::trace!("source reports no lag yet"),
            Err(error) => tracing::warn!(%error, "failed to poll source lag"),
        }
    }
}
