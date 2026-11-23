# Sink: OpenSearch

A `[sinks.<name>]` table with `type = "opensearch"`: the connection and bulk options, the index settings it applies, and how the sink names, maps, refreshes, and seeds the indexes it owns.

{{#include generated/sink-opensearch.md}}

`<NAME>` in the override variables is the uppercased sink name: `[sinks.primary]` reads `PRIMARY_OPENSEARCH_URL`. Precedence is in [Environment variables](environment.md#config-values).

## Retries

A write sends its documents in bulk requests and retries what can succeed on a second try, up to `max_retries` times, waiting 200 ms and doubling:

| What failed | Retried | After the last retry |
| --- | --- | --- |
| The whole request: no response, or a non-2xx status | the whole request | the write fails |
| Single documents refused as **busy**: HTTP 429 (a full write queue, a circuit breaker, a flood-stage disk block) or 503 (a shard unavailable or timed out) | only those documents; the rest of the request is already written | the write fails |
| Single documents refused because their **index** refuses writes (a [write block](#write-blocks), a role without write permission on it) or is gone (`index_not_found_exception`) | not inside the write | n/a: the index's lane parks, or is staged again (see [Write blocks](#write-blocks) and [Seeding](#seeding)) |
| Single documents refused for any other reason (a mapping conflict, a malformed value, a version conflict) | never: the same document is refused again | n/a: each is an [item-level rejection](index-and-on-error.md#on_error), reported once |
| The server's certificate | never | the write fails at once (see below) |

A busy document never reaches [`on_error`](index-and-on-error.md#on_error): under `skip` it is not quarantined, and under `stop` it doesn't park its lane. When the write fails, the sink engine restarts with backoff (one second, doubling to 60 s while it keeps failing, back to one second once a restart writes a batch) and the batch it held comes back, so nothing it carried is confirmed or lost.

What a long busy spell costs:

- **The sink falls behind.** While it retries (a full disk, a cluster refusing work for minutes) its lane fills, the ingest engine is paced to it, and the replication slot keeps WAL on the Postgres server until the sink catches up. Watch the sink's lag and the slot's retained WAL; see [Watch it run](../operate/watch-it-run.md).
- **A redelivered batch is written again in full.** The documents OpenSearch accepted before the write failed are rewritten, with `refresh=true` when the sink is caught up: more load on a cluster that is already refusing work. Each resend logs `OpenSearch refused documents as busy; resending them` with the count and OpenSearch's reason; a write that runs out of retries fails with `… still refused as busy after <n> retries`.

## Write blocks

Some refusals are about an **index**, not its documents. The sink reports them per index, so the sink engine holds that index's lane and keeps writing the others. None of them reaches [`on_error`](index-and-on-error.md#on_error): nothing is quarantined under `skip`, and `stop` isn't consulted.

| OpenSearch answers each item | Cause | The lane |
| --- | --- | --- |
| `403 cluster_block_exception` | `index.blocks.write`, `index.blocks.read_only` or `index.blocks.read_only_allow_delete`, set by an operator | `blocked` |
| `403 security_exception` | a role without write permission on that index | `blocked` |
| `400 index_closed_exception` | the index closed with `POST /<index>/_close` (some versions answer `403 cluster_block_exception`, `FORBIDDEN/4/index closed`, instead) | `blocked` |
| `404 index_not_found_exception` | the generation was deleted while flusso runs: see [Seeding](#seeding) | `restaging` |

A `blocked` lane writes its oldest batch again alone on the [`[backoff]`](config-toml.md#backoff) schedule (1 s, doubling to 60 s, by default). Once the block is lifted the next try lands and the lane resumes, with no restart. Meanwhile [`/status`](http.md#lane-health) shows the lane `blocked` with OpenSearch's reason, and the lane holds the replication slot: the runbook is [A lane is stalled](../operate/stalled-lane.md).

**A closed index** gets its own reason, `index [<generation>] is closed`, since OpenSearch's reads only `closed`. Opening that generation (`POST /<generation>/_open`, or through its alias) lets the next try land. It doesn't stop the sink from starting either: staging skips the refresh it can't make on a closed generation, and the lane parks at its first write. **`{prefix}flusso_meta` is not covered**: closed, it fails the sink's staging, so none of the sink's indexes is written until it's opened.

```text
{"index":{"_index":"users_1a2b3c4d_1","_id":"42","status":403,
  "error":{"type":"cluster_block_exception","reason":"index [users_1a2b3c4d_1] blocked by: [FORBIDDEN/8/index write (api)];"}}}
```

Two look alike and aren't index-wide:

- **The flood-stage disk block** answers `429`, so it is [busy](#retries): resent inside the write, and a write still refused after `max_retries` fails and restarts the sink.
- **A request-level 403** (the whole `_bulk` refused: credentials, a role with no bulk permission at all) fails the write, and the sink restarts with backoff.

## TLS and transport errors

An `https://` cluster's certificate is verified against the Mozilla root bundle built into flusso. The operating system's trust store is never read, so a CA installed into the container's store changes nothing. For a cluster behind a private or company CA, point `ca_file` at its PEM bundle: those certificates are trusted on top of the Mozilla roots, verification stays on.

```toml
[sinks.primary]
type = "opensearch"
url = "https://search.internal:9200"
ca_file = "/etc/flusso/opensearch-ca.pem"
```

The file is read when the sink starts: a missing file, or one with no `-----BEGIN CERTIFICATE-----` block, stops the start with the path in the message. `flusso check` and `flusso build` don't read it, so the file only has to exist where flusso runs. To trust the OS store's CAs as well, point `ca_file` at its bundle (`/etc/ssl/certs/ca-certificates.crt` on Debian). `tls_verify = false` skips verification altogether, `ca_file` included (flusso warns at start when both are set); keep it for a local cluster.

A refused certificate is configuration, not a network blip, so a bulk request it fails is **not** retried: the write fails at once, whatever `max_retries` says, and the sink engine restarts with backoff until the certificate is trusted.

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
| A user that can create, alias, write, and delete flusso's indexes, and read their settings | Plus the small `{prefix}flusso_meta` index. Staging and seeding read each generation's `index.uuid` (`GET /{generation}/_settings/index.uuid`). |
| `analysis-icu` on every node | Only with `text_analysis = "icu"`. |
| `action.auto_create_index` excluding flusso's generations | Recommended. Keep `{prefix}flusso_meta` allowed (it's created by its first write) and forbid `{prefix}{logical}_*` for each index, first match wins; [why](#why-forbid-auto-create). With no `prefix`: `+flusso_meta,-users_*,-orders_*,+*`. With `prefix = "staging_"`, every entry carries it: `+staging_flusso_meta,-staging_users_*,-staging_orders_*,+*` (an unprefixed entry never matches, and the trailing `+*` lets auto-create through). |

## Index naming

| Name | Is | Purpose |
| --- | --- | --- |
| `{prefix}{logical}_{hash}` | alias | The **hash alias**. What flusso and `flusso-query` address. The hash derives from the schema. |
| `{prefix}{logical}_{hash}_{n}` | index | A **generation**: the concrete index holding data. `n` increments on reindex. |
| `{prefix}{logical}` | alias | The **convenience alias** for humans and dashboards. Best-effort: if a real index already owns the name, flusso warns and carries on. |
| `{prefix}flusso_meta` | index | One doc per index: its active generation, that generation's `uuid` (its `index.uuid`, see [Each generation's identity](#each-generations-identity)), its seeded state, and, while it is unseeded, how far its backfill got (`walked`: the source's own text and the generation it was recorded for), so a restart resumes after it. `walked` is written as a partial update, never creating a doc, and read only while the doc is unseeded and names the same generation. |

A structural schema change moves the hash, so the new shape gets a fresh alias and generation, re-seeded from scratch, rather than colliding with the old mapping. An on-demand reindex builds generation `n+1` behind the same hash alias while `n` keeps serving, then the alias flips atomically and `n` is dropped, along with any other `{logical}_{hash}_{n}` nothing serves. A reindex while the index is still unseeded does nothing: a rebuild from scratch is already pending. See [Reindex without downtime](../operate/reindex.md).

Writes to a **seeded** index go through the hash alias, each bulk item flagged `require_alias`, so a write never creates an index under that name. Writes to an **unseeded** generation (first seed, or a rebuild while the alias still serves `n`) go to the generation by its concrete name.

## Mapping

Each index is created up front from the resolved schema mapping with `dynamic: strict`: types come from the schema, and a document with an unmapped key is rejected. The one open spot is a [`map`](objects-and-maps.md#map): its object takes runtime keys, and a `dynamic_templates` entry per map (named `flusso_map:<path>`, matching `<path>.*`) maps each key like a plain field of the map's `values`. An index that already exists is left as is. The `on_error` policy decides what an item-level rejection does; see [Index entries and on_error](index-and-on-error.md#on_error).

## Document ids

A document's `_id` is its root key as a string. OpenSearch caps an `_id` at **512 bytes** of UTF-8, not characters: 512 ASCII characters fit, 257 two-byte characters (`é`) don't. A long `text` key can go over, and so can a `bytea` key past 255 bytes, since its id is the `\x…` hex text.

OpenSearch checks ids before it runs a bulk request, and one bad id refuses the whole request. So the sink checks first: a document it would refuse is **rejected on its own**, never sent, and the rest of the write lands. The id is never truncated or hashed, since that would point the row at a different document.

| Operation | Id | Outcome |
| --- | --- | --- |
| upsert | 1 to 512 bytes | Sent. |
| upsert | over 512 bytes | Rejected: `document id is 600 bytes; OpenSearch accepts at most 512 bytes in an _id`. |
| upsert | empty | Rejected: `document id is empty; OpenSearch needs a non-empty _id`. |
| delete | empty | Not sent, and not a rejection: no document has an empty id. |
| delete | over 512 bytes | Sent. OpenSearch answers `not_found`: no document can carry that id. |

A rejected id is an item-level rejection like any other, so the index's [`on_error`](index-and-on-error.md#on_error) decides it and nothing retries it:

- **`skip`**: the document is quarantined and the batch is acked. The log carries `document rejected by sink; quarantining and continuing` with the `index`, `id`, and `reason` (a log line and `last_error` show the first 64 characters of a longer id, then its length); [`/status`](http.md#get-status) counts it in `documents_quarantined`, and the [`flusso.documents.quarantined`](metrics.md) counter goes up. The row stays out of the index until its key changes.
- **`stop`**: the rest of the batch is written, and the index's lane **parks** with the batch unconfirmed, while the sink's other indexes keep being written. [`/status`](http.md#lane-health) shows the lane `refused` with the reason `refused: id kkkk… (600 bytes) (document id is 600 bytes; …)`. Each probe writes a [fresh build](index-and-on-error.md#a-refused-lanes-fresh-build) of the document and is refused again, until the row's key is shortened or the row deleted, or the policy is `skip` and flusso restarted. See [A lane is stalled](../operate/stalled-lane.md).

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
| Backfill | `-1` | Bulk seeding without per-write refresh churn. |
| Seeded, backlog draining | the configured value | Bounds staleness under sustained load. |
| Seeded, caught up | immediate refresh on each caught-up write | Search is fresh when traffic is light. |

The configured value therefore only matters while a backlog is draining. Raise it for throughput under load; lower it toward `1s` for fresher reads while behind.

A write is **caught up** when nothing else is queued behind it. It carries `refresh=true`, and refreshes every index the writes before it left unrefreshed, so everything written before it is searchable when it returns. Several writes run at once ([`[concurrency] writes`](config-toml.md#concurrency)), so the last ones of a burst can finish after the caught-up one; when the sink goes idle with something not yet searchable, flusso sends it an empty caught-up write, which refreshes what is left. So a burst is searchable as it ends, not `refresh_interval` later.

Each write also reports when its documents become searchable, which is what a [`searchable` fence](http.md#get-waitpositionposition) waits on: at once for a caught-up write, within `refresh_interval` otherwise, or at the next caught-up write with `refresh_interval = "-1"`. flusso never refreshes because a reader is waiting.

## Seeding

A generation is created with the aliases it may carry. The first generation of a hash alias gets the hash alias and the convenience alias in its create request; the convenience alias is left out while an index holds that name. A rebuild's generation is created with none, because the old generation keeps serving until the swap.

> ℹ️ **Note** — A first seed has no older copy to serve meanwhile. Both aliases name the new generation from the moment it is created, so a reader sees the index filling: the documents a refresh has already covered, and the whole index once `/status` shows it `seeded`. A rebuild is different: a reindex, a schema change that moves the hash, or a lost slot. Readers stay on the complete old generation until the swap below. To keep readers off an index until its first backfill finishes, wait for `seeded` in `/status` before sending them to it.

When a backfill completes, the sink seeds the index in this order:

1. Check the generation is the one staging found: it exists and has the same `index.uuid` (see [Each generation's identity](#each-generations-identity)). If not, nothing is seeded: the index is staged again and backfilled in full.
2. Refresh the generation, restore its `refresh_interval`, and read the alias holders and the other generations, all at once.
3. Move both aliases onto the generation and drop every other generation of the hash alias, in one atomic `_aliases` call. On a rebuild, readers see the old generation or the new, refreshed one, never neither. On a first seed the aliases are already there, and nothing moves. If OpenSearch refuses that call, the hash alias is moved alone and the convenience alias stays best-effort.
4. Write the marker in `flusso_meta`, last. If anything before it fails, the index stays unseeded, and the next start runs the backfill again. The backfill is idempotent.

Seeded state is recorded in `flusso_meta`, so a restart skips a completed backfill. A missing replication slot overrides every marker; see [Recover from a dropped slot](../operate/dropped-slot.md).

### Each generation's identity

OpenSearch gives every index it creates its own `index.uuid`, and a new one each time an index is created again under the same name: after a delete, by an auto-create, or by a snapshot restore. flusso records the uuid of each generation it creates in the index's `flusso_meta` doc, and compares it with the live one:

| When | Request | Another uuid, or no index, means |
| --- | --- | --- |
| Staging (every start, every restage) | `GET /{generation}/_settings/index.uuid` | See the table below. |
| Seeding | the same, once more | The generation isn't the one staged: nothing is seeded. |

Creating a generation writes its meta doc twice: unseeded and naming no uuid **before** the create, then the new uuid after it. A staging that fails in between leaves an unseeded doc beside the new, empty generation, never a `seeded` marker or an older generation's uuid; the next start backfills it. A meta doc an older flusso wrote without a `uuid` gains it on the next start, and nothing is rebuilt.

### A generation deleted or replaced

A deleted or replaced generation never ends up seeded with rows missing. Each case, by what was done to it:

| What happened | flusso notices | What it does |
| --- | --- | --- |
| A **seeded** generation deleted between runs | at the next start | Retracts the marker with a warning, recreates the generation empty, and backfills it. This is the supported way to force a rebuild of one index. |
| A **seeded** generation deleted while flusso runs | at the next write: the alias went with it, and `require_alias` makes OpenSearch answer `index_not_found_exception` | Stages that one index again (its lane `restaging`), recreates the generation as above, and backfills it; the sink's other indexes keep being written. Reads through the alias return 404 until the generation exists, then see it filling. |
| An **unseeded** generation (first backfill, or a rebuild) deleted mid-backfill, auto-create forbidden | at the next write: `index_not_found_exception` | Stages that index again and backfills it **from the first row**. The backfill that was cut seeds nothing. |
| An **unseeded** generation deleted mid-backfill, auto-create allowed | at the seed, or at the next start, whichever comes first: the next write silently recreated it with a dynamic mapping, under another uuid | Never seeds it. Deletes it, creates it again with the strict mapping, and backfills it from the first row. |
| A **seeded** generation under another uuid: restored from a snapshot, or recreated by hand | at the next start | Keeps it and serves it as found, even empty, with an info line. Run [`flusso reindex <index>`](../operate/reindex.md) to rebuild it. |

A deletion while the sink engine restarts, or during a staging that fails part way, ends the same way: the next staging finds the new generation and asks for a full backfill. While the fresh backfill runs, the index stays `backfilling` in [`/status`](http.md#get-status) and `/wait` answers `503 backfilling` (`rebuilding` on a rebuild) for it until it is seeded.

The log says which case it is:

| Level | Message | Case |
| --- | --- | --- |
| warn | `staged into a new target while its backfill was running: the rows that backfill wrote are not there, so it can't seed this one; asking for a fresh backfill` | an unseeded generation was replaced mid-backfill |
| info | `a backfill cut short by a new target completed; it seeds nothing, the fresh one does` | the cut backfill's last page was written |
| warn | `this generation is not the one flusso created (deleted, then recreated by a write under action.auto_create_index); deleting it and creating it again` | an auto-created generation found at staging |
| info | `the seeded generation has another index.uuid than when it was created (restored from a snapshot, or recreated by hand?); keeping it` | a restored or hand-made seeded generation |

### Why forbid auto-create

[Cluster requirements](#cluster-requirements) recommends an `action.auto_create_index` pattern that forbids flusso's generations. An unseeded generation is written by its concrete name, so nothing else stops a write from recreating one that was deleted. With the pattern, the next write reports the generation missing and flusso replaces it at once. Without it, until the seed or the next start:

- **Writes land in the auto-created index.** Wasted work, never a partial seed: the fresh backfill rewrites everything.
- **Its dynamic mapping may refuse a document.** Under `on_error = "stop"` the lane parks `refused` and stays parked: its probes go to the same index. A restart replaces the index and heals the lane.

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
