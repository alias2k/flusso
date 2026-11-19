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

`on_error` governs **per-document failures** only: an **item-level rejection** (the sink applied a bulk request, but the destination refused one document: a mapping conflict, a value it can't index, an [id OpenSearch refuses](sink-opensearch.md#document-ids)), and a document that fails to build (a [script failed on it](scripts.md#failures), or an [`array: true`](field-types.md#array) column met an object element). Under `skip`, a document that fails to build keeps its previous indexed version (none on a first backfill) and the log says so; one the destination rejected is quarantined.

What isn't about the document never reaches `on_error`:

- **About one index:** the destination refuses writes to it for now (a [write block](sink-opensearch.md#write-blocks), a role without write permission on it), or its [target was deleted while running](sink-opensearch.md#seeding). That index's lane parks, or is staged again, and the other indexes keep being written. Nothing is quarantined.
- **About the destination:** unreachable, the whole request refused, credentials refused. The sink's run stops and the batch is redelivered on restart.
- **Busy** (HTTP 429 or 503): [resent](sink-opensearch.md#retries); a write that runs out of retries fails like the row above.

| Value | Behavior |
| --- | --- |
| `"stop"` | Park the index's lane: its batch stays unconfirmed, and its oldest batch is written again alone after 1 s, then with the wait doubling up to `max_restart_backoff` (60 s), until it lands. The index's other changes wait behind it; the other indexes keep being written. A persistently bad document halts that index until the data or the policy changes. Dropping data is opt-in. This is the default. |
| `"skip"` | Quarantine the document and continue. The rest of the batch is applied and acked, so the slot advances past the poison. The event is logged, counted in `flusso.documents.quarantined`, and shown in `/status` as `documents_quarantined`. The document never lands until its source row changes again. |

The global key is the default for every index; an `[[index]]` entry's `on_error` overrides it. A `stop` rejection parks only its own index's lane; in a write holding several indexes, the others land and a `skip` index's rejections are quarantined. The policy is operational, not part of the document shape, so changing it never triggers a reindex. The runbook is [Handle rejected documents](../operate/rejected-documents.md).

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
