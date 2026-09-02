# Sink: stdout

A `[sinks.<name>]` table with `type = "stdout"` writes every operation to standard output as a JSON envelope, for development and for piping into `jq`.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `type` | `"stdout"` | — | Required. |
| `pretty` | bool | `false` | Pretty-print each envelope instead of one compact line (NDJSON). |

Logs go to stderr, so stdout stays a clean data stream. When `flusso.toml` defines no sinks at all, `run` uses a stdout sink; `--pretty` then controls its formatting.

## The envelope

| Field | Present on | Meaning |
| --- | --- | --- |
| `sink` | all | `"stdout"` |
| `version` | all | The flusso version that wrote it. |
| `ts` | all | RFC 3339 timestamp. |
| `seq` | all | Monotonic sequence number within the run. |
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
{"document":{"email":"ada@x.io"},"id":"42","index":"users","meta":{"bytes":20,"fields":1},"op":"upsert","seq":1,"sink":"stdout","ts":"2026-06-03T10:20:30.123Z","version":"0.16.0"}
{"id":"7","index":"users","op":"delete","seq":2,"sink":"stdout","ts":"2026-06-03T10:20:30.124Z","version":"0.16.0"}
```
