# flusso-sink

The sink port for `flusso`: where built documents land.

## Quick reference

| Item | Role |
| --- | --- |
| [`Sink`] | The trait every destination implements: `ensure_index`, `write` (safe for concurrent calls), and the seed hooks (`is_seeded`, `mark_seeded`, `reindex`) |
| [`SinkOptions`] | The universal `[sinks.<name>]` keys every sink engine honors (`backfill`) |
| [`Json`] | Renders a [`GenericValue`](kernel::GenericValue) as natural JSON with `serde_json` (numbers as numbers, a decimal with every digit, maps as objects) |
| [`to_json`] | The same rendering as a `serde_json::Value` tree, which holds numbers as `f64` |
| [`WriteReport`] / [`RejectedDocument`] | A write's outcome, including any documents the destination applied-but-rejected |
| [`Visibility`] | When a write's documents become *searchable*, as opposed to fetchable by id: at once, within a duration, or with a later caught-up write |
| [`SinkError`] / [`Result`] | The shared error type |

A sink consumes [`Envelope`](kernel::Envelope)s — the index, the operation, the document id, the built document, and the position of the change that produced it — so it depends on neither the source nor the engine and never builds a document itself. Each sink has its own engine (in `flusso-engine`) that receives batches from its lane, writes several of them at once (never two holding the same document), and acknowledges each once its write returned.

A sink only *reports* visibility. It is never asked to refresh because a reader is waiting; the sink engine turns the reports into the position a `searchable` fence waits on.

[`Json`] is the OpenSearch-flavored translation most sinks need at their boundary; a sink converts the canonical [`GenericValue`](kernel::GenericValue) into its own representation there. An *emitting* sink (stdout, later NATS) forwards the envelope with its document rendered the same way.
