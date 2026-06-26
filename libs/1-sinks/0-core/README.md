# flusso-sinks-core

The sink abstraction for `flusso`.

A *sink* is where assembled documents go. It consumes neutral values — an
[`IndexName`](schema_core::IndexName), a string id, and a document
[`GenericValue`](schema_core::GenericValue) — so it depends on neither the
source nor the engine; the engine is what feeds it.

- [`Sink`] is the trait every destination implements.
- [`to_json`] renders a [`GenericValue`](schema_core::GenericValue) as
  natural JSON (numbers as numbers, maps as objects), which most sinks need.
- [`SinkError`] / [`Result`] are the shared error type.
