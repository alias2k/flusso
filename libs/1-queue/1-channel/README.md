# flusso-queue-channel

An in-process work queue over a bounded `tokio` mpsc channel.

[`channel`] returns a cloneable [`ChannelProducer`] and a single
[`ChannelConsumer`]. The bound gives backpressure (publishing waits when the
channel is full); the queue closes and drains once every producer is
dropped.

Acknowledgement is a no-op: an in-memory channel has nothing to make
durable, so redelivery for the change pipeline comes from the source (the
replication slot only advances once work is confirmed downstream), not the
channel. The [`AckHandle`] surface is still honored so the same engine loop
works unchanged against a durable backend later.
