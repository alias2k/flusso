# flusso-sinks-core

The sink abstraction for `flusso` — where assembled documents land.

## Quick reference

| Item | Role |
| --- | --- |
| [`Sink`] | The trait every destination implements |
| [`FanOutSink`] | A [`Sink`] that broadcasts each write to several sinks |
| [`to_json`] | Renders a [`GenericValue`](schema_core::GenericValue) as natural JSON (numbers as numbers, maps as objects) |
| [`FlushReport`] / [`RejectedDocument`] | A flush's outcome, including any documents the destination applied-but-rejected |
| [`SinkError`] / [`Result`] | The shared error type |

A sink consumes neutral values — an [`IndexName`](schema_core::IndexName), a string id, and a document [`GenericValue`](schema_core::GenericValue) — so it depends on neither the source nor the engine. The engine is what feeds it.

[`to_json`] is the OpenSearch-flavored translation most sinks need at their boundary; a sink converts the canonical [`GenericValue`](schema_core::GenericValue) into its own representation there.
