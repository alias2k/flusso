#![doc = include_str!("../README.md")]
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod config;

pub use config::ChannelConfig;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use kernel::{Position, SinkName};
use stream::{
    AckHandle, Consumer, Delivery, Lane, LaneItem, Producer, QueueError, Request, RequestLane,
    Result, Stream,
};
use tokio::sync::mpsc;

/// The in-process stream: one bounded channel per sink lane, one for requests,
/// and the watermark over the lanes' acknowledgements.
///
/// Built once by the daemon with the full sink set; every [`Stream::lane`] and
/// [`Stream::requests`] call hands out fresh ends over the same channels, so a
/// restarted engine re-attaches and is redelivered whatever its predecessor
/// left unacknowledged. Several deliveries may be outstanding at once (the
/// ingest engine holds every coalesced backfill request until the snapshot is
/// published); each is acknowledged on its own.
#[derive(Debug)]
pub struct ChannelStream {
    lanes: BTreeMap<SinkName, Arc<Channel<LaneItem>>>,
    requests: Arc<Channel<Request>>,
}

impl ChannelStream {
    /// A stream with one lane per sink in `sinks`, each holding `capacity`
    /// items before publishing waits, plus a request lane of the same size.
    pub fn new(capacity: usize, sinks: impl IntoIterator<Item = SinkName>) -> Self {
        let capacity = capacity.max(1);
        Self {
            lanes: sinks
                .into_iter()
                .map(|sink| (sink, Arc::new(Channel::new(capacity))))
                .collect(),
            requests: Arc::new(Channel::new(capacity)),
        }
    }

    /// Build from the adapter's own configuration.
    pub fn from_config(config: &ChannelConfig, sinks: impl IntoIterator<Item = SinkName>) -> Self {
        Self::new(config.capacity, sinks)
    }

    /// The sinks this stream has a lane for.
    pub fn sinks(&self) -> impl Iterator<Item = &SinkName> {
        self.lanes.keys()
    }
}

impl Stream for ChannelStream {
    fn lane(&self, sink: &SinkName) -> Result<Lane> {
        let channel = self
            .lanes
            .get(sink)
            .ok_or_else(|| QueueError::UnknownLane(sink.to_string()))?;
        Ok(Lane {
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

    fn watermark(&self) -> Option<Position> {
        let mut lowest: Option<Position> = None;
        for channel in self.lanes.values() {
            let acked = channel.acked()?;
            lowest = Some(lowest.map_or(acked, |low| low.min(acked)));
        }
        lowest
    }

    fn is_idle(&self) -> bool {
        self.lanes.values().all(|channel| channel.is_idle())
    }
}

/// What acknowledging an item confirms for its lane's watermark.
trait Positioned {
    fn position(&self) -> Option<Position>;
}

impl Positioned for LaneItem {
    fn position(&self) -> Option<Position> {
        LaneItem::position(self)
    }
}

impl Positioned for Request {
    fn position(&self) -> Option<Position> {
        None
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
}

/// Delivered, not yet acknowledged items in lane order, each under a ticket
/// its ack handle carries. An entry marked `due` was released (nacked, or its
/// handle dropped unacked) and is served again by the next `recv`, the
/// earliest first, before anything new is pulled off the channel. Releasing
/// one ticket leaves every other delivery in flight alone.
///
/// So a released delivery can come back after later ones were acknowledged,
/// and deliveries are acknowledged in any order: the sink engine writes
/// several at once and keeps per-document order itself (ADR 0012). The
/// lane's watermark is therefore not the highest acknowledgement but the last
/// positioned item delivered before the oldest unacknowledged positioned one,
/// which each entry records when it is first delivered.
#[derive(Debug)]
struct Ledger<T> {
    in_flight: Vec<InFlight<T>>,
    next_ticket: Ticket,
    /// The position of the last positioned item first delivered on this lane.
    delivered: Option<Position>,
    /// How many entries are marked `due`.
    due: usize,
}

#[derive(Debug)]
struct InFlight<T> {
    ticket: Ticket,
    item: T,
    due: bool,
    /// The position of the last positioned item delivered before this one:
    /// what the lane may confirm while this one is unacknowledged.
    before: Option<Position>,
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

impl<T: Positioned + Clone + Send + 'static> Channel<T> {
    fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self {
            tx,
            rx: tokio::sync::Mutex::new(rx),
            state: Mutex::new(Ledger {
                in_flight: Vec::new(),
                next_ticket: 0,
                delivered: None,
                due: 0,
            }),
            due: AtomicUsize::new(0),
        }
    }

    /// The last positioned item delivered before the oldest unacknowledged
    /// positioned one; with none unacknowledged, the last positioned item
    /// delivered. Items still queued are newer than every delivered one, so
    /// they never lower it.
    fn acked(&self) -> Option<Position> {
        let ledger = self.lock();
        match ledger
            .in_flight
            .iter()
            .find(|entry| entry.item.position().is_some())
        {
            Some(oldest) => oldest.before,
            None => ledger.delivered,
        }
    }

    async fn recv(&self) -> Option<(Ticket, T)> {
        if self.due.load(Ordering::Acquire) > 0 {
            let mut ledger = self.lock();
            if let Some(redelivery) = ledger.take_due() {
                self.due.store(ledger.due, Ordering::Release);
                return Some(redelivery);
            }
        }
        let item = self.rx.lock().await.recv().await?;
        let mut ledger = self.lock();
        let ticket = ledger.next_ticket;
        ledger.next_ticket += 1;
        let before = ledger.delivered;
        if let Some(position) = item.position() {
            ledger.delivered = Some(position);
        }
        // A release that landed while awaiting the channel made an earlier
        // entry due: serve it first and leave the new item due behind it, so
        // nothing is delivered twice or out of order.
        ledger.in_flight.push(InFlight {
            ticket,
            item,
            due: true,
            before,
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
impl<T: Positioned + Clone + Send + Sync + 'static> Producer<T> for ChannelProducer<T> {
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
impl<T: Positioned + Clone + Send + Sync + 'static> Consumer<T> for ChannelConsumer<T> {
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

/// Acking removes the item from the ledger (the watermark is read from what
/// remains: see `Channel::acked`). Nacking,
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
impl<T: Positioned + Clone + Send + Sync + 'static> AckHandle for ChannelAck<T> {
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
