# Configuring a deployment

One `flusso.toml` file describes a deployment — the source database, the sink destinations, and which indexes to build — plus the environment that feeds it secrets and runtime flags. This guide is the single reference for that file and that environment. (Each index's own `*.schema.yml` is covered in [schema authoring](schema-authoring.md).)

## Quick reference

| Looking for… | Jump to |
| --- | --- |
| Every `flusso.toml` top-level key | [The `flusso.toml` format](#the-flussotoml-format) |
| Postgres source options | [Postgres](#postgres) |
| OpenSearch sink options + defaults | [OpenSearch](#opensearch) |
| Which subfield to query (exact / full-text / sort) | [Index analysis & subfields](#index-analysis--subfields) |
| Stdout sink envelope | [Stdout](#stdout) |
| Secrets, `{ env = "VAR" }`, the reserved overrides + precedence | [Secrets & connection values](#secrets--connection-values) |
| The `FLUSSO_*` flag env vars | [CLI flags as env vars](#cli-flags-as-env-vars) |
| Status / metrics / control ports + auth | [HTTP surfaces](#http-surfaces) |
| Sharing one cluster across deployments | [Index prefix](#index-prefix) |
| `RUST_LOG`, OTLP, Prometheus | [Logging & telemetry](#logging--telemetry) |
| A copy-paste env block | [Cheat sheet](#cheat-sheet) |

flusso reads env vars for three jobs: **filling in config values** (the [secrets story](#secrets--connection-values)), **setting CLI flags** (every flag has a `FLUSSO_*` twin — [CLI flags as env vars](#cli-flags-as-env-vars)), and **logging & telemetry** ([below](#logging--telemetry)).

### Every key + default

| Key | Where | Default | Purpose |
| --- | --- | --- | --- |
| `on_error` | top-level / `[[index]]` | `"stop"` | item-rejection policy — `stop` or `skip` ([on_error](#on_error)) |
| `prefix` | top-level | — | prepend to every owned index name ([index prefix](#index-prefix)) |
| `type` | `[source]` | — | `postgres` |
| `connection_url` | `[source]` | — | full URL or parts; `DATABASE_URL` overrides |
| `manage_publication` | `[source]` | `true` | auto-create/extend the publication |
| `type` | `[sinks.<name>]` | — | `opensearch` or `stdout` |
| `url` | opensearch sink | — | cluster URL; `<NAME>_OPENSEARCH_URL` overrides |
| `username` / `password` | opensearch sink | — | HTTP Basic auth |
| `tls_verify` | opensearch sink | `true` | verify TLS certs |
| `batch_size` | opensearch sink | `1000` | docs per bulk chunk |
| `max_bytes` | opensearch sink | 10 MiB | bytes per bulk chunk |
| `timeout_secs` | opensearch sink | `30` | HTTP request timeout |
| `max_retries` | opensearch sink | `3` | transient-failure retries |
| `pipeline` | opensearch sink | — | ingest pipeline applied on index |
| `number_of_shards` | opensearch sink | `1` | primary shards per index |
| `number_of_replicas` | opensearch sink | `1` | replica shards per index |
| `refresh_interval` | opensearch sink | `"10s"` | steady-state refresh ceiling (`"-1"` disables) |
| `text_analysis` | opensearch sink | `builtin` | analyzer toolkit — `builtin` or `icu` |
| `auto_subfields` | opensearch sink | `true` | auto subfields on `text`/`keyword` |
| `pretty` | stdout sink | `false` | pretty JSON instead of NDJSON |
| `name` / `schema` / `enabled` | `[[index]]` | — | logical name / schema path / build on this run |
| `public_address` / `private_address` | `[server]` | `127.0.0.1:9464` / `:9465` | HTTP bind addresses ([HTTP surfaces](#http-surfaces)) |

> ℹ️ **Info** — `schema::load("flusso.toml")` is the front door: it reads the config and every schema it references, validates both layers, and returns one fully-validated `Config`. Schema paths resolve **relative to the config file's directory**. Two JSON Schemas are the machine-readable source of truth — [`config.schema.json`](https://github.com/alias2k/flusso/blob/main/libs/2-schema/1-config-toml/config.schema.json) and [`index.schema.yml`](https://github.com/alias2k/flusso/blob/main/libs/2-schema/1-index-yaml/index.schema.yml); point an editor at them for completion.

---

## The `flusso.toml` format

Top-level table. Only `[source]` is required.

| Key | Required | Description |
| --- | --- | --- |
| `[source]` | **yes** | The database to read from. |
| `[sinks.<name>]` | no | Named destinations. Zero or more; each key is a sink name (a Postgres identifier). |
| `[[index]]` | no | The indexes to build. Zero or more array entries. |
| `on_error` | no | What to do when a sink rejects a document at the item level: `"stop"` (default) or `"skip"`. See [`on_error`](#on_error). |
| `prefix` | no | Literal string prepended to every index name flusso owns (indexes, aliases, `flusso_meta`), so deployments can share one cluster — e.g. `prefix = "dev_"` → `dev_users`. Overridable at runtime by `--index-prefix` / `FLUSSO_INDEX_PREFIX`. See [Index prefix](#index-prefix). |

### `[source]`

The database documents are read from — one per deployment. `type` selects the kind:

| `type` | Reference |
| --- | --- |
| `postgres` | [Postgres source](#postgres) |

```toml
[source]
type = "postgres"
connection_url = "postgresql://user:pass@localhost:5432/mydb"
```

Connection options (full-URL and individual-parts forms, the `DATABASE_URL` override) and
capture behavior live in [Sources](#sources) and
[Secrets & connection values](#secrets--connection-values).

### `[sinks.<name>]`

Named destinations; each key is a sink name (a Postgres identifier) and `type` selects the
kind. Define more than one and flusso **fans out** — every document is written to all of
them. If no sinks are defined, the CLI falls back to a stdout sink.

| `type` | Reference |
| --- | --- |
| `opensearch` | [OpenSearch sink](#opensearch) |
| `stdout` | [Stdout sink](#stdout) |

```toml
[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }

[sinks.audit]
type = "stdout"
pretty = true
```

Each type's full option set and behavior is documented in [Sinks](#sinks).

### `[[index]]`

One array entry per index to build.

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | Postgres identifier | yes | The logical index name — the pipeline's stable identity. |
| `schema` | path | yes | Path to the index's `*.schema.yml`, relative to the config file. Must end in `.yml`/`.yaml`. |
| `enabled` | bool | yes | Whether this index is built on this run. |
| `on_error` | `"stop"` \| `"skip"` | no | Override the global [`on_error`](#on_error) for this index. Omitted inherits the global default. |

```toml
[[index]]
name = "users"
schema = "users.schema.yml"
enabled = true
```

The `*.schema.yml` referenced here is documented in [schema authoring](schema-authoring.md).

### `on_error`

When a sink accepts a flush but rejects a *specific* document — a mapping conflict, a value
the destination can't index — `on_error` decides what happens. It governs only these
**item-level rejections**; a flush-wide failure (the destination unreachable, the whole
request refused) always stops the run.

| Value | Behavior |
| --- | --- |
| `"stop"` (default) | Stop the run. The batch is left unconfirmed and redelivered on restart, so a persistently-bad document halts sync until the data is fixed or the policy changes. Dropping data is opt-in. |
| `"skip"` | Quarantine the document (logged, counted in `flusso.documents.quarantined` and the `/status` `documents_quarantined`) and continue. The rest of the batch is applied and acked; the document never lands until its source row changes again. |

A global `on_error` is the default for every index; override it per index with `on_error`
inside an `[[index]]` entry. The policy is operational, not part of the document shape, so
changing it never triggers a reindex.

```toml
on_error = "stop"   # global default

[[index]]
name = "analytics"
schema = "analytics.schema.yml"
enabled = true
on_error = "skip"   # this index tolerates bad rows
```

### `prefix`

See [Index prefix](#index-prefix) for the full behavior, the runtime overrides, and the
rules for sharing one OpenSearch cluster across deployments.

---

## The model

**One source, many sinks.** Source, sink, and the in-process queue are all trait objects, so backends swap without touching the engine. Today that's Postgres in, OpenSearch (or stdout) out — the only backends documented here.

---

## Sources

A source is where rows come from. There is exactly one per deployment, and one type today.

### Postgres

```toml
[source]
type = "postgres"
connection_url = "postgresql://user:pass@localhost:5432/mydb"
manage_publication = true   # optional; default true
```

flusso follows Postgres' **logical replication** stream to capture changes, and snapshots
tables to seed an index before following live changes.

| Key | Type | Default | |
| --- | --- | --- | --- |
| `connection_url` | URL / parts | — | see below |
| `manage_publication` | bool | `true` | auto-create/extend the publication when privileged; see [capture](#how-it-captures-changes) |

#### Connection

`connection_url` takes one of two shapes.

**A full URL** — a string or [`env_or_value`](#env_or_value-references). Must match
`^(postgresql|postgres)://…`:

```toml
connection_url = "postgresql://user:pass@localhost:5432/mydb"
connection_url = { env = "DATABASE_URL" }
```

**Individual parts** — `database` is required; the rest default:

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

Either shape can be overridden by a reserved deployment variable, so the same config
travels across environments unedited — see
[Secrets & connection values](#secrets--connection-values) for the override and precedence
rules.

#### How it captures changes

- **Logical replication (WAL).** flusso consumes a logical replication slot. The slot is
  **created automatically** if it does not exist. The **publication** is also **managed
  automatically**: flusso derives the full table set from your schema (root tables plus
  every table a join or aggregate reads) and creates or extends the publication to cover
  it — provided the source role can (it must own those tables and hold `CREATE` on the
  database, or be superuser). When it can't, flusso logs the exact `CREATE`/`ALTER
  PUBLICATION` SQL instead and keeps going; `flusso check` prints the same coverage report.
  Set `manage_publication = false` (or `FLUSSO_MANAGE_PUBLICATION=false` /
  `--manage-publication false`) to opt out and manage the publication yourself. Slot and
  publication names are CLI flags (`--slot`, `--publication`, both defaulting to `flusso`).
- **Backfill.** Before live capture, the engine asks each sink whether an index is already
  seeded and, for those that aren't, snapshots the root tables to seed them.
  `--skip-backfill` resumes live capture only.
- Requires `wal_level = logical` on the server. See the
  [`dev/`](https://github.com/alias2k/flusso/tree/main/dev) environment for a ready-to-run
  Postgres configured for this.

---

## Sinks

A sink is where assembled documents land. Each entry under `[sinks]` is a named
destination — the key is a sink name (a Postgres identifier) and `type` selects the kind:

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

Writes documents to an OpenSearch cluster via the bulk API — the sink you deploy.

`url`, `username`, and `password` each accept an [`env_or_value`](#env_or_value-references) (a literal
or a `{ env = "VAR" }` reference resolved at run time), and can also be supplied or
overridden per sink via reserved deployment-override variables — naming and precedence in
[Secrets & connection values](#secrets--connection-values).

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
| `auto_subfields` | bool | `true` | Auto-enrich `text`/`keyword` fields with a good analyzer and subfields. A field's explicit `options` always win; set `false` to emit fields bare. |

**How it owns its indexes:**

- **Explicit, fully-typed mapping.** The sink creates each index up front from the resolved
  schema mapping, `dynamic: strict` — field types come from the schema, not OpenSearch's
  dynamic guesses, and only configured fields are accepted. An index that already exists is
  left untouched.
- **Hashed name over generations.** The addressable name is `{logical}_{hash}` (the hash
  derives from the parsed index schema) — itself a **hash alias** over a concrete generation
  index `{logical}_{hash}_{gen}` that holds the data. A structural schema change moves the
  hash, so the sink writes a fresh alias + generation (re-seeded from scratch) rather than
  into the old, mismatched shape. An on-demand reindex builds the *next* generation behind
  the same hash alias and repoints atomically when it's seeded. flusso and the `flusso-query`
  client address `{logical}_{hash}`; the generation detail is documented in the
  [`flusso-sinks-opensearch` crate](https://github.com/alias2k/flusso/tree/main/libs/1-sinks/2-opensearch).
- **Convenience alias.** The bare logical name (`users`) is *also* kept as an alias on the
  current generation, so a human or ad-hoc tool can `GET /users/_search` without knowing the
  hash. Best-effort: if it fails (e.g. the cluster already has a real index named like the
  alias), flusso logs a warning and carries on — correctness never depends on it.
- **Refresh adapts to the backlog.** Created with auto-refresh disabled (`refresh_interval:
  -1`) for fast bulk seeding; on seeding completion the index is handed the configured
  `refresh_interval` (default `"10s"`) — the steady-state visibility ceiling. A `flush` also
  forces an immediate refresh whenever the pipeline has *caught up* (no backlog behind the
  batch), so search is fresh when traffic is light but bulk indexing stays cheap while a
  backlog drains. The `refresh_interval` only bounds staleness during sustained backlog —
  raise it for more write throughput under load, lower it (toward `1s`) for fresher search
  while behind.
- **Production-ready defaults.** Created indexes ship a tuned `analysis` block and, unless
  `auto_subfields` is off, well-shaped `text`/`keyword` fields — see
  [Index analysis & subfields](#index-analysis--subfields).
- **Seeding markers.** Seeded state is persisted in a hidden `flusso_meta` index, so a
  restart skips a completed backfill instead of redoing it.

#### Index analysis & subfields

The sink creates every index with a good search setup out of the box. flusso owns the
**index** (mapping + analyzers + subfields); your application owns the **queries**. The
notes below say which subfield to target for each job.

**Analyzers** (always defined, named `flusso_*`):

| Name | What it does |
| --- | --- |
| `flusso_code` | The `type: identifier` analyzer (and the analyzer on a `keyword` field's `text` subfield). Splits on punctuation, case, and letter↔digit boundaries, then lowercases and folds accents. `C-01234` indexes as `c-01234`, `c01234`, `c`, `01234`, so it's found by `C01234`, `c-01234`, or `01234` — but **not** by a fuzzy `c1234` (add `fuzziness` on the query side if you want that). Tuned for identifier-like short text. |
| `flusso_text` | The default for `type: text` — natural language. Plain tokenize + lowercase + accent fold, no code-splitting. |
| `flusso_lowercase` | A normalizer (single token, no splitting) for case- and accent-insensitive exact match and sort. |

With `text_analysis = "icu"` the folding/tokenizing swaps to the `analysis-icu` plugin
(`icu_tokenizer` / `icu_folding` / `icu_normalizer`) for stronger multilingual handling —
proper CJK/Thai segmentation and folding across every script. **The plugin must be
installed on every node** (`opensearch-plugin install analysis-icu`) or index creation
fails; `builtin` (the default) needs no plugins.

**Default subfields** (when `auto_subfields` is on — the default):

| Field type | Shape | Query each subfield for… |
| --- | --- | --- |
| `text` | `analyzer: flusso_text` + `.keyword` + `.keyword_lowercase` | the field itself → full-text search; `.keyword` → exact filter / aggregation / exact sort; `.keyword_lowercase` → case-insensitive sort & exact lookup. |
| `keyword` | `.text` (`flusso_code`) + `.keyword_lowercase` | the field itself → exact term / aggregation; `.text` → full-text search; `.keyword_lowercase` → case-insensitive sort. |

`keyword` subfields cap at `ignore_above: 256`. Other types (`long`, `date`, `boolean`, …)
and the `object`/`nested` containers are emitted as-is. Any key you set in a field's
`options` overrides the auto default for that field — e.g. your own `analyzer` replaces
`flusso_code`, and your own `fields` replaces the auto subfields wholesale.

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

Writes each operation to standard output as a JSON envelope — for development and piping
into `jq`.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `pretty` | bool | `false` | Pretty-print JSON instead of compact one-line NDJSON. |

Every envelope carries provenance and bookkeeping so a stream is self-describing: which
sink and version produced it (`sink`, `version`), when (`ts`), in what order (`seq`), the
`index`, the `op` (`upsert` / `delete`) and `id`, plus a `meta` summary (top-level field
count and serialized byte size). An `upsert` carries the `document`; a `delete` does not.

```text
{"document":{"email":"ada@x.io"},"id":"42","index":"users","meta":{"bytes":20,"fields":1},"op":"upsert","seq":1,"sink":"stdout","ts":"2026-06-03T10:20:30.123Z","version":"0.1.0"}
{"id":"7","index":"users","op":"delete","seq":2,"sink":"stdout","ts":"2026-06-03T10:20:30.124Z","version":"0.1.0"}
```

> Logs go to **stderr**, so stdout stays a clean data stream.

---

## Secrets & connection values

flusso never bakes a secret into a compiled config. A `flusso.lock` carries only the
*names* of the variables to read; the real values are read in the environment that runs the
pipeline. Compile in CI, run in prod, and the secret never rides along in between.

### env_or_value references

Anywhere a secret or connection string is expected in `flusso.toml`, give either a literal
string or a reference to an environment variable:

```toml
password = "literal-secret"          # literal — carried as-is
password = { env = "OS_PASSWORD" }   # read from $OS_PASSWORD when the pipeline runs
```

Either form is accepted wherever this guide says a value is an `env_or_value`. The variable
name is yours. Resolution is **deferred to run time** — which lets a
[compiled artifact](#compiling) travel without baking in its secrets. An unset variable
fails **at run time, not compile time** — by design, so the compile step needs no secrets.

### Reserved deployment-override variables

A few **well-known names** act as a deployment override layer: set them and the *same*
`flusso.toml` works across environments unedited (12-factor). When set, they take priority
over the file value (and the override is logged at startup):

| Variable | Overrides / fills | Notes |
| --- | --- | --- |
| `DATABASE_URL` | the source `connection_url` | The source is a singleton, so one well-known name is unambiguous. |
| `<SINK>_OPENSEARCH_URL` | a sink's `url` | `<SINK>` is the **uppercased sink name** — `[sinks.primary]` → `PRIMARY_OPENSEARCH_URL`. |
| `<SINK>_OPENSEARCH_USERNAME` | a sink's `username` | Same naming. |
| `<SINK>_OPENSEARCH_PASSWORD` | a sink's `password` | Same naming. |

The per-sink prefix means several OpenSearch sinks never collide (`PRIMARY_…`,
`SECONDARY_…`).

### Precedence

When more than one source could supply a value, highest wins:

1. **An explicit `{ env = "X" }` reference** — names its own source, **never** overridden
   by a reserved variable. (If `X` is unset, that's an error.)
2. **The reserved variable**, if set — overrides a literal in the file and fills a value
   the file omitted.
3. **The literal value** in the config.
4. Otherwise → an error, for anything required (the source URL, a sink `url`).

Shortcut: *"I asked for a specific variable"* beats *"the deployment set the well-known
one"* beats *"whatever's written in the file."*

```toml
[source]
type = "postgres"
connection_url = "postgres://localhost/dev"   # $DATABASE_URL wins if set

[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"                 # $PRIMARY_OPENSEARCH_URL wins if set
# username / password omitted → filled from
# $PRIMARY_OPENSEARCH_USERNAME / $PRIMARY_OPENSEARCH_PASSWORD
```

---

## CLI flags as env vars

Every `flusso` flag also reads a `FLUSSO_*` environment variable. **The flag wins when both
are set** — env is the fallback.

| Variable | Flag | Commands |
| --- | --- | --- |
| `FLUSSO_CONFIG` | `--config` | `build`, `check`, `run`, `design` |
| `FLUSSO_OUT` | `--out` | `build` |
| `FLUSSO_LOCK` | `--lock` | `run` |
| `FLUSSO_LOCKED` | `--locked` | `run` |
| `FLUSSO_SLOT` | `--slot` | `run` |
| `FLUSSO_PUBLICATION` | `--publication` | `run`, `check` |
| `FLUSSO_MANAGE_PUBLICATION` | `--manage-publication` | `run`, `check` |
| `FLUSSO_SKIP_BACKFILL` | `--skip-backfill` | `run` |
| `FLUSSO_PRETTY` | `--pretty` | `run` |
| `FLUSSO_QUEUE_CAPACITY` | `--queue-capacity` | `run` |
| `FLUSSO_PUBLIC_ADDRESS` | `--public-address` | `run` |
| `FLUSSO_PRIVATE_ADDRESS` | `--private-address` | `run` |
| `FLUSSO_ADMIN_USER` | `--admin-user` | `run`, `indexes`, `reindex` |
| `FLUSSO_ADMIN_PASSWORD` | `--admin-password` | `run`, `indexes`, `reindex` |
| `FLUSSO_SERVER` | `--server` | `indexes`, `reindex` |
| `FLUSSO_LAG_POLL_SECS` | `--lag-poll-secs` | `run` |
| `FLUSSO_INDEX_PREFIX` | `--index-prefix` | `run` |
| `FLUSSO_OFFLINE` | `--offline` | `check` |
| `FLUSSO_FORMAT` | `--format` | `check` |
| `FLUSSO_DESIGN_ADDRESS` | `--address` | `design` |
| `FLUSSO_SCHEMA` | the schema-kind argument | `schema` |

`flusso <cmd> --help` shows the matching `[env: FLUSSO_…]` next to each flag.

The two HTTP surfaces' bind addresses also fall back to a `[server]` table in
`flusso.toml` — see [HTTP surfaces](#http-surfaces).

---

## HTTP surfaces

flusso serves two HTTP surfaces; both read the daemon's live status.

| Surface | Default bind | Auth | Endpoints |
| --- | --- | --- | --- |
| **Public** (read-only) | `127.0.0.1:9464` | none | `/healthz` `/readyz` `/status` `/metrics` |
| **Private** (control) | `127.0.0.1:9465` | HTTP Basic | `/indexes`, `/reindex` |

- **Bind address** — `--public-address` / `--private-address`, the `FLUSSO_PUBLIC_ADDRESS` / `FLUSSO_PRIVATE_ADDRESS` env vars, or a `[server]` table in `flusso.toml`. Precedence: **flag > env > `[server]` config > default**.
- **Basic-auth credentials** — `--admin-user` / `--admin-password` (default `admin` / `flusso`). Flag/env only, never the config file — they're secrets. The `indexes` / `reindex` client subcommands reuse them and take `--server` / `FLUSSO_SERVER` to address a running server's private surface.

> ⚠️ **Warning** — The default control-surface credentials are `admin` / `flusso`. Change them before binding the private surface anywhere but localhost.

---

## Index prefix

`--index-prefix` / `FLUSSO_INDEX_PREFIX` (also the `prefix` key in `flusso.toml`) prepends a
literal string to **every** index name flusso owns — the indexes, their aliases, and the
`flusso_meta` index. Precedence is **flag > env > config > none**. Use it to run several
deployments against **one** OpenSearch cluster without collision: set `dev_`, `staging_`,
`nightly_`, and each gets its own `dev_users` / `staging_users` / … with independent seed
state.

- **You include the separator.** `dev_` → `dev_users`; `dev` → `devusers`.
- **Validated at startup.** Lowercase, no characters OpenSearch forbids, and a leading
  letter or digit (an index name can't start with `_`/`-`/`+`). A bad prefix fails the run
  fast.
- **Read side must match.** The `flusso-query` consumer applies the prefix at runtime
  (`Client::index_prefix`, typically wired from the same `FLUSSO_INDEX_PREFIX`) — see
  [querying](querying.md). The compile-time derive is unaffected, so one consumer binary
  serves every environment.
- **Changing it re-roots everything.** Turning a prefix on (or changing it) points flusso
  at brand-new names and triggers a full reseed; the old indexes/aliases are left orphaned.

---

## Logging & telemetry

| Variable | Default | Effect |
| --- | --- | --- |
| `RUST_LOG` | `info` | Log verbosity / filtering ([`tracing` env filter](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) syntax, e.g. `flusso=debug,info`). |
| `FLUSSO_LOG_FORMAT` | text | Set to `json` for structured JSON logs (one object per line). |
| `NO_COLOR` | unset | Set to anything to disable colored CLI output (also auto-off when not a TTY). |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | unset | Base OTLP endpoint. Its presence *turns on* trace + metric export. |
| `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | unset | Traces-only endpoint (enables trace export on its own). |
| `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` | unset | Metrics-only endpoint (enables metric export on its own). |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `http/protobuf` | OTLP transport for both signals: `http/protobuf` or `grpc`. Unrecognized values warn and fall back to `http/protobuf`. |
| `OTEL_EXPORTER_OTLP_TRACES_PROTOCOL` | (general) | Per-signal transport override for traces. |
| `OTEL_EXPORTER_OTLP_METRICS_PROTOCOL` | (general) | Per-signal transport override for metrics. |

With no OTLP endpoint set, the exporters aren't installed and cost nothing — telemetry is
opt-in. When an endpoint *is* configured, the rest of the standard `OTEL_*` variables
(`OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_SERVICE_NAME`, …) are honored by the OpenTelemetry SDK.

**Transport and port go together.** flusso defaults to OTLP over HTTP/protobuf
(conventionally port `:4318`). Set `OTEL_EXPORTER_OTLP_PROTOCOL=grpc` to switch to OTLP/gRPC
(conventionally `:4317` — the OpenTelemetry Collector's and Jaeger's default receiver).
flusso does **not** infer the protocol from the port, so the endpoint must match the
protocol you choose: pointing the default HTTP exporter at a gRPC `:4317` port just loops
`HTTP export failed: network error`. The per-signal `*_TRACES_PROTOCOL` /
`*_METRICS_PROTOCOL` override the general one.

> Prometheus metrics are a separate, pull-based path: served at `/metrics` on the public
> surface (default `127.0.0.1:9464`), no env var required.

### The derive (compile-time)

`#[derive(FlussoDocument)]` (the [`flusso-query`](querying.md) client) reads **`FLUSSO_CONFIG`
at compile time** to locate `flusso.toml` when it can't be found by walking up from the
crate's `CARGO_MANIFEST_DIR`. Same name as the CLI flag, consumed by the proc-macro instead
of the binary. (You can also point a single struct at a config with
`#[flusso(config = "…")]`.)

---

## Compiling

`flusso build --config config.toml -o flusso.lock` runs all validation and writes the whole
validated configuration — every schema inlined — to a single binary artifact (MessagePack).
Because schemas are self-describing and secrets are
[deferred](#env_or_value-references), compiling needs no database and bakes in
no secret: `{ env = … }` references travel as references, not values.

`flusso run` with no `--config` loads that artifact and resolves the connection and
credentials in its own environment; `flusso run --config flusso.toml` compiles from source
and runs that. So a deployment ships one file — no YAML tree, no source checkout — and the
same artifact runs anywhere its environment provides the secrets. The Docker shipping
recipes are in [deploying](deploying.md).

---

## Cheat sheet

```sh
# secrets & connections (resolved at run time)
DATABASE_URL=postgres://user:pass@host:5432/db
PRIMARY_OPENSEARCH_URL=https://opensearch:9200
PRIMARY_OPENSEARCH_USERNAME=flusso
PRIMARY_OPENSEARCH_PASSWORD=…           # plus any names you used in { env = "…" }

# CLI flags (flag wins if both set) — see the table above for the full list
FLUSSO_CONFIG=flusso.toml
FLUSSO_SLOT=flusso
FLUSSO_PUBLICATION=flusso
FLUSSO_MANAGE_PUBLICATION=false         # off = never issue publication DDL, only warn
FLUSSO_PUBLIC_ADDRESS=0.0.0.0:9464      # read-only surface (health/status/metrics)
FLUSSO_PRIVATE_ADDRESS=0.0.0.0:9465     # control surface (indexes/reindex), Basic auth
FLUSSO_ADMIN_USER=admin                 # change these before exposing the private port!
FLUSSO_ADMIN_PASSWORD=change-me
FLUSSO_SKIP_BACKFILL=true

# logging & telemetry
RUST_LOG=flusso=debug,info
FLUSSO_LOG_FORMAT=json
OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4318
```

---

## A complete example

`flusso.toml`:

```toml
[source]
type = "postgres"
connection_url = { env = "DATABASE_URL" }

[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }

[sinks.audit]
type = "stdout"
pretty = true

[[index]]
name = "users"
schema = "users.schema.yml"
enabled = true
```

The accompanying `users.schema.yml` (and the full schema-authoring reference) lives in
[schema authoring](schema-authoring.md). A change to a `users` row — or to any related row
the schema folds in — rebuilds the whole `users` document and re-emits it to every sink.
