# flusso-stream-channel

The in-process stream adapter: one bounded `tokio` mpsc channel per sink lane plus one for requests, configured by the `[stream]` table with `type = "channel"` ([`ChannelConfig`], one option: `capacity`).

## At a glance

| | |
| --- | --- |
| **Constructor** | [`ChannelStream::new`]`(capacity, sinks)` — one lane per sink name, fixed for the run |
| **Lanes** | [`lane`](stream::Stream::lane) hands out a producer + consumer over the *same* channel every time, so a restarted engine re-attaches |
| **Requests** | [`requests`](stream::Stream::requests): the upward lane sink engines put `Backfill` requests on |
| **Backpressure** | `publish` waits when a lane is full — the slowest sink paces ingest |
| **Redelivery** | every delivered item stays in an in-flight ledger until acked; nacked or dropped unacked → redelivered in order |
| **Watermark** | [`watermark`](stream::Stream::watermark) = the lowest position every lane has acked |

## How it works

Each lane is a bounded channel with a small ledger beside it: the items
delivered but not yet acknowledged, in order, each under a ticket its ack
handle carries. Several deliveries can be outstanding at once — the ingest
engine holds every coalesced backfill request until the snapshot is published
— and each acknowledges on its own. Acking removes the item and, for a lane
batch, records its position; nacking a delivery, or dropping its handle
unacked (an engine that died holding it), moves the redelivery cursor back so
the next `recv` serves it again before pulling anything new. A restarted
engine gets a fresh consumer over the same channel and is redelivered what
its predecessor left.

The stream's [`watermark`](stream::Stream::watermark) is the minimum over
lanes of each lane's last acknowledged position; snapshot batches carry no
position and never move it. The ingest engine hands the watermark to the
source as its confirmation, which is what ties every sink's durability into
one resume point.

Bounded lanes are the trade: a stalled sink fills its lane and eventually
blocks the ingest engine, pinning WAL until it recovers. A broker-backed
stream adapter removes that coupling behind the same [`Stream`](stream::Stream)
port.
