# flusso.toml top level

One `flusso.toml` describes a deployment: one source, any number of sinks, the indexes to build, and two operational knobs. Only `[source]` is required.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `[source]` | table | — | The database rows come from. `type` selects the kind; only `postgres` exists. See [Source: Postgres](source-postgres.md). |
| `[sinks.<name>]` | table per sink | none | Named destinations. `<name>` is a [Postgres identifier](identifiers.md); `type` is `opensearch` or `stdout`. With no sinks, `run` falls back to a stdout sink. |
| `[[index]]` | array of tables | none | One entry per index to build. See [Index entries and on_error](index-and-on-error.md). |
| `on_error` | `"stop"` \| `"skip"` | `"stop"` | Global item-rejection policy; each `[[index]]` may override it. See [on_error](index-and-on-error.md#on_error). |
| `prefix` | string | `""` | Literal prefix prepended to every index name flusso owns. See [prefix](#prefix). |
| `[server]` | table | none | Bind addresses for the two HTTP surfaces. See [server](#server). |

Unknown keys are rejected. Schema paths in `[[index]]` resolve relative to the config file's directory. Loading validates both layers (this file and every referenced `*.schema.yml`) and needs no database.

## Fan-out

Every document goes to **every** configured sink. Two sinks means two writes per document, in one batch.

```toml
[sinks.primary]
type = "opensearch"
url = "https://search.internal:9200"

[sinks.audit]
type = "stdout"
```

## prefix

`prefix` is prepended to the hash alias (`{prefix}{logical}_{hash}`), its generations, the convenience alias (`{prefix}{logical}`), and the `{prefix}flusso_meta` index. Several deployments (dev, staging, nightly) can then share one cluster, each with independent seed state.

- **Include the separator.** `dev_` gives `dev_users`; `dev` gives `devusers`.
- **Validated at startup.** Lowercase, no characters OpenSearch forbids in an index name, and a leading letter or digit. A bad prefix fails the run before it touches anything.
- **Runtime overrides win.** `--index-prefix` beats `FLUSSO_INDEX_PREFIX` beats this key. See [CLI](cli.md#run).
- **Read side must match.** A `flusso-query` client sets the same prefix at runtime. See [Binding to the schema](../query/binding.md#reading-a-prefixed-deployment).
- **Changing it re-roots everything.** New names mean a full reseed; the old indexes and aliases are left behind and must be deleted by hand.

## server

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `public_address` | `host:port` | `127.0.0.1:9464` | The unauthenticated surface: `/healthz`, `/readyz`, `/status`, `/metrics`. |
| `private_address` | `host:port` | `127.0.0.1:9465` | The Basic-auth control surface: `/indexes`, `/reindex`. |

Precedence is flag, then `FLUSSO_PUBLIC_ADDRESS` / `FLUSSO_PRIVATE_ADDRESS`, then this table, then the default. The credentials for the private surface are **not** config keys; they come from `--admin-user` / `--admin-password` or their env vars only. See [HTTP endpoints](http.md).

## Example

```toml
on_error = "stop"
prefix = "prod_"

[source]
type = "postgres"
connection_url = { env = "DATABASE_URL" }
ssl_mode = "verify-full"

[sinks.primary]
type = "opensearch"
url = { env = "PRIMARY_OPENSEARCH_URL" }
username = "flusso"
password = { env = "OS_PASSWORD" }

[[index]]
name = "users"
schema = "schemas/users.schema.yml"
enabled = true

[[index]]
name = "analytics"
schema = "schemas/analytics.schema.yml"
enabled = true
on_error = "skip"

[server]
public_address = "0.0.0.0:9464"
```
