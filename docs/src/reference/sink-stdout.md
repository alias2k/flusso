# Sink: stdout

A `[sinks.<name>]` table with `type = "stdout"` writes every operation to standard output as a JSON envelope, for development and for piping into `jq`.

{{#include generated/sink-stdout.md}}

Logs go to stderr, so stdout stays a clean data stream. When `flusso.toml` defines no sinks at all, `run` uses a stdout sink; `--pretty` then controls its formatting.

## The envelope

The kernel envelope every lane carries, written as-is plus the emitting sink's name, the flusso version, a `seq`, and a `meta` summary. A NATS sink (#128) forwards the same shape.

| Field | Present on | Meaning |
| --- | --- | --- |
| `sink` | all | `"stdout"` |
| `version` | all | The flusso version that wrote it. |
| `ts` | all | RFC 3339 timestamp: when the ingest engine built the document. |
| `seq` | live changes | The source position of the change, rendered as an opaque string. Monotonic within a run. Absent on a row published by a backfill or reindex snapshot. |
| `index` | all | Logical index name. |
| `op` | all | `upsert` or `delete`. |
| `id` | all | Document id, the root primary key as a string. |
| `meta.fields`, `meta.bytes` | `upsert` | Top-level field count and serialized size. |
| `document` | `upsert` | The assembled document. |

## Example

```toml
[sinks.audit]
type = "stdout"
pretty = false
```

```text
{"document":{"email":"ada@x.io"},"id":"42","index":"users","meta":{"bytes":20,"fields":1},"op":"upsert","seq":"1","sink":"stdout","ts":"2026-06-03T10:20:30.123Z","version":"0.16.0"}
{"id":"7","index":"users","op":"delete","seq":"2","sink":"stdout","ts":"2026-06-03T10:20:30.124Z","version":"0.16.0"}
```
