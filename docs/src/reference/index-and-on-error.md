# Index entries and on_error

An `[[index]]` entry names one index, points at its schema, and may override the deployment's item-rejection policy and its sinks' per-index options.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `name` | [Postgres identifier](identifiers.md) | — | The logical index name, the pipeline's stable identity. |
| `schema` | relative path ending `.yml`/`.yaml` | — | The `*.schema.yml`, resolved from the config file's directory. |
| `enabled` | bool | — | Whether this index is built on this run. A disabled index is left untouched at the sink. |
| `on_error` | `"stop"` \| `"skip"` | the global `on_error` | Per-index override of the per-document failure policy. |
| `[index.sinks.<name>]` | table per sink | none | This index's options on the sink named `<name>`. See [sinks](#sinks). |

All three of `name`, `schema`, and `enabled` are required.

## on_error

`on_error` governs **per-document failures** only: an **item-level rejection** (the sink applied a bulk request, but the destination refused one document: a mapping conflict, a value it can't index, an [id OpenSearch refuses](sink-opensearch.md#document-ids)), and a document a [script failed on](scripts.md#failures). A flush-wide failure (destination unreachable, whole request refused, or the OpenSearch sink's [target index deleted while running](sink-opensearch.md#seeding)) always stops the run and the batch is redelivered on restart.

| Value | Behavior |
| --- | --- |
| `"stop"` | Stop the run. The batch stays unconfirmed and is redelivered on restart, so a persistently bad document halts sync until the data or the policy changes. Dropping data is opt-in. This is the default. |
| `"skip"` | Quarantine the document and continue. The rest of the batch is applied and acked, so the slot advances past the poison. The event is logged, counted in `flusso.documents.quarantined`, and shown in `/status` as `documents_quarantined`. The document never lands until its source row changes again. |

The global key is the default for every index; an `[[index]]` entry's `on_error` overrides it. A single `stop` rejection halts the whole batch, decided before any quarantine event is emitted. The policy is operational, not part of the document shape, so changing it never triggers a reindex. The runbook is [Handle rejected documents](../operate/rejected-documents.md).

## sinks

`[index.sinks.<name>]` holds the options one sink takes for this index only. `<name>` is a sink name from `[sinks.<name>]`, not an adapter type, so two OpenSearch sinks can carry different options for the same index. What the table may hold is the sink adapter's business:

| Sink type | Per-index options |
| --- | --- |
| `opensearch` | `index_settings`. See [Sink: OpenSearch](sink-opensearch.md#index-settings). |
| `stdout` | none |

`build`, `check`, and `run` reject a `<name>` that isn't a declared sink, a sink whose adapter takes no per-index options, and any key the adapter doesn't know. Like `on_error`, these options are operational: they never change the index's content hash.

## Example

```toml
on_error = "stop"

[[index]]
name = "users"
schema = "users.schema.yml"
enabled = true

[[index]]
name = "analytics"
schema = "analytics.schema.yml"
enabled = true
on_error = "skip"

[index.sinks.primary]
index_settings = { "mapping.total_fields.limit" = 5000 }
```
