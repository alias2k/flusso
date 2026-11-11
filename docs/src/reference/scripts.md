# Script fields

A `script:` turns a leaf field into a value computed from the built document by a [Rhai](https://rhai.rs) script; `emit: false` makes any field input-only, so a script can read what the document never carries.

## script

`script` is a sibling of a leaf type key: `text`, `identifier`, `keyword`, `enum`, `uuid`, `boolean`, `short`, `integer`, `long`, `float`, `double`, `decimal`, `date`, `timestamp`. The type key names the result's type, exactly as for a column field: the mapping, the [query derive](../query/document-structs.md) and the designer all read it.

| Sibling | Required | Meaning |
| --- | --- | --- |
| `script` | yes | The Rhai source. |
| `required` | yes | `true` maps the field non-null; the script must then never return `()`. |
| `array` | no | `true`: the script returns an array of the field's type, mapped like an `ids` aggregate. Only with `script`. |
| `variants` | no | On `enum:`: the declared variants, in rank order. The script must return one of them. |
| `subfields` | no | On `text`/`keyword`/`uuid`: as for a column field ([field types](field-types.md#subfields)). |
| `options` | no | Extra mapping properties. |

`column`, `transforms`, `default` and `indexed` are rejected beside `script`: it reads no column. Write a default as `this.x ?? "fallback"`.

```yaml
- keyword: label
  required: true
  script: |
    `${this.sku} — ${this.name}`

- has_many: variants
  table: variants
  foreign_key: product_id
  primary_key: id
  fields:
    - decimal: price
      required: true
    - decimal: gross
      required: true
      script: this.price * root.vatRate
```

## What a script sees

| Name | Value |
| --- | --- |
| `this` | The object at the field's level, as built: the root document, a group, or one element of a `has_many`/`many_to_many` (the script runs once per element). |
| `root` | The whole document, as built. |

Nothing else: no database, no clock, no randomness, no other script's output. A script reads the document *before* any script runs, so a script field read through `this` or `root` is `()`. Writing to `this` or `root` doesn't reach the document or another script.

Because a script reads only the built document, it adds no dependency: a change to any column it reads is already a change to a field the document reads, so [reverse resolution](joins.md) and the [skip on unread columns](source-postgres.md#updates-that-change-nothing-a-document-reads) are unchanged.

## emit

`emit: false` is accepted on every field, joins and aggregates included.

| Value | Meaning |
| --- | --- |
| `true` | In the document and the mapping. The default. |
| `false` | Input-only: fetched and built like any field, readable by scripts, then stripped. Not in the document, the mapping, or the query derive. |

Unlike [`indexed: false`](joins.md#indexed), which keeps content in `_source` without indexing it, an input-only field reaches no sink at all.

## Values in and out

A document value enters Rhai as:

| Document | Rhai |
| --- | --- |
| integers | `i64` |
| `float`, `double` | `f64` |
| `decimal` | `Decimal`, exact |
| text, `uuid` | string |
| `date`, `timestamp` | RFC 3339 string (`2024-05-01`, `2024-05-01T12:00:00Z`) |
| `binary` | blob |
| object, array | object map, array |
| null | `()` |

The result is checked against the declared type, strictly. The one widening allowed: an integer for a `float`, `double` or `decimal` field (a `float` result is stored as 32-bit). A `date` result must be `YYYY-MM-DD`; a `timestamp` result an RFC 3339 timestamp, with or without the offset. A `uuid` result must parse as a uuid. `()` is null, accepted only when `required: false`; an array holds no nulls.

## The sandbox

| Rule | Why |
| --- | --- |
| No clock, no imports, no `eval` | A rebuild must produce the same value: delivery is at-least-once. |
| An undefined variable fails to compile | A typo is caught by `flusso check --offline`, not in production. |
| Reading a missing property fails | `this.nmae` is an error, not a silent `()`. Test with `"name" in this`. |
| 1,000,000 operations, 32 call levels, 64 expression levels | A runaway script fails its document instead of stalling the pipeline. |
| Strings up to 1 MiB, arrays and maps up to 100,000 entries | |

`print` and `debug` go to the log at `debug` under the `script` target (`RUST_LOG=script=debug`), tagged with the index, the document id and the field, and to the output of [`flusso script run`](cli.md#script).

## Failures

A script that fails to compile fails `build`, `check` (with `--offline` too), `run` and the designer's preview, listing every broken script with its line, column and source line.

A script that fails on one document (a runtime error, or a result of the wrong type) fails that document, and the index's [`on_error`](index-and-on-error.md#on_error) decides:

| `on_error` | A failed document |
| --- | --- |
| `"stop"` | Stops the ingest engine before anything in the batch is published. The change is retried after the restart and fails again until the script or the data changes. |
| `"skip"` | Is published to no sink, so every sink keeps its indexed version. |

Either way the failure is logged at `error` with the script line and a caret (the first per index and field each minute, carrying a count of the ones suppressed since), counted in [`flusso.script.failures`](metrics.md), and shown in [`/status`](http.md#get-status) under `script_failures`. The document as built is logged at `debug` under the `script` target, since it may hold personal data.

## Content hash

The script source, `array`, and `emit: false` are part of the [content hash](sink-opensearch.md#index-naming): editing a script rebuilds the index into a new generation. A schema that uses none of them keeps its hash.
