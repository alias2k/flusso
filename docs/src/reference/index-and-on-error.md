# Index entries and on_error

An `[[index]]` entry names one index, points at its schema, and may override the deployment's item-rejection policy.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `name` | [Postgres identifier](identifiers.md) | — | The logical index name, the pipeline's stable identity. |
| `schema` | relative path ending `.yml`/`.yaml` | — | The `*.schema.yml`, resolved from the config file's directory. |
| `enabled` | bool | — | Whether this index is built on this run. A disabled index is left untouched at the sink. |
| `on_error` | `"stop"` \| `"skip"` | the global `on_error` | Per-index override of the item-rejection policy. |

All three of `name`, `schema`, and `enabled` are required.

## on_error

`on_error` governs **item-level rejections** only: the sink applied a bulk request, but the destination refused one document (a mapping conflict, a value it can't index). A flush-wide failure (destination unreachable, whole request refused) always stops the run and the batch is redelivered on restart.

| Value | Behavior |
| --- | --- |
| `"stop"` | Stop the run. The batch stays unconfirmed and is redelivered on restart, so a persistently bad document halts sync until the data or the policy changes. Dropping data is opt-in. This is the default. |
| `"skip"` | Quarantine the document and continue. The rest of the batch is applied and acked, so the slot advances past the poison. The event is logged, counted in `flusso.documents.quarantined`, and shown in `/status` as `documents_quarantined`. The document never lands until its source row changes again. |

The global key is the default for every index; an `[[index]]` entry's `on_error` overrides it. A single `stop` rejection halts the whole batch, decided before any quarantine event is emitted. The policy is operational, not part of the document shape, so changing it never triggers a reindex. The runbook is [Handle rejected documents](../operate/rejected-documents.md).

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
```
