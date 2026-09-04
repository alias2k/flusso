# Sink: OpenSearch

A `[sinks.<name>]` table with `type = "opensearch"`: the connection and bulk options, and how the sink names, maps, refreshes, and seeds the indexes it owns.

{{#include generated/sink-opensearch.md}}

`<NAME>` in the override variables is the uppercased sink name: `[sinks.primary]` reads `PRIMARY_OPENSEARCH_URL`. Precedence is in [Environment variables](environment.md#config-values).

## Cluster requirements

| Requirement | Detail |
| --- | --- |
| OpenSearch 2.x | The query side also speaks Elasticsearch 7.x. |
| A reachable HTTP(S) endpoint | The `url`. |
| A user that can create, alias, write, and delete flusso's indexes | Plus the small `{prefix}flusso_meta` index. |
| `analysis-icu` on every node | Only with `text_analysis = "icu"`. |

## Index naming

| Name | Is | Purpose |
| --- | --- | --- |
| `{prefix}{logical}_{hash}` | alias | The **hash alias**. What flusso and `flusso-query` address. The hash derives from the schema. |
| `{prefix}{logical}_{hash}_{n}` | index | A **generation**: the concrete index holding data. `n` increments on reindex. |
| `{prefix}{logical}` | alias | The **convenience alias** for humans and dashboards. Best-effort: if a real index already owns the name, flusso warns and carries on. |
| `{prefix}flusso_meta` | index | One doc per index recording its active generation and seeded state. |

A structural schema change moves the hash, so the new shape gets a fresh alias and generation, re-seeded from scratch, rather than colliding with the old mapping. An on-demand reindex builds generation `n+1` behind the same hash alias while `n` keeps serving, then the alias flips atomically and `n` is dropped. See [Reindex without downtime](../operate/reindex.md).

## Mapping

Each index is created up front from the resolved schema mapping with `dynamic: strict`: types come from the schema, and a document with an unmapped key is rejected. An index that already exists is left as is. The `on_error` policy decides what an item-level rejection does; see [Index entries and on_error](index-and-on-error.md#on_error).

## Refresh

| Phase | `refresh_interval` | Why |
| --- | --- | --- |
| Backfill | `-1` | Bulk seeding without per-flush refresh churn. |
| Seeded, backlog draining | the configured value | Bounds staleness under sustained load. |
| Seeded, caught up | immediate refresh on each flush | Search is fresh when traffic is light. |

The configured value therefore only matters while a backlog is draining. Raise it for throughput under load; lower it toward `1s` for fresher reads while behind.

## Seeding

Seeded state is recorded in `flusso_meta`, so a restart skips a completed backfill. On every start the marker is checked against the generation it names: if that index is gone, the marker is retracted with a warning, the generation is recreated empty, and the backfill refills it. Deleting `{logical}_{hash}_{n}` between runs is the supported way to force a rebuild of one index. A missing replication slot overrides every marker; see [Recover from a dropped slot](../operate/dropped-slot.md).

## Analysis and subfields

Every index defines three `flusso_*` analysis components.

| Name | Kind | Does |
| --- | --- | --- |
| `flusso_text` | analyzer | Natural language: tokenize, lowercase, fold accents. The default for `text`. |
| `flusso_code` | analyzer | Identifier-like text: splits on punctuation, case, and letter/digit boundaries, then lowercases and folds. `C-01234` is found by `C01234`, `c-01234`, or `01234`. The analyzer for `identifier` and for a `keyword`'s `.text` subfield. |
| `flusso_lowercase` | normalizer | Single token, lowercased and folded, for case-insensitive exact match and sort. |

With `text_analysis = "icu"` the tokenizer and folding swap to the ICU plugin's, for CJK and Thai segmentation and folding across scripts.

With `auto_subfields` on, each string field is enriched:

| Field type | Shape | Query this for… |
| --- | --- | --- |
| `text`, `identifier` | analyzer + `.keyword` + `.keyword_lowercase` | the field: full-text; `.keyword`: exact filter, aggregation, exact sort; `.keyword_lowercase`: case-insensitive sort and lookup |
| `keyword`, `enum`, `uuid` | `.text` (`flusso_code`) + `.keyword_lowercase` | the field: exact term, aggregation; `.text`: full-text; `.keyword_lowercase`: case-insensitive sort |

Keyword subfields cap at `ignore_above: 256`. An `enum` with declared `variants` also gets a `.sort` subfield holding each value's rank; see [Field types](field-types.md#enum). Any key set in a field's `options` overrides the auto default for that field: your `analyzer` replaces the default, your `fields` replaces the subfields wholesale.

## Example

```toml
[sinks.primary]
type = "opensearch"
url = { env = "PRIMARY_OPENSEARCH_URL" }
username = "flusso"
password = { env = "OS_PASSWORD" }
batch_size = 2000
refresh_interval = "5s"
number_of_replicas = 2
```
