# flusso-queue-core

A generic work-queue abstraction for the sync pipeline — domain-agnostic, so the backend swaps without touching pipeline code.

## Quick reference

| Item | Role |
| --- | --- |
| [`Producer`] | Publishes work items |
| [`Consumer`] | Receives them, one [`Delivery`] at a time |
| [`Delivery`] | A received item plus its [`AckHandle`] |
| [`AckHandle`] | Confirms (or returns) a delivery — [`ack`](AckHandle::ack) / `nack` |

Generic over the payload `T`, so it depends on neither the source, the sink, nor the engine. Both backends drive the same engine loop: `recv` → process → `ack`.

The point of the abstraction is a swappable *backend*:

- **In-process** [`tokio` channels](https://docs.rs/tokio) (see `queue-channel`) for single-node runs. Acking is a no-op there — durability comes from the source (the replication slot), and the bounded channel gives backpressure.
- **A durable broker** (e.g. NATS JetStream) later, where [`AckHandle::ack`] becomes a real server acknowledgement and the queue itself is the durability boundary for the work pipeline.

> ℹ️ **Info** — because [`AckHandle`] models the ack identically, the engine loop is unchanged whichever backend backs it; only the durability guarantee shifts.
