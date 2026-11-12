# Sink: OpenSearch

A `[sinks.<name>]` table with `type = "opensearch"`: the connection and bulk options, the index settings it applies, and how the sink names, maps, refreshes, and seeds the indexes it owns.

{{#include generated/sink-opensearch.md}}

`<NAME>` in the override variables is the uppercased sink name: `[sinks.primary]` reads `PRIMARY_OPENSEARCH_URL`. Precedence is in [Environment variables](environment.md#config-values).

## TLS and transport errors

An `https://` cluster's certificate is verified against the Mozilla root bundle built into flusso. The operating system's trust store is never read, so a CA installed into the container's store changes nothing. For a cluster behind a private or company CA, point `ca_file` at its PEM bundle: those certificates are trusted on top of the Mozilla roots, verification stays on.

```toml
[sinks.primary]
type = "opensearch"
url = "https://search.internal:9200"
ca_file = "/etc/flusso/opensearch-ca.pem"
```

The file is read when the sink starts: a missing file, or one with no `-----BEGIN CERTIFICATE-----` block, stops the start with the path in the message. To trust the OS store's CAs as well, point `ca_file` at its bundle (`/etc/ssl/certs/ca-certificates.crt` on Debian). `tls_verify = false` skips verification altogether; keep it for a local cluster.

A refused certificate is configuration, not a network blip, so a bulk request it fails is **not** retried: the flush fails at once, whatever `max_retries` says, and the sink engine restarts with backoff until the certificate is trusted.

A request that gets no HTTP response is reported, in the log and in [`/status`](http.md#get-status), with its cause. A refused certificate reads:

```text
write failed: listing indices failed: error sending request for url (https://127.0.0.1:9200/_cat/indices/users_00000239?h=index&format=json): client error (Connect): invalid peer certificate: UnknownIssuer
```

A refused connection ends in `tcp connect error: Connection refused` instead. Credentials written into the `url` (`https://user:pass@host`) never appear in the message.

## Cluster requirements

| Requirement | Detail |
| --- | --- |
| OpenSearch 2.x | The query side also speaks Elasticsearch 7.x. |
| A reachable HTTP(S) endpoint | The `url`. |
| A user that can create, alias, write, and delete flusso's indexes | Plus the small `{prefix}flusso_meta` index. |
| `analysis-icu` on every node | Only with `text_analysis = "icu"`. |
| `action.auto_create_index` excluding flusso's generations | Recommended. Keep `{prefix}flusso_meta` allowed (it's created by its first write) and forbid `{prefix}{logical}_*` for each index, first match wins. With no `prefix`: `+flusso_meta,-users_*,-orders_*,+*`. With `prefix = "staging_"`, every entry carries it: `+staging_flusso_meta,-staging_users_*,-staging_orders_*,+*` (an unprefixed entry never matches, and the trailing `+*` lets auto-create through). See [Seeding](#seeding). |

## Index naming

| Name | Is | Purpose |
| --- | --- | --- |
| `{prefix}{logical}_{hash}` | alias | The **hash alias**. What flusso and `flusso-query` address. The hash derives from the schema. |
| `{prefix}{logical}_{hash}_{n}` | index | A **generation**: the concrete index holding data. `n` increments on reindex. |
| `{prefix}{logical}` | alias | The **convenience alias** for humans and dashboards. Best-effort: if a real index already owns the name, flusso warns and carries on. |
| `{prefix}flusso_meta` | index | One doc per index recording its active generation and seeded state. |

A structural schema change moves the hash, so the new shape gets a fresh alias and generation, re-seeded from scratch, rather than colliding with the old mapping. An on-demand reindex builds generation `n+1` behind the same hash alias while `n` keeps serving, then the alias flips atomically and `n` is dropped, along with any other `{logical}_{hash}_{n}` nothing serves. A reindex while the index is still unseeded does nothing: a rebuild from scratch is already pending. See [Reindex without downtime](../operate/reindex.md).

Writes to a **seeded** index go through the hash alias, each bulk item flagged `require_alias`, so a write never creates an index under that name. Writes to an **unseeded** generation (first seed, or a rebuild while the alias still serves `n`) go to the generation by its concrete name.

## Mapping

Each index is created up front from the resolved schema mapping with `dynamic: strict`: types come from the schema, and a document with an unmapped key is rejected. The one open spot is a [`map`](objects-and-maps.md#map): its object takes runtime keys, and a `dynamic_templates` entry per map (named `flusso_map:<path>`, matching `<path>.*`) maps each key like a plain field of the map's `values`. An index that already exists is left as is. The `on_error` policy decides what an item-level rejection does; see [Index entries and on_error](index-and-on-error.md#on_error).

## Index settings

`index_settings` adds OpenSearch index settings to the indexes the sink creates, in two places:

| Where | Applies to |
| --- | --- |
| `index_settings` in `[sinks.<name>]` | Every index this sink creates. |
| `index_settings` in an `[[index]]` entry's `[index.sinks.<name>]` | That index only, on that sink. |

{{#include generated/sink-opensearch-index.md}}

A key is a setting name, with or without the `index.` prefix (`"mapping.total_fields.limit"` = `index.mapping.total_fields.limit`); nested tables flatten to dotted keys. The typical use is raising the mapping limits a wide document hits: `index.mapping.total_fields.limit` (1000 by default, and each `text`/`keyword` field costs 3–4 fields with `auto_subfields` on; see [Keep content without indexing it](../author/keep-content-unindexed.md) to shrink the mapping instead), `index.mapping.nested_fields.limit`, `index.mapping.nested_objects.limit`.

**Precedence**, highest first:

1. Keys flusso owns, **rejected** by `build`, `check`, and `run` rather than silently dropped: `index.refresh_interval` (see [Refresh](#refresh); set the `refresh_interval` option instead), anything under `index.analysis` (the `flusso_*` components) or `index.blocks` (flusso must write, swap aliases, and update settings), and `index.hidden` (generations are found by wildcard, which skips hidden indexes).
2. The index's own `index_settings`.
3. The sink-wide `index_settings`.
4. The typed `number_of_shards` and `number_of_replicas` options.

A higher layer replaces the same key and any key nested under or above it. They also reject a key given twice (`mapping.x` and `index.mapping.x`), a key that is both a value and a table, an empty table, and `null`. That is all flusso checks: an unknown key, a wrong case, or a bad value passes through as written and fails index creation.

> **Warning:** some settings flusso leaves open weaken its guarantees. `index.translog.durability = "async"` lets OpenSearch acknowledge a bulk before it is durable, and flusso then confirms the replication slot past it. `index.mapping.ignore_malformed` and `index.mapping.coerce` drop or reshape values that `on_error` would otherwise see as rejections.

**When settings apply.** Settings are read when a generation is **created**: the first seed, a schema change, or a [reindex](../operate/reindex.md). Changing them never updates a live index and never triggers a rebuild on its own, because they are not part of the content hash. The sink reads its settings once, at startup, so a reindex against a running flusso still uses the old ones. To apply a change, restart (or redeploy) flusso with the new config, then run `flusso reindex <index>`: the new generation is created with the new settings and the alias flips to it.

```toml
[sinks.primary]
type = "opensearch"
url = "https://search.internal:9200"
index_settings = { "mapping.total_fields.limit" = 2000 }

[[index]]
name = "orders"
schema = "orders.schema.yml"
enabled = true

[index.sinks.primary]
index_settings = { "mapping.total_fields.limit" = 5000, "mapping.nested_objects.limit" = 20000 }
```

## Refresh

| Phase | `refresh_interval` | Why |
| --- | --- | --- |
| Backfill | `-1` | Bulk seeding without per-flush refresh churn. |
| Seeded, backlog draining | the configured value | Bounds staleness under sustained load. |
| Seeded, caught up | immediate refresh on each flush | Search is fresh when traffic is light. |

The configured value therefore only matters while a backlog is draining. Raise it for throughput under load; lower it toward `1s` for fresher reads while behind.

Each flush also reports when its documents become searchable, which is what a [`searchable` fence](http.md#get-waitpositionposition) waits on: at once for a caught-up flush, within `refresh_interval` otherwise. With `refresh_interval = "-1"` nothing refreshes a flush made during a backlog, so the next caught-up flush refreshes the indexes the backlog wrote. flusso never refreshes because a reader is waiting.

## Seeding

Seeded state is recorded in `flusso_meta`, so a restart skips a completed backfill. On every start the marker is checked against the generation it names: if that index is gone, the marker is retracted with a warning, the generation is recreated empty, and the backfill refills it. Deleting `{logical}_{hash}_{n}` between runs is the supported way to force a rebuild of one index. A missing replication slot overrides every marker; see [Recover from a dropped slot](../operate/dropped-slot.md).

Deleting a seeded generation **while flusso runs** is caught on the next write to that index: the alias is gone with it, so the write fails with `index_not_found_exception` instead of OpenSearch auto-creating a dynamically mapped index. That fails the whole flush, not one document. The sink's engine restarts with backoff, the batch is redelivered, and the restart's marker check rebuilds the generation as above. Until that next write, reads through the alias return 404.

Deleting an **unseeded** generation mid-backfill isn't covered by `require_alias`. With the default `action.auto_create_index: true` the next write recreates it with a dynamic mapping. With the recommended pattern from [Cluster requirements](#cluster-requirements) the write fails and the engine restarts, but the part of the snapshot already written is lost. A restart doesn't fix either case: the snapshot in flight is still outstanding, so no new one is requested, and when it completes the partial (or dynamically mapped) generation is marked seeded and kept. Once that backfill finishes, run `flusso reindex <index>` to rebuild it from scratch.

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

Keyword subfields cap at `ignore_above: 256`. An `enum` with declared `variants` also gets a `.sort` subfield holding each value's rank; see [Field types](field-types.md#enum). Any key set in a field's `options` overrides the auto default for that field: your `analyzer` replaces the default, your `fields` replaces the subfields wholesale. A field with [`subfields: false`](field-types.md#subfields) keeps the analyzer and gets no subfields, and a field with [`indexed: false`](joins.md#indexed) is one disabled `object` with nothing under it mapped.

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
