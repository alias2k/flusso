---
status: proposed
---

# The stream owns the slot watermark and carries requests upward

The stream port is bidirectional. Downward, one lane per sink carries envelopes that include the source position; each lane's acknowledgements feed a watermark, and the stream exposes the lowest position every lane has acknowledged, which the ingest engine forwards to the source as confirmation. Upward, a request lane carries backfill and reindex requests from sink engines to the ingest engine over the same generic producer/consumer traits. Decided in #130. The alternative, a multiplexer inside the source port, stopped fitting once documents were built before the stream (ADR 0002): the source never sees lanes, and a stacked sink's late acknowledgement arrives via another sink's engine, so only the stream can tie every acknowledgement of one position together.

## Considered options

- **Source-side multiplexer confirming the minimum across consumers.** Works only if the source sees every consumer directly; a chained lane breaks that.
- **Daemon-mediated control path** (a handle from each sink engine into the ingest engine). Simpler, but requests are lost on a crash, cannot be batched, and give the daemon a relay role it should not have.
- **Stream-owned watermark plus a request lane (chosen).** Positions and requests both survive whatever the adapter makes durable; coalescing is ordinary batching on the request lane.

## Consequences

- A backfill request is acknowledged only after the snapshot it triggered is fully published, so a crash mid-backfill redelivers it. Concurrent requests for the same index coalesce into one snapshot fanned to every requesting lane; ten reindex requests at once cost one pass over the table.
- The name "stream" is flusso's middle port. Where NATS's own stream must be distinguished, it is called a "JetStream stream".
- A lane carries batches, not single envelopes: one batch per ingest commit, in build order, carrying the position of the last change it covers, plus a snapshot-complete marker after the last batch of a snapshot. Acknowledging a batch confirms its position, so contiguity is trivial and a change that resolves to no document still confirms through its empty batch. A broker-backed adapter may split a batch on the wire and reassemble it.
- `Position` is a kernel `u64`, monotonic within one run and assigned by the source, which keeps the mapping back to its own offset (an LSN). Snapshot batches carry no position and never move the watermark.
- The in-process adapter implements the watermark in memory over per-sink channels and redelivers an unacknowledged batch to a restarted consumer. A broker-backed adapter reads its consumers' acknowledgement floors. Nothing in the engines changes between the two.
