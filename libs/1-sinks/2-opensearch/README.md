# flusso-sinks-opensearch

A [`Sink`] that writes documents to an OpenSearch cluster via the bulk API, with fully-typed mappings, hashed index names behind aliases, and zero-downtime reindex.

## Config keys

| Key | Default | What |
| --- | --- | --- |
| `url` | — | cluster URL (`<NAME>_OPENSEARCH_URL` overrides) |
| `username` / `password` | none | basic auth (`<NAME>_OPENSEARCH_{USERNAME,PASSWORD}`) |
| `tls_verify` | `true` | verify TLS certs (set false for local dev) |
| `batch_size` | `1000` | documents per bulk request |
| `max_bytes` | 10 MiB | max serialized bytes per request, split independently of `batch_size` |
| `timeout_secs` | `30` | request timeout |
| `max_retries` | `3` | transient-failure retries |
| `pipeline` | none | ingest pipeline applied on index |
| `number_of_shards` | `1` | primary shards per created index |
| `number_of_replicas` | `1` | replica shards per created index |
| `refresh_interval` | `"10s"` | steady-state `refresh_interval` after backfill (`"-1"` disables) |
| `text_analysis` | `builtin` | analyzer toolkit — `builtin` or `icu` (needs `analysis-icu` plugin) |
| `auto_subfields` | `true` | auto-enrich `text`/`keyword` fields with analyzer + subfields |

## At a glance

- **Addressing.** `{logical}_{hash}` is an alias; data lives in a generation index
  `{logical}_{hash}_{gen}` behind it. flusso and `flusso-query` address
  `{logical}_{hash}` — the hash comes from the parsed schema.
- **Convenience alias.** Bare `{logical}` is also kept on the current generation
  for ad-hoc queries; best-effort.
- **Mapping.** Field types come from the schema, created `dynamic: strict`.
- **Subfields.** Each `text`/`keyword` field gets `keyword` (exact),
  `keyword_lowercase` (sortable), and `text` (searchable) subfields.
- **Refresh.** Off during seeding; flips to `refresh_interval` once seeded; an
  immediate refresh on any caught-up flush.
- **Seeding.** Persisted in a hidden `flusso_meta` index, so restarts skip a
  completed backfill.

## Index naming — alias over generations

The sink owns each index it writes and creates it up front from an explicit,
fully-typed mapping ([`ensure_index`](OpensearchSink::ensure_index)).

- **Hash alias over generations.** The addressable name `{logical}_{hash}` (hash
  derived from the parsed schema) is an **alias**; the data lives in a concrete
  *generation* index `{logical}_{hash}_{gen}` behind it. A structural schema
  change moves the hash — a fresh alias + generation, re-seeded from scratch. An
  on-demand [`reindex`](OpensearchSink::reindex) (same schema) builds the *next*
  generation while the current one keeps serving reads, then
  [`mark_seeded`](OpensearchSink::mark_seeded) atomically repoints the alias and
  drops the old generation — so reads never see a half-built index. flusso and
  the `flusso-query` client address `{logical}_{hash}` (reading through an alias
  is transparent); the active generation + seeded-state live in a per-index meta
  doc.
- **Convenience alias.** The logical name `{logical}` is *also* kept as an alias
  on the current generation, so a human or ad-hoc tool can query `{logical}`
  without knowing the hash. Best-effort: a failure (say, the cluster already has
  a real index named `{logical}`) is logged and ignored, because correctness
  never depends on it.

> 💡 **Did you know** — a reindex builds a fresh index beside the live one and
> flips an alias. No downtime, no holding your breath.

## Mapping and analysis

- **Explicit mapping.** Field types come from the schema, not OpenSearch's
  dynamic guesses, and the index is created `dynamic: strict` so only configured
  fields are accepted. An index that already exists is left untouched.
- **Production-ready defaults.** Every index ships the `flusso_*` analysis
  definitions, and (unless `auto_subfields` is off) each `text`/`keyword` field
  is enriched with a good case/accent-insensitive analyzer plus `keyword`
  (exact), `keyword_lowercase` (sortable), and `text` (searchable) subfields. A
  field's explicit mapping always wins. See the crate-private `build_analysis`
  and `build_property`.

## Refresh — adapts to backlog

The index is created with auto-refresh disabled (`refresh_interval: -1`) for
fast bulk seeding; writes during backfill accumulate without per-flush refresh
churn. When seeding completes ([`mark_seeded`](OpensearchSink::mark_seeded)) the
index is refreshed once and handed the configured `refresh_interval` (default
`"10s"`) — the steady-state visibility ceiling. On top of that,
[`flush`](OpensearchSink::flush) forces an immediate refresh whenever it's told
the pipeline has *caught up* (no backlog behind the batch), so search is fresh
when idle but indexing stays cheap while a backlog drains. The
`refresh_interval` only bounds staleness during sustained backlog, when a
caught-up flush never happens.

Operations are buffered in memory until `flush` is called. Large flushes are
chunked by `batch_size` to stay within OpenSearch request limits. Seeding state
is persisted in a hidden `flusso_meta` index so restarts skip a completed
backfill.

## Module layout

This file holds the [`OpensearchSink`] type, its constructor, and the few
shared helpers (`maybe_auth`, `physical`). The rest is split by concern:

- `transport` — the HTTP plumbing: the bulk request, the small request
  helpers, and the generic per-index operations (create/exists/delete/refresh).
- `generations` — the alias-over-generations addressing: the aliases, the
  meta doc, generation discovery, and the pure naming functions.
- `sink` — the [`Sink`] trait implementation tying it all together.
- `mapping` — building the `dynamic: strict` index body and analysis.
- `bulk` — the bulk wire format, request chunking, and rejection parsing.
