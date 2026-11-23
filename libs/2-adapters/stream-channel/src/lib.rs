#![doc = include_str!("../README.md")]
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod config;

pub use config::ChannelConfig;

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use kernel::{IndexName, SinkName};
use stream::{
    AckHandle, Consumer, Delivery, Lane, LaneItem, LaneKey, Producer, QueueError, Request,
    RequestLane, Result, Stream, Substitute, SubstituteLane,
};
use tokio::sync::mpsc;

/// The in-process stream: one bounded channel per `(index, sink)` lane and
/// one for requests.
///
/// Built once by the daemon with the full lane set; every [`Stream::lane`] and
/// [`Stream::requests`] call hands out fresh ends over the same channels, so a
/// restarted engine re-attaches and is redelivered whatever its predecessor
/// left unacknowledged. Several deliveries may be outstanding at once (the
/// ingest engine holds every coalesced backfill request until the snapshot is
/// published); each is acknowledged on its own.
#[derive(Debug)]
pub struct ChannelStream {
    lanes: BTreeMap<LaneKey, Arc<Channel<LaneItem>>>,
    requests: Arc<Channel<Request>>,
    /// Each sink's substitute lane: fresh builds of its parked lanes' items.
    substitutes: BTreeMap<SinkName, Arc<Channel<Substitute>>>,
}

/// Each sink of `lanes` with its substitute lane, holding up to the
/// [`LANE_BACKSTOP`]: at most one substitute per lane is outstanding, so it
/// never binds.
fn substitute_lanes<'a>(
    lanes: impl IntoIterator<Item = &'a LaneKey>,
) -> BTreeMap<SinkName, Arc<Channel<Substitute>>> {
    lanes
        .into_iter()
        .map(|key| (key.sink.clone(), Arc::new(Channel::new(LANE_BACKSTOP))))
        .collect()
}

/// How many items a lane built from configuration holds before publishing
/// waits: a backstop that never binds before the ingest engine's memory
/// budget, which counts at least 1 KiB per batch (the default 512 MiB holds at
/// most 524,288 batches; only a budget past 1 GiB of 1 KiB batches reaches
/// it). A bounded channel doesn't allocate its capacity.
pub const LANE_BACKSTOP: usize = 1 << 20;

impl ChannelStream {
    /// A stream with the lanes `lanes`, plus a request lane holding
    /// `capacity` items. A sink's lanes share `capacity` between them: each
    /// holds `max(1, capacity / its sink's lanes)` items before publishing
    /// waits, so a sink holds about `capacity` items however many indexes it
    /// receives.
    pub fn new(capacity: usize, lanes: impl IntoIterator<Item = LaneKey>) -> Self {
        let capacity = capacity.max(1);
        let lanes: Vec<LaneKey> = lanes.into_iter().collect();
        let mut per_sink: BTreeMap<&SinkName, usize> = BTreeMap::new();
        for key in &lanes {
            *per_sink.entry(&key.sink).or_default() += 1;
        }
        let bound = |key: &LaneKey| {
            let shared = per_sink.get(&key.sink).copied().unwrap_or(1).max(1);
            (capacity / shared).max(1)
        };
        Self {
            lanes: lanes
                .iter()
                .map(|key| (key.clone(), Arc::new(Channel::new(bound(key)))))
                .collect(),
            requests: Arc::new(Channel::new(capacity)),
            substitutes: substitute_lanes(&lanes),
        }
    }

    /// Build from the adapter's own configuration: every lane, and the
    /// request lane, with the [`LANE_BACKSTOP`]. A `capacity` the config
    /// still names is ignored, with a warning: the memory budget bounds what
    /// lanes hold.
    pub fn from_config(config: &ChannelConfig, lanes: impl IntoIterator<Item = LaneKey>) -> Self {
        if let Some(capacity) = config.capacity {
            tracing::warn!(
                capacity,
                "[stream] capacity (and --queue-capacity) is deprecated and ignored: \
                 [concurrency] max_built_mib bounds what the lanes hold"
            );
        }
        Self::bounded_per_lane(LANE_BACKSTOP, lanes)
    }

    /// A stream whose every lane, and the request lane, holds `bound` items,
    /// however many lanes a sink has.
    fn bounded_per_lane(bound: usize, lanes: impl IntoIterator<Item = LaneKey>) -> Self {
        let bound = bound.max(1);
        let lanes: Vec<LaneKey> = lanes.into_iter().collect();
        Self {
            substitutes: substitute_lanes(&lanes),
            lanes: lanes
                .into_iter()
                .map(|key| (key, Arc::new(Channel::new(bound))))
                .collect(),
            requests: Arc::new(Channel::new(bound)),
        }
    }

    /// A stream with one lane per pair of `indexes` and `sinks`.
    pub fn over(
        capacity: usize,
        indexes: impl IntoIterator<Item = IndexName>,
        sinks: impl IntoIterator<Item = SinkName> + Clone,
    ) -> Self {
        let lanes: Vec<LaneKey> = indexes
            .into_iter()
            .flat_map(|index| {
                sinks
                    .clone()
                    .into_iter()
                    .map(move |sink| LaneKey::new(index.clone(), sink))
            })
            .collect();
        Self::new(capacity, lanes)
    }

    /// The lanes of this stream.
    pub fn keys(&self) -> impl Iterator<Item = &LaneKey> {
        self.lanes.keys()
    }

    /// Every sink this stream has a lane into.
    pub fn sinks(&self) -> Vec<SinkName> {
        let sinks: std::collections::BTreeSet<&SinkName> =
            self.lanes.keys().map(|key| &key.sink).collect();
        sinks.into_iter().cloned().collect()
    }
}

impl Stream for ChannelStream {
    fn lane(&self, key: &LaneKey) -> Result<Lane> {
        let channel = self
            .lanes
            .get(key)
            .ok_or_else(|| QueueError::UnknownLane(key.to_string()))?;
        Ok(Lane {
            producer: Box::new(ChannelProducer {
                channel: Arc::clone(channel),
            }),
            consumer: Box::new(ChannelConsumer {
                channel: Arc::clone(channel),
            }),
        })
    }

    fn lanes(&self, sink: &SinkName) -> Vec<LaneKey> {
        self.lanes
            .keys()
            .filter(|key| &key.sink == sink)
            .cloned()
            .collect()
    }

    fn substitutes(&self, sink: &SinkName) -> Result<SubstituteLane> {
        let channel = self
            .substitutes
            .get(sink)
            .ok_or_else(|| QueueError::UnknownLane(format!("substitutes of {sink}")))?;
        Ok(SubstituteLane {
            producer: Box::new(ChannelProducer {
                channel: Arc::clone(channel),
            }),
            consumer: Box::new(ChannelConsumer {
                channel: Arc::clone(channel),
            }),
        })
    }

    fn requests(&self) -> Result<RequestLane> {
        Ok(RequestLane {
            producer: Box::new(ChannelProducer {
                channel: Arc::clone(&self.requests),
            }),
            consumer: Box::new(ChannelConsumer {
                channel: Arc::clone(&self.requests),
            }),
        })
    }

    fn is_idle(&self) -> bool {
        self.lanes.values().all(|channel| channel.is_idle())
    }
}

/// One bounded channel with its in-flight ledger and acknowledged position.
/// The receiver sits behind a mutex because consumers are handed out
/// repeatedly (one active at a time) and `mpsc::Receiver` cannot be cloned.
#[derive(Debug)]
struct Channel<T> {
    tx: mpsc::Sender<T>,
    rx: tokio::sync::Mutex<mpsc::Receiver<T>>,
    state: Mutex<Ledger<T>>,
    /// The ledger's `due` count, written under its lock, so `is_caught_up`
    /// reads it without taking the lock the ingest engine shares (a sink
    /// asks once or twice per lane item).
    due: AtomicUsize,
    /// Woken by every release: a `recv` already waiting on the channel
    /// hands the released delivery back instead of waiting for a new item.
    released: tokio::sync::Notify,
}

/// Delivered, not yet acknowledged items in lane order, each under a ticket
/// its ack handle carries. An entry marked `due` was released (nacked, or its
/// handle dropped unacked) and is served again by the next `recv`, the
/// earliest first, before anything new is pulled off the channel. Releasing
/// one ticket leaves every other delivery in flight alone.
///
/// So a released delivery can come back after later ones were acknowledged,
/// and deliveries are acknowledged in any order: the sink engine writes
/// several at once and keeps per-document order itself (ADR 0012). How far a
/// lane has landed is not the stream's to say: the engine's `Progress` keeps
/// it.
#[derive(Debug)]
struct Ledger<T> {
    in_flight: Vec<InFlight<T>>,
    next_ticket: Ticket,
    /// How many entries are marked `due`.
    due: usize,
}

#[derive(Debug)]
struct InFlight<T> {
    ticket: Ticket,
    item: T,
    due: bool,
}

type Ticket = u64;

impl<T: Clone> Ledger<T> {
    /// Hand out the earliest due entry again, if any.
    fn take_due(&mut self) -> Option<(Ticket, T)> {
        if self.due == 0 {
            return None;
        }
        let entry = self.in_flight.iter_mut().find(|entry| entry.due)?;
        entry.due = false;
        let taken = Some((entry.ticket, entry.item.clone()));
        self.due -= 1;
        taken
    }
}

impl<T> Channel<T> {
    fn lock(&self) -> std::sync::MutexGuard<'_, Ledger<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Mark `tickets` due for redelivery. Only they come back, in lane order;
    /// a ticket already acknowledged is ignored.
    fn release(&self, tickets: &[Ticket]) {
        let mut ledger = self.lock();
        let mut released = 0;
        for entry in &mut ledger.in_flight {
            if tickets.contains(&entry.ticket) && !entry.due {
                entry.due = true;
                released += 1;
            }
        }
        ledger.due += released;
        self.due.store(ledger.due, Ordering::Release);
        drop(ledger);
        if released > 0 {
            self.released.notify_waiters();
        }
    }

    /// Nothing queued behind the deliveries already handed out. A hint read
    /// without the ledger's lock: the lane can change the moment after.
    fn is_caught_up(&self) -> bool {
        self.due.load(Ordering::Acquire) == 0 && self.queue_is_empty()
    }

    /// Nothing queued and nothing delivered but unacknowledged.
    fn is_idle(&self) -> bool {
        self.lock().in_flight.is_empty() && self.queue_is_empty()
    }

    fn queue_is_empty(&self) -> bool {
        self.tx.max_capacity() == self.tx.capacity()
    }
}

impl<T: Clone + Send + 'static> Channel<T> {
    fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self {
            tx,
            rx: tokio::sync::Mutex::new(rx),
            state: Mutex::new(Ledger {
                in_flight: Vec::new(),
                next_ticket: 0,
                due: 0,
            }),
            due: AtomicUsize::new(0),
            released: tokio::sync::Notify::new(),
        }
    }

    /// The next delivery: the earliest released one, else the next item on
    /// the channel. A release while it waits on the channel wakes it (the
    /// wake-up is registered before the ledger is read, so none is missed),
    /// and it hands the released delivery back; nothing else wakes it.
    async fn recv(&self) -> Option<(Ticket, T)> {
        let item = loop {
            let released = self.released.notified();
            tokio::pin!(released);
            released.as_mut().enable();
            if self.due.load(Ordering::Acquire) > 0 {
                let mut ledger = self.lock();
                if let Some(redelivery) = ledger.take_due() {
                    self.due.store(ledger.due, Ordering::Release);
                    return Some(redelivery);
                }
            }
            let mut rx = self.rx.lock().await;
            // `recv` is cancel-safe: dropped for a release, it loses no item.
            let mut receiving = std::pin::pin!(rx.recv());
            let next = std::future::poll_fn(|cx| {
                if released.as_mut().poll(cx).is_ready() {
                    return std::task::Poll::Ready(None);
                }
                receiving.as_mut().poll(cx).map(Some)
            })
            .await;
            if let Some(item) = next {
                break item?;
            }
        };
        let mut ledger = self.lock();
        let ticket = ledger.next_ticket;
        ledger.next_ticket += 1;
        // A release that landed while awaiting the channel made an earlier
        // entry due: serve it first and leave the new item due behind it, so
        // nothing is delivered twice or out of order.
        ledger.in_flight.push(InFlight {
            ticket,
            item,
            due: true,
        });
        ledger.due += 1;
        let taken = ledger.take_due();
        self.due.store(ledger.due, Ordering::Release);
        taken
    }

    fn ack(&self, ticket: Ticket) {
        let mut ledger = self.lock();
        let Some(index) = ledger
            .in_flight
            .iter()
            .position(|entry| entry.ticket == ticket)
        else {
            return;
        };
        if ledger.in_flight.remove(index).due {
            ledger.due -= 1;
            self.due.store(ledger.due, Ordering::Release);
        }
    }
}

/// The publishing end of one channel.
struct ChannelProducer<T> {
    channel: Arc<Channel<T>>,
}

impl<T> std::fmt::Debug for ChannelProducer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelProducer").finish_non_exhaustive()
    }
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> Producer<T> for ChannelProducer<T> {
    async fn publish(&self, item: T) -> Result<()> {
        self.channel
            .tx
            .send(item)
            .await
            .map_err(|_| QueueError::Closed)
    }
}

/// The receiving end of one channel.
struct ChannelConsumer<T> {
    channel: Arc<Channel<T>>,
}

impl<T> std::fmt::Debug for ChannelConsumer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelConsumer").finish_non_exhaustive()
    }
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> Consumer<T> for ChannelConsumer<T> {
    async fn recv(&mut self) -> Result<Option<Delivery<T>>> {
        Ok(self.channel.recv().await.map(|(ticket, item)| {
            Delivery::new(
                item,
                Box::new(ChannelAck {
                    channel: Arc::clone(&self.channel),
                    ticket,
                    acked: false,
                }),
            )
        }))
    }

    fn is_empty(&self) -> bool {
        self.channel.is_caught_up()
    }
}

/// Acking removes the item from the ledger. Nacking,
/// or dropping the handle unacked (an engine that died holding it), marks the
/// item for redelivery on the next `recv`.
struct ChannelAck<T> {
    channel: Arc<Channel<T>>,
    ticket: Ticket,
    acked: bool,
}

impl<T> std::fmt::Debug for ChannelAck<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelAck")
            .field("ticket", &self.ticket)
            .finish_non_exhaustive()
    }
}

impl<T> Drop for ChannelAck<T> {
    fn drop(&mut self) {
        // The redelivery path for a handle dropped unacked. After `ack` the
        // ticket is gone from the ledger, so its lock isn't taken again.
        if !self.acked {
            self.channel.release(&[self.ticket]);
        }
    }
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> AckHandle for ChannelAck<T> {
    async fn ack(mut self: Box<Self>) -> Result<()> {
        self.channel.ack(self.ticket);
        self.acked = true;
        Ok(())
    }

    async fn nack(self: Box<Self>) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
