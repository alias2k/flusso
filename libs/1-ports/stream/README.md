# flusso-stream

The stream port: what sits between the ingest engine and the sink engines. One **lane** per sink carries batches of envelopes down; one **request lane** carries backfill requests up; the stream owns the **watermark** that turns every lane's acknowledgements into the one position the source may confirm.

## Quick reference

| Item | Role |
| --- | --- |
| [`Stream`] | The port: `lane(sink)`, `requests()`, `watermark()` |
| [`LaneItem`] | What a lane carries: a [`Batch`] of envelopes, or a `SnapshotComplete` marker |
| [`Request`] | What goes up: `Backfill { sink, indexes }` |
| [`Producer`] / [`Consumer`] | The generic publish/receive pair both directions are built from |
| [`Delivery`] / [`AckHandle`] | A received item plus the handle that confirms it; acking a batch moves its lane's watermark |

## The contract

- **Batches, not envelopes.** A lane item is the set of envelopes one ingest commit built, in build order, with the position of the last change it covers. Acknowledging it confirms that position for that lane; contiguity is free because batches are published in order. A live batch with no documents still carries its position. A snapshot batch carries none, and a `SnapshotComplete` marker follows the last one.
- **The watermark is the minimum over lanes** of each lane's last acknowledged position. The ingest engine reads it after every commit and hands it to the source as confirmation, so the replication slot advances only past what *every* sink holds.
- **Requests are at-least-once.** The ingest engine acknowledges a `Backfill` only after publishing the `SnapshotComplete` that ends it; a crash in between redelivers the request.
- **Re-attachable ends.** `lane` and `requests` return fresh ends over the same lane each call. A restarted engine picks up where the last one stopped, including the item it left unacknowledged.
- **Backpressure is the in-process trade.** With the bounded channel adapter (`stream-channel`), a full lane blocks the ingest engine, so the slowest sink paces ingest. A broker-backed adapter is what removes that.

> ℹ️ **Info** — the port is generic over the payload, so the same `Producer`/`Consumer` pair carries envelopes down and requests up. A broker-backed adapter implements the same three methods; nothing in the engines changes.
