# CLI

The `flusso` binary has seven subcommands. Five work locally; `indexes` and `reindex` are HTTP clients for a running server's private surface. Every flag also reads a `FLUSSO_*` variable, listed in [Environment variables](environment.md#cli-flags); the flag wins.

| Command | Does | Needs a database? |
| --- | --- | --- |
| [`build`](#build) | Compile `flusso.toml` + schemas into one `flusso.lock`. | no |
| [`run`](#run) | Compile if a config is present, then backfill and follow. | yes |
| [`check`](#check) | Validate and print the typed mapping; confirm against live columns. | optional |
| [`design`](#design) | Open the visual designer. | optional |
| [`schema`](#schema) | Print an embedded editor-assist JSON Schema. | no |
| [`indexes`](#indexes) | List a running server's indexes and states. | no (a running flusso) |
| [`reindex`](#reindex) | Rebuild one index on a running server. | no (a running flusso) |

## build

Runs the full load and validation, then writes the whole configuration, every schema inlined, as deterministic TOML. No database, no secrets baked in.

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` | Source config. |
| `-o`, `--out <path>` | `flusso.lock` | Where to write the lock. |

## run

Like `cargo run`, it compiles first. With a `flusso.toml` present (the default path or `--config`) it recompiles, **rewrites `flusso.lock`** when the bytes changed, and runs that. With no config it loads the existing lock. Connection and credentials resolve here, in the running environment. Then: continuity check, ensure indexes, backfill unseeded ones, follow live changes, serve the two HTTP surfaces until a signal or an error.

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` if present | Config to compile. An explicit path that doesn't exist is an error. |
| `--lock <path>` | `flusso.lock` | The lock to rewrite, or to load when there's no config. |
| `--locked` | off | Run the lock as is; no recompile, no rewrite. For deterministic deploys off a committed lock. |
| `--slot <name>` | `flusso` | Replication slot to consume. Created when missing. |
| `--publication <name>` | `flusso` | Publication to subscribe to. Created or extended when allowed. |
| `--manage-publication <bool>` | `[source] manage_publication`, else `true` | Whether flusso may issue publication DDL. |
| `--skip-backfill` | off | Resume live capture only. A fresh slot then only warns instead of rebuilding. |
| `--pretty` | off | Pretty-print the fallback stdout sink's output. |
| `--queue-capacity <n>` | `1024` | Changes buffered between capture and processing. |
| `--public-address <host:port>` | `[server] public_address`, else `127.0.0.1:9464` | Bind the public surface. |
| `--private-address <host:port>` | `[server] private_address`, else `127.0.0.1:9465` | Bind the private surface. |
| `--admin-user <user>` | `admin` | Basic-auth user for the private surface. |
| `--admin-password <pw>` | `flusso` | Basic-auth password. A loud warning is logged on every start while it's the default. |
| `--lag-poll-secs <n>` | `15` | How often to sample slot lag. |
| `--index-prefix <prefix>` | `prefix` from config, else none | Prefix every owned index name. |

Both listeners are bound before the pipeline starts, so a bad address fails fast. A lock-write failure is fatal.

## check

Loads and validates the config and every schema, then prints the source, the sinks, and each index's fully typed mapping. Without `--offline` it also confirms every declared type and nullability against the live columns and prints the publication coverage report with the exact SQL for any gap. `check` never mutates the database.

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` | Config to check. |
| `--offline` | off | Files only; skip the database and the coverage report. |
| `--publication <name>` | `flusso` | Publication whose coverage to report. |
| `--manage-publication <bool>` | config, else `true` | Affects the report's phrasing only. |
| `--format <human\|json>` | `human` | `json` prints one machine-readable document. |

Exit status is non-zero when validation fails or the schema disagrees with the database.

## design

Opens the visual designer, a local web UI over the config and its schemas. Binds to localhost and opens a browser tab. Absent from the server-only build (`--no-default-features`).

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` | Deployment to edit. May not exist yet. |
| `--address <host:port>` | `127.0.0.1:7700` | Bind address. |
| `--no-open` | off | Print the URL instead of opening a browser. |

## schema

Prints the embedded JSON Schema for editor completion. It's compiled into the binary, so it always matches the installed version.

```sh
flusso schema config > config.schema.json    # for flusso.toml
flusso schema index  > index.schema.yml      # for *.schema.yml
```

Versioned copies are published per release at `https://alias2k.github.io/flusso/schemas/<ref>/…`, where `<ref>` is `latest`, a minor like `v0.16`, or an exact `v0.16.0`. A `# yaml-language-server: $schema=…` modeline pins one.

## indexes

`GET /indexes` on a running server, printed as JSON: `{"users": "seeded", …}`. States are `pending`, `backfilling`, `seeded`.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--server <addr>` | `127.0.0.1:9465` | The private surface. A bare `host:port` means `http://`. |
| `--admin-user`, `--admin-password` | `admin` / `flusso` | Its Basic-auth credentials. |

## reindex

`POST /reindex?index=<name>` on a running server. Returns immediately with `202`; the server stages a fresh generation and restarts its pipeline to fill it while the current generation keeps serving. Watch `/status` for the index returning to `seeded`. See [Reindex without downtime](../operate/reindex.md).

```sh
flusso reindex users --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
```

Same connection flags as `indexes`; the index name is positional.
