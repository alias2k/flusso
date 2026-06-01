//! A generic work-queue abstraction for the sync pipeline.
//!
//! The engine publishes work items with a [`Producer`] and processes them from
//! a [`Consumer`]; each item arrives as a [`Delivery`] carrying an [`AckHandle`]
//! that confirms (or returns) it. This is deliberately domain-agnostic —
//! generic over the payload `T` — so it depends on neither the source, the
//! sink, nor the engine.
//!
//! The point of the abstraction is to make the *backend* swappable without
//! touching pipeline code:
//!
//! - In-process [`tokio` channels](https://docs.rs/tokio) (see `queue-channel`)
//!   for single-node runs. Acknowledgement is a no-op there — durability comes
//!   from the source (the replication slot), and the bounded channel gives
//!   backpressure.
//! - A durable broker (e.g. NATS JetStream) later, where [`AckHandle::ack`]
//!   becomes a real server acknowledgement and the queue itself is the
//!   durability boundary for the work pipeline.
//!
//! Because [`AckHandle`] models the ack the same way both back the same engine
//! loop: `recv` → process → `ack`.

mod error;
mod queue;

pub use error::*;
pub use queue::*;
