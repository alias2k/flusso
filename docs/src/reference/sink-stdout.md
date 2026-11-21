# Sink: stdout

A `[sinks.<name>]` table with `type = "stdout"` writes every operation to standard output as a JSON envelope, for development and for piping into `jq`.

{{#include generated/sink-stdout.md}}

Logs go to stderr, so stdout stays a clean data stream. When `flusso.toml` defines no sinks at all, `run` uses a stdout sink; `--pretty` then controls its formatting.

## Order

Lines are written one write at a time: [`[concurrency] writes`](config-toml.md#concurrency) doesn't apply to this sink. The order is **per index**:

- **Within one index**, lines come in the order that index's batches were started. A document's later version always comes after its earlier one, and a live change never overtakes a backfill row published before it. `seq` never goes down.
- **`seq` can repeat** within an index. Every line of one batch carries the same `seq`, and a change split over several builds (one touching more than [`[batch] max_changes`](config-toml.md#batch) documents of the index) gives several batches with the same `seq`.
- **Across indexes there is no order.** Each index is batched and built on its own, so a line of `orders` can come before a line of `users` with a lower `seq`.

**Tracking progress.** Compare `seq` as a number, and keep the last one read **per index**: one number for every index would skip lines of an index that is behind. An index's lines are complete up to *below* its last `seq`; more lines with that same `seq` may still follow, so resume from it inclusive.

- **Lines can come again.** Delivery is at least once: after an ingest engine restart, lines already printed can be printed again, with a newer `seq`. A line carries the whole document by `id`, so applying it twice is harmless.
- **`seq` counts within one process.** A new `flusso run` starts it again from the beginning, so it is no checkpoint across processes.

## The envelope

The kernel envelope every lane carries, written as-is with the document as JSON, so a consumer deserializes the same type. A NATS sink (#128) forwards the same shape.

| Field | Present on | Meaning |
| --- | --- | --- |
| `sink` | all | The sink's name in `flusso.toml` (`audit` below). |
| `version` | all | The envelope format version, `1`. Bumped when a field changes meaning. |
| `ts` | all | RFC 3339 timestamp: when the ingest engine built the document. |
| `seq` | live changes | The source position of the newest change in the line's batch, rendered as an opaque string. Never goes down within one index, may repeat, and has no order across indexes ([Order](#order)). Absent on a row published by a backfill or reindex snapshot. |
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
