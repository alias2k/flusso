# flusso-sink

The sink port for `flusso`: where built documents land.

## Quick reference

| Item | Role |
| --- | --- |
| [`Sink`] | The trait every destination implements: `ensure_index`, `apply`, `flush`, and the seed hooks (`is_seeded`, `mark_seeded`, `reindex`) |
| [`SinkOptions`] | The universal `[sinks.<name>]` keys every sink engine honors (`backfill`) |
| [`to_json`] | Renders a [`GenericValue`](kernel::GenericValue) as natural JSON (numbers as numbers, maps as objects) |
| [`FlushReport`] / [`RejectedDocument`] | A flush's outcome, including any documents the destination applied-but-rejected |
| [`Visibility`] | When a flush's documents become *searchable*, as opposed to fetchable by id: at once, within a duration, or with a later flush |
| [`SinkError`] / [`Result`] | The shared error type |

A sink consumes [`Envelope`](kernel::Envelope)s — the index, the operation, the document id, the built document, and the position of the change that produced it — so it depends on neither the source nor the engine and never builds a document itself. Each sink has its own engine (in `flusso-engine`) that receives batches from its lane, applies them, flushes once per batch, and acknowledges.

A sink only *reports* visibility. It is never asked to refresh because a reader is waiting; the sink engine turns the reports into the position a `searchable` fence waits on.

[`to_json`] is the OpenSearch-flavored translation most sinks need at their boundary; a sink converts the canonical [`GenericValue`](kernel::GenericValue) into its own representation there. An *emitting* sink (stdout, later NATS) forwards the envelope as-is instead.
