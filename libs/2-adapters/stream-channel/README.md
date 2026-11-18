# flusso-stream-channel

The in-process stream adapter: one bounded `tokio` mpsc channel per sink lane plus one for requests, configured by the `[stream]` table with `type = "channel"` ([`ChannelConfig`], one option: `capacity`).

## At a glance

| | |
| --- | --- |
| **Constructor** | [`ChannelStream::new`]`(capacity, sinks)` — one lane per sink name, fixed for the run |
| **Lanes** | [`lane`](stream::Stream::lane) hands out a producer + consumer over the *same* channel every time, so a restarted engine re-attaches |
| **Requests** | [`requests`](stream::Stream::requests): the upward lane sink engines put `Backfill` requests on |
| **Backpressure** | `publish` waits when a lane is full — the slowest sink paces ingest |
| **Redelivery** | every delivered item stays in an in-flight ledger until acked; nacked or dropped unacked → that delivery alone redelivered, in lane order |
| **Watermark** | [`watermark`](stream::Stream::watermark) = over every lane, the last position before its oldest unacknowledged positioned item |

## How it works

Each lane is a bounded channel with a small ledger beside it: the items
delivered but not yet acknowledged, in order, each under a ticket its ack
handle carries. Several deliveries can be outstanding at once — the ingest
engine holds every coalesced backfill request until the snapshot is
published, and a sink engine writes several lane items at once — and each
acknowledges on its own, in any order. Acking removes the item; nacking a
delivery, or dropping its handle unacked (an engine that died holding it),
marks that delivery alone due: the next `recv` serves the earliest due one
again before pulling anything new, and the deliveries still in flight are
left alone. A released delivery can therefore come back after later ones were
acknowledged: the channel keeps lane order, and the sink engine keeps the
order of each document by never writing two items sharing one at once. A
restarted engine gets a fresh consumer over the same channel and is
redelivered what its predecessor left.

The stream's [`watermark`](stream::Stream::watermark) is the minimum over
lanes of the last position each lane has behind its oldest unacknowledged
positioned item: every entry records, when it is first delivered, the position
delivered before it. Items are acknowledged in any order (the sink engine
writes several at once), so the highest acknowledgement would pass an earlier
item still being written. Snapshot batches carry no position and never move
it. The ingest engine hands the watermark to the
source as its confirmation, which is what ties every sink's durability into
one resume point.

Bounded lanes are the trade: a stalled sink fills its lane and eventually
blocks the ingest engine, pinning WAL until it recovers. A broker-backed
stream adapter removes that coupling behind the same [`Stream`](stream::Stream)
port.
