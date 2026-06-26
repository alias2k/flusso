# 1-queue — the work-queue layer

The queue that sits between capture and the worker: `0-core` is the abstraction, the higher-numbered crates are its backends.

| Crate | Path | Role |
| --- | --- | --- |
| queue-core | [0-core](0-core) | Payload-agnostic `Producer`/`Consumer` pair; each item arrives as a `Delivery` carrying an `AckHandle`. |
| queue-channel | [1-channel](1-channel) | In-process backend over a bounded tokio mpsc channel. |

The engine drains the source into the queue and a worker pulls items back out, so a full queue back-pressures the producer for free. The same `recv` → process → `ack` loop works over any backend.

In `queue-channel` the bound supplies the back-pressure and `ack` is a no-op — durability comes from the source's replication slot, not the channel.

Part of [the flusso library crates](../README.md).
