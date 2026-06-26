# 1-queue — the work-queue layer

This layer is the queue abstraction (`0-core`) plus its in-process backends. The
engine drains the source into the queue and a worker pulls items back out, so a full
queue back-pressures the producer for free.

- [queue-core](0-core) — the abstraction: a payload-agnostic `Producer`/`Consumer` pair, with each item delivered as a `Delivery` carrying an `AckHandle` so the same `recv` → process → `ack` loop works over any backend.
- [queue-channel](1-channel) — bounded tokio mpsc backend: an in-process queue where the bound supplies back-pressure and acknowledgement is a no-op (durability comes from the source's replication slot, not the channel).

Part of [the flusso library crates](../README.md).
