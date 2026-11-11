# CLI

The `flusso` binary has eight subcommands. Six work locally; `indexes` and `reindex` are HTTP clients for a running server's private surface. Every flag also reads a `FLUSSO_*` variable, listed in [Environment variables](environment.md#cli-flags); the flag wins.

| Command | Does | Needs a database? |
| --- | --- | --- |
| [`build`](#build) | Compile `flusso.toml` + schemas into one `flusso.lock`. | no |
| [`run`](#run) | Compile if a config is present, then backfill and follow. | yes |
| [`check`](#check) | Validate and print the typed mapping; confirm against live columns. | optional |
| [`design`](#design) | Open the visual designer. | optional |
| [`schema`](#schema) | Print an editor-assist JSON Schema, or the adapters' Reference option tables. | no |
| [`script run`](#script) | Run an index's script fields over one document. | optional |
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
| `--slot <name>` | `[source] slot`, else `flusso` | Replication slot to consume. Created when missing. |
| `--publication <name>` | `[source] publication`, else `flusso` | Publication to subscribe to. Created or extended when allowed. |
| `--manage-publication <bool>` | `[source] manage_publication`, else `true` | Whether flusso may issue publication DDL. |
| `--manage-replica-identity <bool>` | `[source] manage_replica_identity`, else `true` | Whether flusso may set `REPLICA IDENTITY FULL` on every table an index reads (a `USING INDEX` identity carrying every parent link is kept). |
| `--skip-backfill` | off | Resume live capture only. A fresh slot then only warns instead of rebuilding. |
| `--pretty` | off | Pretty-print every stdout sink's output (the default sink included). |
| `--queue-capacity <n>` | `[stream] capacity`, else `1024` | Changes buffered between capture and processing. |
| `--public-address <host:port>` | `[server] public_address`, else `127.0.0.1:9464` | Bind the public surface. |
| `--private-address <host:port>` | `[server] private_address`, else `127.0.0.1:9465` | Bind the private surface. |
| `--admin-user <user>` | `admin` | Basic-auth user for the private surface. |
| `--admin-password <pw>` | `flusso` | Basic-auth password. A loud warning is logged on every start while it's the default. |
| `--lag-poll-secs <n>` | `15` | How often to sample slot lag. |
| `--index-prefix <prefix>` | `prefix` from config, else none | Prefix every owned index name. |
| `--batch-max-changes <n>` | `[batch] max_changes`, else `256` | Commit a batch once this many live changes have accumulated. |
| `--batch-max-delay-ms <ms>` | `[batch] max_delay_ms`, else `50` | The longest a batch stays open while changes keep arriving. |

The adapter flags (`--slot`, `--publication`, `--manage-publication`, `--manage-replica-identity`, `--pretty`, `--queue-capacity`) are laid over the file's port tables before every table is validated against its adapter; an unknown option or type fails here, before anything connects. Both listeners are bound before the pipeline starts, so a bad address fails fast. A lock-write failure is fatal.

## check

Loads and validates the config and every schema, validates every port table against its adapter, then prints the source, the stream, the sinks, and each index's fully typed mapping. Without `--offline` it also checks the schemas against the live database, then prints the publication coverage report and the tables whose replica identity isn't `FULL` (see [Replica identity](source-postgres.md#replica-identity)), in two groups: the child tables that miss their parent link, and the tables that only miss the rest of the row. Each says whether `run` will fix it or gives the exact SQL. `check` never mutates the database.

**What the database check verifies.** Every problem of one run is listed together, each naming the index, the field path (`users.orders.items`), and the missing table or column, with the schema key that names it (`` `foreign_key` references unknown column `public.orders.userid` ``).

- **Fields.** Each column a field reads exists, and its database type is one the declared type accepts. A field declared `required` over a nullable column is a warning.
- **Geo points.** Both coordinate columns exist and are numeric.
- **Aggregate input.** A `sum`/`min`/`max` column's type is one `value_type` accepts; each column of an [expression](aggregates.md#arithmetic-input) exists and is numeric.
- **Relations, at any depth.** For every join and aggregate:
  - the related `table`;
  - the key: a `belongs_to` `column` on the table that holds the field, a `has_one`/`has_many`/aggregate `foreign_key` on the related table;
  - a junction's `table`, `left_key`, and `right_key`;
  - each `via` hop's `table` and `foreign_key`;
  - both sides of every `on:` pair: `local` on the table before the hop, `related` on the hop's table;
  - the columns of the value and null-check `filters` on the relation, its junction, and each hop (a `raw` filter is passed to the database unread, so it isn't checked);
  - every `order_by` column, and a join's `primary_key`.

  A missing table is reported once, and nothing on it is looked up.

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` | Config to check. |
| `--offline` | off | Files only; skip the database and the coverage report. |
| `--publication <name>` | `[source] publication`, else `flusso` | Publication whose coverage to report. |
| `--manage-publication <bool>` | config, else `true` | Affects the report's phrasing only. |
| `--manage-replica-identity <bool>` | config, else `true` | Affects the replica-identity report's phrasing only. |
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

Prints an artifact for editor assist or the docs. No database.

| Argument | Prints |
| --- | --- |
| `config` | The complete JSON Schema for `flusso.toml`: the top-level keys plus, for each port table, one alternative per registered adapter with that adapter's options. |
| `index` | The JSON Schema (authored as YAML) for `*.schema.yml`. |
| `docs` | One Markdown option table per registered adapter, the tables the Reference pages include. `--out <dir>` writes them as `<port>-<kind>.md` files instead. |

Both schemas are also published per release at `https://alias2k.github.io/flusso/schemas/<ref>/`, so an editor modeline can pin the version you run; see [Write your first schema](../author/first-schema.md).

## script

`script run` builds one document, runs its index's [script fields](scripts.md) over it, and prints the document as built, what the scripts printed, and the finished document. On a failure it prints the field, the message, and the script line with a caret, and exits non-zero. The scripts are compiled exactly as `run` compiles them.

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` | Deployment to read. |
| `--index <name>` | — | The index whose scripts to run. Required. |
| `--id <key>` | the first row | The root row's primary key, built from the database. Read as an integer, else a uuid, else text. |
| `--input <file.json>` | none | Run over this JSON document instead; no database. Whole numbers are integers, other numbers doubles. Conflicts with `--id`. |
| `--field <path>` | none | Print only this field of the finished document: `label`, `variants[0].gross`. |
| `--watch` | off | Re-run whenever a `.toml`/`.yml`/`.yaml` file under the config's directory changes (two levels deep). |

## indexes

`GET /indexes` on a running server, printed as JSON: `{"users": "seeded", …}`. States are `pending`, `backfilling`, `seeded`.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--server <addr>` | `127.0.0.1:9465` | The private surface. A bare `host:port` means `http://`. |
| `--admin-user`, `--admin-password` | `admin` / `flusso` | Its Basic-auth credentials. |

## reindex

`POST /reindex?index=<name>[&sink=<name>]` on a running server. Returns immediately with `202`; each targeted sink engine stages a fresh generation and requests its own snapshot while the current generation keeps serving. Nothing restarts. Watch `/status` for the index returning to `seeded` under that sink. See [Reindex without downtime](../operate/reindex.md).

```sh
flusso reindex users --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
flusso reindex users --sink primary --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
```

Same connection flags as `indexes`; the index name is positional. `--sink` limits the rebuild to one sink; omitted, every sink rebuilds, sharing one pass over the table.
