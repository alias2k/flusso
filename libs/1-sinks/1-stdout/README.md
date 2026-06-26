# flusso-sinks-stdout

A [`Sink`] that writes each document operation to stdout as a JSON envelope — the development/debugging sink.

## At a glance

| | |
| --- | --- |
| **Output** | one JSON envelope per operation |
| **Format** | compact NDJSON (default), or pretty-printed |
| **Config key** | `pretty` — default `false` |

**Envelope fields:** `op` (`upsert`/`delete`), `id`, `index`, `document`,
`sink`, `version`, `ts`, `seq` (order), `meta` (`fields` count + serialized
`bytes`).

## What it does

Every operation becomes a self-describing JSON envelope — one NDJSON line by
default, pretty-printed when `pretty` is set — easy to watch or pipe into `jq`.
Alongside the operation, each envelope carries provenance and bookkeeping: which
sink and version produced it (`sink`, `version`), when (`ts`), in what order
(`seq`), and a quick `meta` summary of the document (top-level field count and
serialized byte size).

```text
{"document":{"email":"ada@x.io"},"id":"42","index":"users","meta":{"bytes":20,"fields":1},"op":"upsert","seq":1,"sink":"stdout","ts":"2026-06-03T10:20:30.123Z","version":"0.1.0"}
{"id":"7","index":"users","op":"delete","seq":2,"sink":"stdout","ts":"2026-06-03T10:20:30.124Z","version":"0.1.0"}
```
