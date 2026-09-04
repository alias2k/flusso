#![doc = include_str!("../README.md")]
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod config;

pub use config::ChannelConfig;

use std::collections::BTreeMap;
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
}

/// Delivered, not yet acknowledged items in delivery order, each under a
/// ticket its ack handle carries. Everything at or after `cursor` is due for
/// redelivery: a nack or a dropped consumer moves the cursor back, `recv`
/// serves from it before pulling anything new off the channel.
#[derive(Debug)]
struct Ledger<T> {
    in_flight: Vec<(Ticket, T)>,
    cursor: usize,
    next_ticket: Ticket,
    /// The last position acknowledged on this lane.
    acked: Option<Position>,
}

type Ticket = u64;

impl<T> Channel<T> {
    fn lock(&self) -> std::sync::MutexGuard<'_, Ledger<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn acked(&self) -> Option<Position> {
        self.lock().acked
    }

    /// Mark `tickets` for redelivery: the cursor moves back to the earliest of
    /// them, so it and everything delivered after it come back in their original
    /// order (a lane consumer processes in order, so what followed a failed item
    /// is redone too).
    fn release(&self, tickets: &[Ticket]) {
        let mut ledger = self.lock();
        let earliest = ledger
            .in_flight
            .iter()
            .position(|(t, _)| tickets.contains(t));
        if let Some(index) = earliest {
            ledger.cursor = ledger.cursor.min(index);
        }
    }

    /// Nothing queued behind the deliveries already handed out.
    fn is_caught_up(&self) -> bool {
        let ledger = self.lock();
        ledger.cursor == ledger.in_flight.len() && self.queue_is_empty()
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
                cursor: 0,
                next_ticket: 0,
                acked: None,
            }),
        }
    }

    async fn recv(&self) -> Option<(Ticket, T)> {
        if let Some(redelivery) = self.redelivery() {
            return Some(redelivery);
        }
        let item = self.rx.lock().await.recv().await?;
        let mut ledger = self.lock();
        let ticket = ledger.next_ticket;
        ledger.next_ticket += 1;
        ledger.in_flight.push((ticket, item.clone()));
        // A release that landed while awaiting the channel moved the cursor
        // back: serve the redelivery first and leave the new item in order
        // behind it, so nothing is delivered twice or out of order.
        if ledger.cursor + 1 < ledger.in_flight.len() {
            let entry = ledger.in_flight.get(ledger.cursor).cloned()?;
            ledger.cursor += 1;
            return Some(entry);
        }
        ledger.cursor = ledger.in_flight.len();
        Some((ticket, item))
    }

    fn redelivery(&self) -> Option<(Ticket, T)> {
        let mut ledger = self.lock();
        let entry = ledger.in_flight.get(ledger.cursor).cloned()?;
        ledger.cursor += 1;
        Some(entry)
    }

    fn ack(&self, ticket: Ticket) {
        let mut ledger = self.lock();
        let Some(index) = ledger.in_flight.iter().position(|(t, _)| *t == ticket) else {
            return;
        };
        let (_, item) = ledger.in_flight.remove(index);
        if index < ledger.cursor {
            ledger.cursor -= 1;
        }
        if let Some(position) = item.position() {
            ledger.acked = Some(ledger.acked.map_or(position, |p| p.max(position)));
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
                }),
            )
        }))
    }

    fn is_empty(&self) -> bool {
        self.channel.is_caught_up()
    }
}

/// Acking removes the item from the ledger and records its position. Nacking,
/// or dropping the handle unacked (an engine that died holding it), marks the
/// item for redelivery on the next `recv`.
struct ChannelAck<T> {
    channel: Arc<Channel<T>>,
    ticket: Ticket,
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
        // A no-op after `ack` (the ticket is gone from the ledger); the
        // redelivery path for a handle dropped unacked.
        self.channel.release(&[self.ticket]);
    }
}

#[async_trait]
impl<T: Positioned + Clone + Send + Sync + 'static> AckHandle for ChannelAck<T> {
    async fn ack(self: Box<Self>) -> Result<()> {
        self.channel.ack(self.ticket);
        Ok(())
    }

    async fn nack(self: Box<Self>) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
