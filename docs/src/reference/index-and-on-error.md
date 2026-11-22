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

`on_error` decides what one bad **document** does. It covers two failures:

- **An item-level rejection.** The destination took the write but refused one document: a mapping conflict, a value it can't index, an [id OpenSearch refuses](sink-opensearch.md#document-ids).
- **A document that fails to build.** A [script failed on it](scripts.md#failures), or an [`array: true`](field-types.md#array) column met an object element.

| Value | What happens to the document | What happens to its index |
| --- | --- | --- |
| `"stop"` (default) | Nothing is dropped. A rejected document's batch stays unconfirmed; a document that fails to build is retried after a wait, doubling up to 60 s by default. With two or more indexes only its index stops meanwhile ([A failing index](config-toml.md#a-failing-index)); with one, the ingest engine restarts. | A rejection **parks** the index's lane on that sink: it writes the same batch again alone after 1 s, then with the wait doubling up to 60 s, until it lands. The index's later changes wait behind it; the other indexes keep being written. See [A lane is stalled](../operate/stalled-lane.md). |
| `"skip"` | A rejected document is **quarantined**: logged, counted in `flusso.documents.quarantined`, shown in `/status` as `documents_quarantined`. A document that fails to build keeps its previous indexed version (none on a first backfill). It doesn't land until its source row changes again. | Keeps going: the rest of the batch is applied and acknowledged, and the slot moves past the bad document. |

The top-level `on_error` is the default for every index; an `[[index]]` entry's `on_error` overrides it. In a write holding several indexes, each is decided by its own policy: a `stop` rejection parks only its own lane, the others land, and a `skip` index's rejections are quarantined. The policy is operational, not part of the document shape, so changing it never triggers a reindex. The runbook is [Handle rejected documents](../operate/rejected-documents.md).

### What never reaches on_error

Failures that aren't the document's fault are handled the same way under either policy, and nothing is quarantined:

| Failure | Example | What happens |
| --- | --- | --- |
| The index refuses writes for now | a [write block](sink-opensearch.md#write-blocks), a role without write permission on that index | The lane parks as `blocked` and is probed with the same backoff; it resumes once the refusal is lifted. |
| The index's target is gone | its generation [deleted while flusso runs](sink-opensearch.md#seeding) | That index alone is staged again and backfilled; the other indexes keep being written. |
| The destination is busy | HTTP 429 or 503, including the flood-stage disk block | [Resent](sink-opensearch.md#retries); a write that runs out of retries fails like the row below. |
| The destination fails the whole write | unreachable, credentials refused, the whole request refused | The sink's run stops and restarts with backoff; the batch is redelivered. Every index on that sink waits. |

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
