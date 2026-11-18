# flusso-stream

The stream port: what sits between the ingest engine and the sink engines. One **lane** per sink carries batches of envelopes down; one **request lane** carries backfill requests up. What each lane has landed is not the stream's: the engine's `Progress` keeps it and the source confirms from it.

## Quick reference

| Item | Role |
| --- | --- |
| [`Stream`] | The port: `lane(sink)`, `requests()`, `is_idle()` |
| [`LaneItem`] | What a lane carries: a [`Batch`] of envelopes, or a `SnapshotComplete` marker |
| [`Request`] | What goes up: `Backfill { sink, indexes }` |
| [`Producer`] / [`Consumer`] | The generic publish/receive pair both directions are built from |
| [`Delivery`] / [`AckHandle`] | A received item plus the handle that confirms it; acking a batch ends its redelivery |

## The contract

- **Batches, not envelopes.** A lane item is the set of envelopes one ingest commit built, in build order, with the position of the last change it covers. Acknowledging it confirms that position for that lane; contiguity is free because batches are published in order. A live batch with no documents still carries its position. A snapshot batch carries none, and a `SnapshotComplete` marker follows the last one.
- **Confirmation is not the stream's.** The ingest engine registers what it publishes in the engine's `Progress`, each sink engine reports what landed, and the source is confirmed from that, so the replication slot advances only past what *every* sink holds.
- **Requests are at-least-once.** The ingest engine acknowledges a `Backfill` only after publishing the `SnapshotComplete` that ends it; a crash in between redelivers the request.
- **Re-attachable ends.** `lane` and `requests` return fresh ends over the same lane each call. A restarted engine picks up where the last one stopped, including the item it left unacknowledged.
- **Backpressure is the in-process trade.** With the bounded channel adapter (`stream-channel`), a full lane blocks the ingest engine, so the slowest sink paces ingest. A broker-backed adapter is what removes that.

> ℹ️ **Info** — the port is generic over the payload, so the same `Producer`/`Consumer` pair carries envelopes down and requests up. A broker-backed adapter implements the same three methods; nothing in the engines changes.
