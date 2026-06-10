# flusso sources and sinks

> [!IMPORTANT]
> ## 🤖 Generative AI disclosure
>
> **Generative AI was used in this project to produce boilerplate and
> documentation.** Every single line of code has been manually reviewed and
> revised by a human software developer.

flusso connects one **source** — where rows come from — to one or more **sinks**,
where the built documents go. You configure them under `[source]` and
`[sinks.<name>]` in `flusso.toml`, and this document is the reference for every
supported type and its options.

The cast is short today: Postgres in, OpenSearch (or stdout) out. The seams are
trait objects, so the menu can grow without the engine noticing — but this page
only documents what actually ships.

For the overall `flusso.toml` structure and the index document format, see
[`SCHEMA.md`](SCHEMA.md). For how environment variables resolve and override these
values — the `{ env = "VAR" }` form, the reserved deployment-override names, and
who wins when two of them disagree — see [`CONFIG.md`](CONFIG.md). For the
query-side client that reads what flusso writes, see [`CLIENT.md`](CLIENT.md). The
big-picture tour lives in [`README.md`](README.md).

## Contents

- [The model](#the-model) — one source, many sinks
- [Sinks](#sinks) — where documents land
  - [OpenSearch](#opensearch) — the real sink, with all the knobs
  - [Index analysis & subfields](#index-analysis--subfields) — what flusso bakes into every index
  - [Stdout](#stdout) — the development sink
- [Sources](#sources) — where rows come from
  - [Postgres](#postgres) — connection shapes and change capture

## The model

**One source, many sinks.** A deployment reads from a single source and writes
every document to *all* configured sinks. That's the fan-out: define as many
destinations as you need, and the same document lands in each. Define none and the
CLI quietly falls back to a single [stdout](#stdout) sink, which is more useful
than it sounds when you're poking at things.

**Pluggable.** Source, sink, and the in-process queue are all trait objects, so
the backend choices below swap without touching the engine. The types documented
here are the ones implemented today; the abstraction is ready for more, the
authors are not.

---

## Sinks

A sink is where assembled documents land. Each entry under `[sinks]` is a named
destination — the key is a sink name (a Postgres identifier) and `type` selects
the kind:

```toml
[sinks.primary]            # name: "primary"
type = "opensearch"
url = "https://localhost:9200"

[sinks.audit]              # name: "audit" — documents go here too (fan-out)
type = "stdout"
pretty = true
```

### OpenSearch

```toml
[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }
batch_size = 2000
```

Writes documents to an OpenSearch cluster via the bulk API. This is the sink you
actually deploy.

`url`, `username`, and `password` each accept an
[`env_or_value`](CONFIG.md#secret--connection-values) — a literal, or a
`{ env = "VAR" }` reference resolved when the pipeline runs. These three fields can
also be supplied or overridden per sink via reserved deployment-override
variables; the naming and precedence rules live in
[`CONFIG.md`](CONFIG.md#secret--connection-values).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `url` | `env_or_value` | — (**required**) | Base URL of the cluster, e.g. `https://search.example.com:9200`. |
| `username` | `env_or_value` | — | HTTP Basic Auth username. |
| `password` | `env_or_value` | — | HTTP Basic Auth password. |
| `tls_verify` | bool | `true` | Verify TLS certificates. Set `false` only for local development. |
| `batch_size` | int ≥ 1 | `1000` | Maximum documents per bulk-request chunk. |
| `max_bytes` | int | `10485760` (10 MiB) | Maximum bytes per bulk chunk; within OpenSearch's recommended 5–15 MB range, well under the 100 MB `http.max_content_length` default. A single document larger than this is sent on its own. |
| `timeout_secs` | int ≥ 1 | `30` | HTTP request timeout, in seconds. |
| `max_retries` | int ≥ 0 | `3` | Additional retry attempts on transient failures (exponential backoff). |
| `pipeline` | string | — | Optional OpenSearch ingest pipeline applied on every index operation. |
| `number_of_shards` | int ≥ 1 | `1` | Primary shards for each created index. |
| `number_of_replicas` | int ≥ 0 | `1` | Replica shards for each created index. |
| `refresh_interval` | string | `"10s"` | OpenSearch `refresh_interval` applied to each index after seeding — the steady-state visibility ceiling (e.g. `"10s"`, `"1s"`, or `"-1"` to disable auto-refresh). flusso forces an immediate refresh whenever the pipeline catches up, so this only bounds staleness while a backlog drains (see below). |
| `text_analysis` | `builtin` \| `icu` | `builtin` | Analysis backend for the `flusso_*` analyzers (see [below](#index-analysis--subfields)). `icu` requires the `analysis-icu` plugin on every node. |
| `auto_subfields` | bool | `true` | Auto-enrich `text`/`keyword` fields with a good analyzer and subfields. A field's explicit `mapping` always wins; set `false` to emit fields bare. |

**How it owns its indexes:**

- **Explicit, fully-typed mapping.** The sink creates each index up front from
  the resolved schema mapping, `dynamic: strict` — field types come from the
  schema, not OpenSearch's dynamic guesses, and only configured fields are
  accepted. An index that already exists is left untouched.
- **Hashed physical name.** The actual index is named `{logical}_{hash}`, where
  the hash derives from the parsed index schema. A structural schema change
  changes the hash, so the sink writes to a *fresh* index (re-seeded from
  scratch) rather than into the old, now-mismatched shape. The logical name
  remains the pipeline's identity.
- **Refresh adapts to the backlog.** Created with auto-refresh disabled
  (`refresh_interval: -1`) for fast bulk seeding; on seeding completion the index
  is handed the configured `refresh_interval` (default `"10s"`) — the
  steady-state visibility ceiling. On top of that, a `flush` forces an immediate
  refresh whenever the pipeline has *caught up* (no backlog behind the batch), so
  search is fresh when traffic is light but bulk indexing stays cheap while a
  backlog drains. The `refresh_interval` only bounds staleness during sustained
  backlog, when a caught-up flush never happens — raise it for more write
  throughput under load, lower it (toward `1s`) for fresher search while behind.
- **Production-ready defaults.** Created indexes ship a tuned `analysis` block
  and, unless `auto_subfields` is off, well-shaped `text`/`keyword` fields — see
  [Index analysis & subfields](#index-analysis--subfields).
- **Seeding markers.** Seeded state is persisted in a hidden `flusso_meta` index,
  so a restart skips a completed backfill instead of redoing all that work for
  old times' sake.

#### Index analysis & subfields

The sink creates every index with a good search setup out of the box. flusso owns
the **index** (mapping + analyzers + subfields); your application owns the
**queries**. The notes below say which subfield to target for each job, so you
don't have to reverse-engineer it later.

**Analyzers** (always defined, named `flusso_*`):

| Name | What it does |
| --- | --- |
| `flusso_code` | The `type: identifier` analyzer (and the analyzer on a `keyword` field's `text` subfield). Splits on punctuation, case, and letter↔digit boundaries, then lowercases and folds accents. `C-01234` indexes as `c-01234`, `c01234`, `c`, `01234`, so it's found by `C01234`, `c-01234`, or `01234` — but **not** by a fuzzy `c1234` (add `fuzziness` on the query side if you want that). Tuned for identifier-like short text. |
| `flusso_text` | The default for `type: text` — natural language. Plain tokenize + lowercase + accent fold, no code-splitting. |
| `flusso_lowercase` | A normalizer (single token, no splitting) for case- and accent-insensitive exact match and sort. |

With `text_analysis = "icu"` the folding/tokenizing swaps to the `analysis-icu`
plugin (`icu_tokenizer` / `icu_folding` / `icu_normalizer`) for stronger
multilingual handling — proper CJK/Thai segmentation and folding across every
script. **The plugin must be installed on every node** (`opensearch-plugin
install analysis-icu`) or index creation fails; `builtin` (the default) needs no
plugins.

**Default subfields** (when `auto_subfields` is on — the default):

| Field type | Shape | Query each subfield for… |
| --- | --- | --- |
| `text` | `analyzer: flusso_code` + `.keyword` + `.keyword_lowercase` | the field itself → full-text search; `.keyword` → exact filter / aggregation / exact sort; `.keyword_lowercase` → case-insensitive sort & exact lookup. |
| `keyword` | `.text` (`flusso_code`) + `.keyword_lowercase` | the field itself → exact term / aggregation; `.text` → full-text search; `.keyword_lowercase` → case-insensitive sort. |

`keyword` subfields cap at `ignore_above: 256`. Other types (`long`, `date`,
`boolean`, …) and the `object`/`nested` containers are emitted as-is. Any key you
set in a field's `mapping` overrides the auto default for that field — e.g.
supplying your own `analyzer` replaces `flusso_code`, and supplying `fields`
replaces the auto subfields wholesale. flusso has opinions, but they yield to
yours.

Example query against a `text` field `name`, precise (all terms must match) and
case/punctuation-insensitive:

```json
{ "query": { "match": { "name": { "query": "C-01234", "operator": "and" } } } }
```

### Stdout

```toml
[sinks.audit]
type = "stdout"
pretty = true
```

Writes each operation to standard output as a JSON envelope — handy for
development and for piping into `jq` at 2am to find out why a document looks wrong.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `pretty` | bool | `false` | Pretty-print JSON instead of compact one-line NDJSON. |

Every envelope carries provenance and bookkeeping so a stream is
self-describing: which sink and version produced it (`sink`, `version`), when
(`ts`), in what order (`seq`), the `index`, the `op` (`upsert` / `delete`) and
`id`, plus a `meta` summary (top-level field count and serialized byte size).
An `upsert` carries the `document`; a `delete` does not.

```text
{"document":{"email":"ada@x.io"},"id":"42","index":"users","meta":{"bytes":20,"fields":1},"op":"upsert","seq":1,"sink":"stdout","ts":"2026-06-03T10:20:30.123Z","version":"0.1.0"}
{"id":"7","index":"users","op":"delete","seq":2,"sink":"stdout","ts":"2026-06-03T10:20:30.124Z","version":"0.1.0"}
```

> Logs go to **stderr**, so stdout stays a clean data stream.

---

## Sources

A source is where rows come from. There is exactly one per deployment, and exactly
one type today.

### Postgres

```toml
[source]
type = "postgres"
connection_url = "postgresql://user:pass@localhost:5432/mydb"
```

The only source type today. flusso follows Postgres' **logical replication**
stream to capture changes, and snapshots tables to seed an index before following
live changes.

#### Connection

`connection_url` takes one of two shapes — pick whichever you find less annoying
to template.

**A full URL** — a string or
[`env_or_value`](CONFIG.md#secret--connection-values). Must match
`^(postgresql|postgres)://…`:

```toml
connection_url = "postgresql://user:pass@localhost:5432/mydb"
connection_url = { env = "DATABASE_URL" }
```

**Individual parts** — a table. `database` is required; the rest default:

| Part | Type | Default | |
| --- | --- | --- | --- |
| `host` | string | `127.0.0.1` | |
| `port` | int (1–65535) | `5432` | |
| `user` | string | `postgres` | |
| `password` | `env_or_value` | — | optional |
| `database` | string | — | **required** |

```toml
[source.connection_url]
host = "127.0.0.1"
port = 5432
user = "postgres"
password = { env = "PGPASSWORD" }
database = "mydb"
```

Whichever shape you choose can be overridden by a reserved deployment variable, so
the same config travels across environments unedited — see
[`CONFIG.md`](CONFIG.md#secret--connection-values) for the override and precedence
rules.

#### How it captures changes

- **Logical replication (WAL).** flusso consumes a logical replication slot. The
  slot is **created automatically** if it does not exist; the **publication must
  already exist** (it decides which tables are streamed — a schema decision, not
  a runtime one). Slot and publication names are CLI flags (`--slot`,
  `--publication`, both defaulting to `flusso`).
- **Backfill.** Before live capture, the engine asks each sink whether an index
  is already seeded and, for those that aren't, snapshots the root tables to seed
  them. `--skip-backfill` resumes live capture only.
- Requires `wal_level = logical` on the server. See the
  [`dev/`](dev/README.md) environment for a ready-to-run Postgres configured for
  this, so you don't have to discover the requirement the hard way.
