# flusso-sink-stdout

A [`Sink`] that writes each document operation to stdout as a JSON envelope — the development/debugging sink.

## At a glance

| | |
| --- | --- |
| **Output** | one JSON envelope per operation |
| **Format** | compact NDJSON (default), or pretty-printed |
| **Config** | [`StdoutConfig`]: `pretty` — default `false` |

**Envelope fields:** `op` (`upsert`/`delete`), `id`, `index`, `document`,
`sink` (the configured sink name), `version` (envelope format), `ts`, `seq`
(the source position, absent on snapshot rows), `meta` (`fields` count +
serialized `bytes`).

## What it does

Every operation becomes a self-describing JSON envelope — one NDJSON line by
default, pretty-printed when `pretty` is set — easy to watch or pipe into `jq`.
It is the kernel `Envelope` as-is — the document translated to JSON — so a
consumer deserializes the same type. Alongside the operation, each envelope
carries provenance and bookkeeping: which sink emitted it (`sink`, the name in
`flusso.toml`) and which envelope format (`version`), when it was built (`ts`),
the source position of the change (`seq`, an opaque string; absent on a row a
backfill or reindex snapshot produced), and a quick `meta` summary of the
document (top-level field count and serialized byte size).

```text
{"sink":"audit","version":1,"ts":"2026-06-03T10:20:30.123Z","seq":"1","index":"users","op":"upsert","id":"42","meta":{"fields":1,"bytes":20},"document":{"email":"ada@x.io"}}
{"sink":"audit","version":1,"ts":"2026-06-03T10:20:30.124Z","seq":"2","index":"users","op":"delete","id":"7"}
```
