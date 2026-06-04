# flusso sources and sinks

flusso connects one **source** — where rows come from — to one or more **sinks**
— where the built documents go. They are configured under `[source]` and
`[sinks.<name>]` in `config.toml`; this document is the reference for every
supported type and its options.

For the overall `config.toml` structure, the `env_or_value` form, and the
reserved-environment-variable override rules referenced throughout, see
[`SCHEMA.md`](SCHEMA.md). For the index document format, see the same file.

## The model

- **One source, many sinks.** A deployment reads from a single source and writes
  every document to *all* configured sinks (**fan-out**). Define as many as you
  need; if none are defined, the CLI falls back to a single [stdout](#stdout) sink.
- **Pluggable.** Source, sink, and the in-process queue are trait objects, so
  the backend choices below can be swapped without touching the engine. The
  types documented here are the ones implemented today.

---

## Sources

### Postgres

```toml
[source]
type = "postgres"
connection_url = "postgresql://user:pass@localhost:5432/mydb"
```

The only source type today. flusso follows Postgres' **logical replication**
stream to capture changes, and snapshots tables to seed an index before
following live changes.

#### Connection

`connection_url` takes one of two shapes.

**A full URL** — a string or [`env_or_value`](SCHEMA.md#env_or_value). Must match
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

The reserved **`DATABASE_URL`** environment variable overrides whichever shape is
configured, and fills an omitted `connection_url` — see
[Reserved environment variables](SCHEMA.md#reserved-environment-variables) for
the precedence.

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
  this.

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

Writes documents to an OpenSearch cluster via the bulk API.

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

`url`, `username`, and `password` can also be supplied or overridden per sink via
the reserved `<SINK>_OPENSEARCH_URL` / `_USERNAME` / `_PASSWORD` environment
variables (for `[sinks.primary]`, that's `PRIMARY_OPENSEARCH_*`). See
[Reserved environment variables](SCHEMA.md#reserved-environment-variables).

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
- **Refresh follows the index lifecycle.** Created with auto-refresh disabled
  (`refresh_interval: -1`) for fast bulk seeding; on seeding completion the index
  is refreshed once and handed back to the cluster's default interval. In steady
  state, visibility is automatic — `flush` does not force a refresh.
- **Seeding markers.** Seeded state is persisted in a hidden `flusso_meta` index,
  so a restart skips a completed backfill.

### Stdout

```toml
[sinks.audit]
type = "stdout"
pretty = true
```

Writes each operation to standard output as a JSON envelope — handy for
development and for piping into `jq`.

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
