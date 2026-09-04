# Sink: stdout

A `[sinks.<name>]` table with `type = "stdout"` writes every operation to standard output as a JSON envelope, for development and for piping into `jq`.

{{#include generated/sink-stdout.md}}

Logs go to stderr, so stdout stays a clean data stream. When `flusso.toml` defines no sinks at all, `run` uses a stdout sink; `--pretty` then controls its formatting.

## The envelope

The kernel envelope every lane carries, written as-is with the document as JSON, so a consumer deserializes the same type. A NATS sink (#128) forwards the same shape.

| Field | Present on | Meaning |
| --- | --- | --- |
| `sink` | all | The sink's name in `flusso.toml` (`audit` below). |
| `version` | all | The envelope format version, `1`. Bumped when a field changes meaning. |
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
{"sink":"audit","version":1,"ts":"2026-06-03T10:20:30.123Z","seq":"1","index":"users","op":"upsert","id":"42","meta":{"fields":1,"bytes":20},"document":{"email":"ada@x.io"}}
{"sink":"audit","version":1,"ts":"2026-06-03T10:20:30.124Z","seq":"2","index":"users","op":"delete","id":"7"}
```
