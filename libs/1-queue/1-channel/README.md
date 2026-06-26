# flusso-queue-channel

An in-process work queue over a bounded `tokio` mpsc channel.

## At a glance

| | |
| --- | --- |
| **Constructor** | [`channel`]`(capacity)` → ([`ChannelProducer`], [`ChannelConsumer`]) |
| **Producer** | cloneable — publish from many tasks |
| **Consumer** | single — one per queue |
| **Backpressure** | `publish` waits when the channel is full |
| **Close** | drains and closes once every producer is dropped |
| **Ack** | no-op (nothing to make durable in memory) |

## How it works

[`channel`] returns a cloneable [`ChannelProducer`] and a single
[`ChannelConsumer`]. The bound gives backpressure — publishing waits when the
channel is full — and the queue closes and drains once every producer is
dropped.

Acknowledgement is a no-op: an in-memory channel has nothing to make durable.
Redelivery for the change pipeline comes from the source, not the channel —
the replication slot only advances once work is confirmed downstream. The
[`AckHandle`] surface is honored anyway, so the same engine loop runs unchanged
against a durable backend later.
