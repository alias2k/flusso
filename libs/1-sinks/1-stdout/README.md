# flusso-sinks-stdout

A [`Sink`] that writes documents to standard output as JSON.

Each operation is emitted as a JSON envelope — one line of NDJSON by
default, or pretty-printed when configured — which makes the pipeline's
output easy to watch or pipe into `jq` during development.

Alongside the operation itself, every envelope carries provenance and
bookkeeping so a stream is self-describing: which sink and version produced
it (`sink`, `version`), when (`ts`), in what order (`seq`), and a quick
`meta` summary of the document (top-level field count and serialized byte
size).

```text
{"document":{"email":"ada@x.io"},"id":"42","index":"users","meta":{"bytes":20,"fields":1},"op":"upsert","seq":1,"sink":"stdout","ts":"2026-06-03T10:20:30.123Z","version":"0.1.0"}
{"id":"7","index":"users","op":"delete","seq":2,"sink":"stdout","ts":"2026-06-03T10:20:30.124Z","version":"0.1.0"}
```
