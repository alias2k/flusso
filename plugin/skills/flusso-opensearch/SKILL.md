---
name: flusso-opensearch
description: How flusso's OpenSearch sink side works — flusso owns the index (dynamic:strict typed mapping derived from the schema), the hashed physical index name + convenience alias, generations and reindex, the flusso_* analyzers and auto subfields (and which subfield to query for exact vs full-text vs case-insensitive), refresh behavior, and the sink config keys. Use to understand what flusso produces in OpenSearch and how to query it correctly. Covers the flusso-relevant slice of OpenSearch, not the whole query DSL.
---

# flusso's OpenSearch sink (how the write-to side works)

flusso **owns the index**: it derives the mapping from your schema, creates the index, seeds it, and keeps it current. Your application **owns the queries**. Reads go **straight to OpenSearch** — the engine is write-only, and the read path is the **flusso-query** crate (see that skill), never flusso itself.

OpenSearch is the *substrate*. This covers the slice flusso shapes; for the query DSL itself the [OpenSearch query DSL docs](https://opensearch.org/docs/latest/query-dsl/) are the authority.

## flusso owns the index — what that means

- **Explicit, fully-typed mapping, `dynamic: strict`.** Field types come from the schema, not from OpenSearch's dynamic guesses; only configured fields are accepted (an unexpected field is rejected, not silently mapped). An index that already exists is left untouched.
- **Hashed physical name `{logical}_{hash}`.** The real index name carries a hash of the parsed schema. A **structural schema change rotates the hash** → flusso writes to a *fresh* index seeded from scratch, rather than into the old, now-mismatched shape. The **logical name** (e.g. `users`) is the pipeline's stable identity.
- **Convenience alias.** The logical name is also kept as an alias on the *current* physical index, repointed atomically when the hash moves — so `GET /users/_search` always hits the latest generation without you knowing the hash. It's for humans and ad-hoc tooling: flusso and `flusso-query` always address the **physical** name directly (the derive computes it as `Struct::INDEX`). Alias upkeep is best-effort — if it can't be set (e.g. a real index already squats the name), flusso logs a warning and continues.
- **Seeding markers.** Seeded state lives in a hidden `flusso_meta` index, so a restart skips a completed backfill instead of redoing it.

## Generations & reindex

Because the physical name is hash-suffixed, multiple **generations** of an index can coexist; the alias points at the live one. A schema change naturally rolls a new generation (new hash → new index → re-seed → alias repoint). An on-demand **reindex** (`POST /reindex` on the private HTTP surface, or the `flusso reindex` client) rebuilds a fresh generation and repoints the alias when it's seeded. (Write-side zero-downtime cutover specifics are tracked in issue #6.)

## Analyzers & subfields — the part that affects *your* queries

flusso creates every index with a tuned `analysis` block and well-shaped fields. **Which subfield you target decides what kind of match you get.**

**Analyzers** (always defined, named `flusso_*`):

| Name | Role |
| --- | --- |
| `flusso_code` | The `type: identifier` analyzer (and the analyzer on a `keyword` field's `.text` subfield). Splits on punctuation, case, and letter↔digit boundaries, then lowercases + folds accents. `C-01234` → `c-01234`, `c01234`, `c`, `01234` — found by `C01234`, `c-01234`, or `01234`, but **not** by fuzzy `c1234` (add `fuzziness` on the query side for that). |
| `flusso_text` | Default for `type: text` — natural language. Tokenize + lowercase + accent-fold, no code-splitting. |
| `flusso_lowercase` | A normalizer (single token, no splitting) for case/accent-insensitive exact match and sort. |

`text_analysis = "icu"` swaps folding/tokenizing to the `analysis-icu` plugin (proper CJK/Thai segmentation, multilingual folding) — **the plugin must be installed on every node** or index creation fails. `builtin` (default) needs no plugins.

**Auto subfields** (when `auto_subfields = true`, the default):

| Schema type | Subfields created | Query which, for what |
| --- | --- | --- |
| `text` | `.keyword` + `.keyword_lowercase` (base analyzer `flusso_code`) | base field → full-text search; `.keyword` → exact filter / aggregation / exact sort; `.keyword_lowercase` → case-insensitive sort & exact lookup |
| `keyword` | `.text` (`flusso_code`) + `.keyword_lowercase` | base field → exact term / aggregation; `.text` → full-text search; `.keyword_lowercase` → case-insensitive sort |

`keyword` subfields cap at `ignore_above: 256`. Other types (`long`, `date`, `boolean`, …) and `object`/`nested` containers map as-is. An **`ids` aggregate** maps to its bare `element_type` (`keyword` or `long`) — OpenSearch has no array type, so a flat array of values is just the element type (a `keyword` id array still gets the keyword auto subfields; query/aggregate it like a single `keyword`/`long`, term queries match any element). **Anything you set in a field's `mapping` overrides the auto default** — your own `analyzer` replaces `flusso_code`, your own `fields` replaces the auto subfields wholesale.

> When querying by hand (not via `flusso-query`), pick the subfield deliberately: filtering/sorting a `text` field → use `.keyword`; full-text over a `keyword` field → use `.text`. The **flusso-query** derive picks the right subfield for each operator automatically.

## Refresh — how fresh search is

Indexes are created with auto-refresh **disabled** (`refresh_interval: -1`) for fast bulk seeding, then handed the configured `refresh_interval` (default `"10s"`) once seeded. A `flush` also **forces an immediate refresh whenever the pipeline has caught up** (no backlog) — so search is fresh under light traffic, while bulk indexing stays cheap when a backlog is draining. `refresh_interval` only bounds staleness *during sustained backlog*: raise it for write throughput under load, lower it (toward `1s`) for fresher search while behind.

## Sink config keys (`[sinks.<name>]`, `type = "opensearch"`)

| Key | Default | Note |
| --- | --- | --- |
| `url` | — (**required**) | cluster base URL |
| `username` / `password` | — | HTTP Basic; each an `env_or_value` (`{ env = "VAR" }`), resolved at run time |
| `tls_verify` | `true` | set `false` only for local dev |
| `batch_size` | `1000` | docs per bulk chunk |
| `max_bytes` | `10 MiB` | bytes per bulk chunk; an oversized doc goes alone |
| `timeout_secs` | `30` | HTTP timeout |
| `max_retries` | `3` | transient-failure retries (exp. backoff) |
| `pipeline` | — | optional ingest pipeline on every index op |
| `number_of_shards` / `number_of_replicas` | `1` / `1` | per created index |
| `refresh_interval` | `"10s"` | steady-state visibility ceiling (see above) |
| `text_analysis` | `builtin` | or `icu` (needs `analysis-icu` on every node) |
| `auto_subfields` | `true` | `false` emits fields bare (explicit `mapping` always wins) |

Define multiple `[sinks.<name>]` and flusso **fans out** — every document lands in each. No sinks → it falls back to a `stdout` sink (handy to *see* documents while integrating). Full reference: the repo's `SOURCES_AND_SINKS.md`.

## Querying — don't hand-roll it

The typed, compile-time-checked way to query a flusso index is **flusso-query** + `#[derive(FlussoDocument)]` (see that skill). It addresses the physical index, picks the right subfield per operator, and fails to compile if your struct drifts from the mapping. Reach for the raw OpenSearch DSL only for what the typed surface defers (aggregations/facets, `knn`, `function_score`, `script`, `geo_shape`) — via the derive's `raw` escape hatch.
