# CLI

The `flusso` binary has ten subcommands. Seven work locally; `indexes`, `reindex` and `reattach` are HTTP clients for a running server's private surface. Every flag but `check --fix` and `reattach`'s `--sink` and `--accept-gap` also reads a `FLUSSO_*` variable, listed in [Environment variables](environment.md#cli-flags); the flag wins.

| Command | Does | Needs a database? |
| --- | --- | --- |
| [`build`](#build) | Compile `flusso.toml` + schemas into one `flusso.lock`. | no |
| [`run`](#run) | Compile if a config is present, then backfill and follow. | yes |
| [`check`](#check) | Check the config, the schemas, the lock and the capture; report what won't work. | optional |
| [`show`](#show) | Print the deployment, or each index's typed mapping. | no |
| [`design`](#design) | Open the visual designer. | optional |
| [`schema`](#schema) | Print an editor-assist JSON Schema, or the adapters' Reference option tables. | no |
| [`script run`](#script) | Run an index's script fields over one document. | optional |
| [`indexes`](#indexes) | List a running server's indexes and states. | no (a running flusso) |
| [`reindex`](#reindex) | Rebuild one index on a running server. | no (a running flusso) |
| [`reattach`](#reattach) | Reattach a detached index on a sink that can't rebuild it. | no (a running flusso) |

## build

Runs the full load and validation, then writes the whole configuration, every schema inlined, as deterministic TOML. No database, no secrets baked in.

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` | Source config. |
| `-o`, `--out <path>` | `flusso.lock` | Where to write the lock. |

## run

Like `cargo run`, it compiles first. With a `flusso.toml` present (the default path or `--config`) it recompiles, **rewrites `flusso.lock`** when the bytes changed, and runs that. With no config it loads the existing lock. Connection and credentials resolve here, in the running environment. With two or more OpenSearch sinks it refuses [two on one cluster](checks.md#two-opensearch-sinks-on-one-cluster), once it has bound its HTTP listeners. Then: [validate against the database](checks.md#validation-at-start), continuity check, ensure indexes, backfill unseeded ones, follow live changes, serve the two HTTP surfaces until a signal or an error.

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` if present | Config to compile. An explicit path that doesn't exist is an error. |
| `--lock <path>` | `flusso.lock` | The lock to rewrite, or to load when there's no config. |
| `--locked` | off | Run the lock as is; no recompile, no rewrite. For deterministic deploys off a committed lock. |
| `--slot <name>` | `[source] slot`, else `flusso` | Replication slot to consume. Created when missing. |
| `--publication <name>` | `[source] publication`, else `flusso` | Publication to subscribe to. Created or extended when allowed. |
| `--manage-publication <bool>` | `[source] manage_publication`, else `true` | Whether flusso may issue publication DDL. |
| `--manage-replica-identity <bool>` | `[source] manage_replica_identity`, else `true` | Whether flusso may set `REPLICA IDENTITY FULL` on every table an index reads (a `USING INDEX` identity carrying the primary key and every parent link is kept). |
| `--strict` | off | Refuse to start on any error `check` reports from the schemas and the capture, not only on those no document builds past. See [--strict](checks.md#strict). |
| `--skip-backfill` | off | Resume live capture only. A fresh slot then only warns instead of rebuilding. |
| `--pretty` | off | Pretty-print every stdout sink's output (the default sink included). |
| `--queue-capacity <n>` | `[stream] capacity` | Deprecated and ignored, with a warning: `--max-built-mib` bounds what the lanes hold. |
| `--public-address <host:port>` | `[server] public_address`, else `127.0.0.1:9464` | Bind the public surface. |
| `--private-address <host:port>` | `[server] private_address`, else `127.0.0.1:9465` | Bind the private surface. |
| `--admin-user <user>` | `admin` | Basic-auth user for the private surface. |
| `--admin-password <pw>` | `flusso` | Basic-auth password. A loud warning is logged on every start while it's the default. |
| `--lag-poll-secs <n>` | `15` | How often to sample slot lag. |
| `--index-prefix <prefix>` | `prefix` from config, else none | Prefix every owned index name. |
| `--batch-max-changes <n>` | `[batch] max_changes`, else `256` | Changes per batch, and documents per build of one index. |
| `--batch-max-delay-ms <ms>` | `[batch] max_delay_ms`, else `50` | The longest a batch stays open while changes keep arriving, and the longest a sink holds a ready batch for its writes out so it goes out merged. |
| `--writes <n>` | `[concurrency] writes`, else `4` | Bulk writes each sink runs at once; never two holding the same document. Never written into the lock. |
| `--max-built-mib <n>` | `[concurrency] max_built_mib`, else `512` | MiB of built documents held until every sink acknowledges them; past it the ingest engine waits. 16 to 1,048,576. Never written into the lock. |
| `--index-budget-share <share>` | `[concurrency] index_budget_share`, else `0.5` | The share of `--max-built-mib` one index's live documents may hold while two or more indexes run, so the others keep building beside one in a burst. Above 0, at most 1; with one index it does nothing. See [concurrency](config-toml.md#concurrency). Never written into the lock. |
| `--max-waiting-per-index <n>` | `[concurrency] max_waiting_per_index`, else twice `--batch-max-changes` (`512`), or `--max-waiting` when that is set lower | Documents one index may have waiting to be built before flusso stops reading changes. At least 1. See [reading ahead](config-toml.md#reading-ahead). Never written into the lock. |
| `--max-waiting <n>` | `[concurrency] max_waiting`, else `2 × --builds × --batch-max-changes` (`2048`), or `--max-waiting-per-index` when that is set higher | The same over every index together, and the changes read ahead of the builds. With both set, at least `--max-waiting-per-index`. Never written into the lock. |
| `--max-pending-documents <n>` | `[concurrency] max_pending_documents`, else `1000000` | Documents every [exempt](config-toml.md#a-stuck-index) index together may have waiting before flusso stops reading changes for every index. At least 1. Never written into the lock. |
| `--backoff-initial-ms <ms>` | `[backoff] initial_ms`, else `1000` | The first wait before flusso tries again: an engine's restart, a parked lane's probe, a restaging lane's staging. See [backoff](config-toml.md#backoff). Never written into the lock. |
| `--backoff-max-ms <ms>` | `[backoff] max_ms`, else `60000` or the first wait when larger | The longest wait; at least the first. Never written into the lock. |
| `--builds <n>` | `[concurrency] builds`, else `4` | Batches of documents built at once; never two holding the same document, published in the order they started. At most 32. Never written into the lock. |
| `--shutdown-timeout-secs <n>` | `30` | How long a [stop](#stopping) may take, from the first signal to exit. Past it flusso gives up on what is left and exits `1`. `0` doesn't drain: it stops reading, abandons what is in flight (read again on the next start) and exits `0`. |
| `--exit-with-parent` | off | Stop when the process that started flusso exits, as if it had sent `SIGTERM`. As an env var, `FLUSSO_EXIT_WITH_PARENT=true` (like every boolean flag, `1` is refused). See [Stopping](#stopping). |

The adapter flags (`--slot`, `--publication`, `--manage-publication`, `--manage-replica-identity`, `--pretty`, `--queue-capacity`) are laid over the file's port tables before every table is validated against its adapter; an unknown option or type fails here, before anything connects. Both listeners are bound before the pipeline starts, so a bad address fails fast. A lock-write failure is fatal.

### Validation at start

Once connected, and before any index is touched, `run` checks the schemas and the capture as `check` does, on every start. An error no document builds past stops it; every other error is logged at `error`, counted on `flusso.index.schema_errors`, and the start goes on, unless `--strict` is set. Which is which: [Validation at start](checks.md#validation-at-start). There's no flag to skip it.

### Stopping

A stop is a graceful drain: flusso lands what it already read, confirms it to Postgres, then exits.

| What stops `run` | What flusso does |
| --- | --- |
| `SIGTERM`, `SIGINT` (Ctrl-C), `SIGHUP` | The drain below. `SIGHUP` (a closed terminal, a task runner passing on its own hang-up) is a stop, not a reload; a run started with it ignored (`nohup`) keeps ignoring it on Linux, and elsewhere `SIGHUP` keeps its default action (the process ends at once, as before 0.18). |
| The parent process exits, under `--exit-with-parent` | The same drain, logged as `the parent process <pid> exited`. Checked twice a second. A parent that is already `init` (PID 1, as in a container) is never followed. |
| **The same sender twice within 500 ms**, or **three stop signals within 1 s** from anyone | Exits at once with `128 + n`: `129` (`SIGHUP`), `130` (`SIGINT`), `143` (`SIGTERM`), logging `forced: exiting at once while <step>`. Pressing Ctrl-C twice quickly does it, directly or under a task runner. |
| On Windows: Ctrl-C, Ctrl-Break, a closed console, a system shutdown | The same drain. Windows reports no sender, so only three within 1 s force it, and Windows itself ends the process a few seconds after a closed console or a shutdown, drained or not. `--exit-with-parent` is ignored there, with a warning. |
| Any other repeat | The same request: the drain carries on, and flusso logs `stop already requested: <step> (<sink>: <n> changes in flight), <n>s left. Send the signal twice quickly to exit now` (at most once a second). |

A sender is the process that sent the signal (`siginfo`): the kernel for a terminal's Ctrl-C or hang-up, the task runner for its forwarded copy, `0` for one outside the container (`docker stop`, Kubernetes). One Ctrl-C under a task runner that forwards it is two signals from two senders, so it drains. A stop loop re-sending `SIGTERM` every second never forces. `/bin/kill` and `pkill` are a new sender each time: three of them within a second force. On macOS a terminal's Ctrl-C carries no sender, so there three presses force (two under a task runner).

The drain, in order:

1. **Stop taking in work.** The phase turns `stopping`: [`/readyz`](http.md#get-readyz) answers `503`, a new [reindex](http.md#post-indexesnamereindexsinkname) is refused with `503`. No further change is read from the replication stream and no new backfill is served.
2. **Build what was read.** The changes already read (at most the [lead](config-toml.md#reading-ahead), plus the run being read when the stop came, plus every [stuck index's](config-toml.md#a-stuck-index) documents) are built in batches as usual and published to every sink, with what was being built. An index that is [down](config-toml.md#a-failing-index) is tried once more at once, without waiting out its backoff; if it fails again, the stop exits `1` with the error. Every backfill in progress, and one waiting to retry, is abandoned at a row boundary, and its batches already queued for a sink are acknowledged without being written: its index stays unseeded, so the next start backfills it again (a backfill writes deterministic ids; what it wrote so far is overwritten). The abandoned backfill costs the stop nothing; the live changes already read still land, so under write load the stop takes as long as they do, and with a backlog that's the whole lead.
3. **Every sink lands its lane**: the writes in flight return, the rest are written (several at once, as always), each acknowledged, until nothing is left; an idle sink then makes what it wrote searchable. A stalled or unreachable sink doesn't slow the others; it is what the deadline is for.
4. **Confirm and close the stream.** The position every sink acknowledged is confirmed to the slot, flusso waits until Postgres was sent it, then ends replication. The next start resumes there and reads nothing already indexed again.
5. **Answer the fences.** An open [`/wait`](http.md#get-waitpositionposition) on `searchable` is held while a sink's refresh interval would still answer it within the deadline; then every wait still open is answered `503` "stopping".
6. **Close**: the HTTP surfaces, then the telemetry flush, each given at most 5 s of what is left.

`--shutdown-timeout-secs` (30 s) covers all of it. At the deadline flusso logs `shutdown deadline of <n>s passed while <step> (<sink>: <n> changes in flight, …)` and exits `1`. It names every sink with a lane still behind, its [`changes_in_flight`](http.md#get-status), and each of its lanes flusso doesn't write. When some documents read were never built, a line before it names each such index and how many: `the stop could not build everything it read (orders: 1200 documents read and not built); the next start reads it again`. Giving up is safe: a change no sink acknowledged was never confirmed to the slot, so the next start reads it again (at-least-once), and rebuilding a document gives the same document. Short of `--shutdown-timeout-secs 0`, a stop never exits `0` with a change read and not built.

| Exit | Means |
| --- | --- |
| `0` | Every step of the drain completed: what was read is indexed and its final position confirmed to Postgres, except the changes a [lane flusso doesn't write](#lanes-flusso-doesnt-write-at-a-stop) leaves for the next start. The one exception is `--shutdown-timeout-secs 0`, which doesn't drain and logs `stopped without draining, as asked`. An HTTP connection left open or telemetry that didn't flush in its 5 s is logged, not counted: neither is the data path. |
| `1` | The deadline passed during the drain, the final position couldn't be confirmed to Postgres (`the final position was not confirmed to the source`), or the pipeline failed. |
| `128 + n` | The drain was forced (the rule above). |

#### Lanes flusso doesn't write at a stop

A [lane](http.md#lane-health) that isn't being written, or is waiting on a reindex, doesn't hold the drain, with one exception: a parked lane whose write still can't land.

| The lane at the stop | What the stop does | Exit |
| --- | --- | --- |
| `refused` or `blocked` | Writes its oldest item once more, at once and as it was (no [fresh build](index-and-on-error.md#a-refused-lanes-fresh-build)). If it lands, the lane opens and drains. If it is refused or blocked again (a block still set, an index still closed), it holds the drain until the deadline. | `1` at the deadline, naming the lane |
| `blocked` on its seed: a rebuild's old generation [can't be retired](sink-opensearch.md#an-old-generation-that-cant-be-retired) | Tries the seed once more, at once. If it passes, the index is seeded. If it is refused again, the index is left unseeded and the drain goes on; the next start resumes the backfill, then seeds, or parks again. | `0` |
| `restaging`: its target gone, a reindex [waiting to stage](http.md#when-the-reindex-cant-stage), or an index a restart [couldn't ensure](http.md#when-a-restart-cant-ensure-an-index) | Stages nothing, retries nothing, asks for no backfill. Its changes are let go unwritten and unconfirmed. | `0` |
| `open`, its stuck item waiting for a [reindex's](http.md#a-reindex-of-a-lane-flusso-doesnt-write) backfill | Lets the stuck item go, unconfirmed; the items behind it are written as usual. The backfill is abandoned like any other. | `0` |

A change let go was never confirmed to the slot, so the next start reads it again: it stages a `restaging` index, backfills an index left unseeded (from its last checkpoint), and writes the change like any other. What it does about a reindex that was waiting: [A restart keeps or drops it](http.md#when-the-reindex-cant-stage).

**A reindex at the stop.** A new one is refused with `503`. One queued and not staged yet is dropped, and the log says `stopping: the reindex is not staged; ask again after the restart`. A staging already under way finishes but opens no lane: the lane is left `restaging`, with the reason `the reindex was staging as the stop came; the next start picks it up`.

**Two waits remain**, beside the parked lane:

- **A reindex being staged when the stop came.** The drain waits for the destination to answer that one staging.
- **A sink that fails during the stop.** It is restarted after its [`[backoff]`](config-toml.md#backoff) wait, then only makes sure its indexes exist: no backfill is asked for and no lane reopens. An index it can't ensure is left `restaging`, its reason `could not be ensured during the stop: <error>` (or `not ensured during the stop: not tried` for one it didn't get to), and let go like any `restaging` lane. That wait doubles each time the sink fails again without progress, up to `max_ms` (60 s by default), past the default 30 s deadline, so such a stop can end at the deadline with exit `1`.

Until the pipeline is started (through [validation at start](checks.md#validation-at-start)), a signal ends the process the default way, at once; a parent that exits meanwhile is noticed as soon as it is.

Whatever stops flusso must wait longer than the deadline before killing it: Kubernetes' `terminationGracePeriodSeconds` (the Helm chart sets 45), compose's `stop_grace_period` (Docker's default is 10 s: set `35s`), systemd's `TimeoutStopSec` (90 s by default), moon's `killProcessThreshold` (2 s by default). Each pairing is in [Run under a task runner or supervisor](../deploy/compile-and-run.md#run-under-a-task-runner-or-supervisor).

Without `--exit-with-parent`, a flusso whose parent dies keeps running, like any daemon: it holds its addresses and its replication slot, so the next start fails on the address with `another flusso is already serving there; it reports pid <pid>` (followed by the system's `Address already in use`; inside a container that pid is the one in its own namespace), or, on other addresses, retries the slot with `replication slot "<slot>" is in use by Postgres backend <pid> (…)`.

## check

Loads and validates the config and every schema, validates every port table against its adapter, compiles every script, and compares `flusso.lock` with what the config compiles to. Without `--offline` it also checks the schemas against the live database, the source server, and the publication and replica identities. It prints one line per section that passes, each problem in full, and a summary line, and exits non-zero on any error. Every check, finding and the report's shape: [Checks and findings](checks.md). `check` never changes the database, the schemas or the config.

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` if present | Config to check. With none, the `--lock` is checked instead. |
| `--lock <path>` | `flusso.lock` | The lock to compare with what the config compiles to, as `run` resolves it; or to check when there's no config. See [Checked by flusso check](lock.md#checked-by-flusso-check). |
| `--offline` | off | Connect to nothing: skip the database validation, the source server, and the publication and replica-identity reports. |
| `--fix` | off | Rewrite the `--lock` when it isn't what the config compiles to, as `flusso build` writes it. Fixes the lock only. No environment variable sets it. |
| `--publication <name>` | `[source] publication`, else `flusso` | Publication whose coverage to report. |
| `--manage-publication <bool>` | config, else `true` | Whether `run` would change the publication: decides whether a missing table is a [gap](checks.md#capture-gaps). |
| `--manage-replica-identity <bool>` | config, else `true` | Whether `run` would set replica identities: decides whether a missing link is a [gap](checks.md#capture-gaps). |
| `--format <human\|json>` | `human` | `json` prints the same report as one document: see [JSON output](checks.md#json-output). |
| `--color <auto\|always\|never>` | `auto` | Colour the human report. `auto` colours it when stdout is a terminal and `NO_COLOR` is unset. |

Exit status is `0` when the summary line says `check passed` (warnings allowed), else non-zero.

## show

Prints what the config declares, with no database: the deployment, or the typed mapping each index gets. It reads `--config` (or `flusso.toml` when present), else the `--lock`, and validates every port table against its adapter first.

| Subcommand | Prints |
| --- | --- |
| `show config` | The source, the stream, the sinks and the indexes. Every value an adapter treats as a secret prints as `***` when written literally; an `{ env = "VAR" }` reference prints by name (`${VAR}`, or `{"env": "VAR"}` in JSON). A disabled index is listed as `disabled`. |
| `show mapping [index]` | Each enabled index's fields with their mapped type and whether they're required, or only the index named. A disabled index named is shown, after a note that it's disabled; a name the config doesn't have fails, listing the indexes it has. A date field whose `format` flusso [completes](field-types.md#dates-and-times) gets a note first. |

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c`, `--config <path>` | `flusso.toml` if present | Config to read. |
| `--lock <path>` | `flusso.lock` | The lock to read when there's no config. |
| `--format <human\|json>` | `human` | `json` prints one document: the config as it loads, or a list of `{index, hash, fields}` with each field's full mapping. |
| `--color <auto\|always\|never>` | `auto` | As for `check`. |

```text
$ flusso show config
Source
──────
  postgres  connection_url=${PG_URL} slot=search

Stream
──────
  channel

Sinks
─────
  primary  opensearch  password=*** url=*** username=***

Indexes
───────
  labels  disabled  public.tags   pk id
  tags    enabled  public.tags   pk id

$ flusso show mapping users
! users.updatedAt: flusso appends `strict_date_optional_time` to its `format`, so the dates it writes index; dates still print in the `format`'s first pattern

Index  users
────────────
  id .............. integer  required
  email ........... keyword  required
  fullName ........ text     required
  nicknames ....... keyword  optional
  updatedAt ....... date     required
  account ......... object   required
    tier .......... keyword  required
…
```

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

`script run` builds one document, runs its index's [script fields](scripts.md) over it, and prints the document as built, what the scripts printed, and the finished document. On a failure it prints the field, the message, and the script line with a caret, and exits non-zero. A row whose document fails to build before any script runs (an [`array`](field-types.md#array) element the field can't hold) prints `the document failed to build:` with the field path and the reason, and exits non-zero too; only a missing or soft-deleted row is `no document`. The scripts are compiled exactly as `run` compiles them.

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
| `--ca-file <path>` | none | A PEM bundle of CA certificates trusted on top of the built-in Mozilla roots, for an `https://` `--server` behind a private CA. The OS trust store is never read. A missing file or one with no certificate fails before any request. |

## reindex

[`POST /indexes/<name>/reindex[?sink=<name>]`](http.md#post-indexesnamereindexsinkname) on a running server. Returns immediately with `202` and prints `reindex of users queued on primary, audit` (with `--sink primary`, `reindex of users on primary queued`); each targeted sink engine stages a fresh generation and requests its own snapshot while the current generation keeps serving. Nothing restarts. Watch `/status` for the index returning to `seeded` under that sink. See [Reindex without downtime](../operate/reindex.md).

```sh
flusso reindex users --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
flusso reindex users --sink primary --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
```

Same connection flags as `indexes`; the index name is positional, and one that isn't a valid index name is refused before anything is sent (`"<name>" is not an index name`). `--sink` (`FLUSSO_SINK`) limits the rebuild to one sink; omitted, every sink rebuilds, sharing one pass over the table. On a detached index it begins a rebuild ([Bring back a detached index](../operate/detached-index.md)). Without `--sink`, each sink the reindex [skipped](http.md#a-reindex-that-is-skipped) gets a line of its own after the queued one, exit `0`:

```text
reindex of users queued on primary
skipped on audit: this sink can't rebuild the index (backfill = false): flusso reattach users --sink audit --accept-gap
```

A refusal fails the command, exit `1`, with `server returned <status>: <body>`: a `409` when the sink named, or every sink, is skipped, a `503` while flusso stops.

## reattach

[`POST /indexes/<name>/reattach?sink=<name>&accept_gap=true`](http.md#post-indexesnamereattachsinknameaccept_gaptrue) on a running server: reattach a [detached](http.md#lane-health) index on a sink with `backfill = false`, accepting that the changes made while it was detached never reach it. See [Bring back a detached index](../operate/detached-index.md).

```sh
flusso reattach users --sink audit --accept-gap --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `<index>` | — | The index to reattach. |
| `--sink <name>` | — | The sink whose lane to reattach. Required, and never read from the environment: a reattach accepts lost changes on exactly the lane named. |
| `--accept-gap` | off | Accept the gap. Required; never read from the environment. |
| `--server`, `--admin-user`, `--admin-password`, `--ca-file` | as for [`indexes`](#indexes) | The private surface and its credentials. |

| Outcome | Prints | Exit |
| --- | --- | --- |
| Started (`202`) | `reattach of <index> on <sink> started: the record is gone and the lane is being attached; /status shows it open, with its gap` | `0` |
| No `--accept-gap` | `Error: a reattach drops the changes made to <index> while its lane on <sink> was detached; pass --accept-gap to accept that, or reindex it on a sink that can rebuild`, nothing sent | `1` |
| Not an index name | `Error: "<name>" is not an index name`, nothing sent | `1` |
| Any other answer | `Error: server returned <status>: <body>`, the [refusals](http.md#a-reattach-that-is-refused) included | `1` |
