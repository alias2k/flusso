# flusso.toml top level

One `flusso.toml` describes a deployment: one source, one stream, any number of sinks, the indexes to build, and a few operational knobs. Only `[source]` is required.

Every port table (`[source]`, `[stream]`, `[sinks.<name>]`) has the same shape: `type` names the adapter, and every other key is that adapter's own option, documented on the adapter's page. Unknown keys are rejected there.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `[source]` | table | — | The database rows come from. `type` selects the adapter; only `postgres` exists. See [Source: Postgres](source-postgres.md). |
| `[stream]` | table | `type = "channel"` | The stream between the source side and the sinks. See [Stream: channel](stream-channel.md). |
| `[sinks.<name>]` | table per sink | none | Named destinations. `<name>` is a [Postgres identifier](identifiers.md); `type` is `opensearch` or `stdout`; `backfill` is the one universal key (see [Several sinks](#several-sinks)). With no sinks, `run` adds a stdout sink named `stdout`. |
| `[[index]]` | array of tables | none | One entry per index to build, with optional per-sink options under `[index.sinks.<name>]`. See [Index entries and on_error](index-and-on-error.md). |
| `on_error` | `"stop"` \| `"skip"` | `"stop"` | Global item-rejection policy; each `[[index]]` may override it. See [on_error](index-and-on-error.md#on_error). |
| `prefix` | string | `""` | Literal prefix prepended to every index name flusso owns. See [prefix](#prefix). |
| `[batch]` | table | none | How live changes are grouped into a batch. See [batch](#batch). |
| `[concurrency]` | table | none | How many writes each sink runs at once. Never written into the lock. See [concurrency](#concurrency). |
| `[backoff]` | table | none | How long flusso waits before trying again after a failure. Never written into the lock. See [backoff](#backoff). |
| `[server]` | table | none | Bind addresses for the two HTTP surfaces. See [server](#server). |

Unknown keys are rejected. Schema paths in `[[index]]` resolve relative to the config file's directory. Loading validates both layers (this file and every referenced `*.schema.yml`) plus every port table against its adapter, and needs no database.

## Several sinks

Every document is built once and delivered to **every** configured sink, each over its own lane with its own engine. Sinks are independent: each decides its own backfill (`is_seeded` per index), a [reindex](../operate/reindex.md) targets one sink or all, and one sink's outage stops only that sink while the others keep writing. The slowest sink paces the replication slot, since the source is confirmed only what every sink has made durable.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `backfill` | bool | `true` | Whether this sink is ever backfilled. `false` makes it receive live changes only, never a snapshot: the opt-out for a stateless sink (stdout, a message bus) beside a stateful one, which would otherwise seed it on every start. |

```toml
[sinks.primary]
type = "opensearch"
url = "https://search.internal:9200"

[sinks.audit]
type = "stdout"
backfill = false
```

## prefix

`prefix` is prepended to the hash alias (`{prefix}{logical}_{hash}`), its generations, the convenience alias (`{prefix}{logical}`), and the `{prefix}flusso_meta` index. Several deployments (dev, staging, nightly) can then share one cluster, each with independent seed state.

- **Include the separator.** `dev_` gives `dev_users`; `dev` gives `devusers`.
- **Validated at startup.** Lowercase, no characters OpenSearch forbids in an index name, and a leading letter or digit. A bad prefix fails the run before it touches anything.
- **Runtime overrides win.** `--index-prefix` beats `FLUSSO_INDEX_PREFIX` beats this key. See [CLI](cli.md#run).
- **Read side must match.** A `flusso-query` client sets the same prefix at runtime. See [Binding to the schema](../query/binding.md#reading-a-prefixed-deployment).
- **Changing it re-roots everything.** New names mean a full reseed; the old indexes and aliases are left behind and must be deleted by hand.

## batch

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `max_changes` | integer ≥ 1 | `256` | A batch groups at most this many changes, and one index's build takes at most this many documents. `1` commits per change. |
| `max_delay_ms` | integer | `50` | The longest a batch stays open after its first change while changes keep arriving, in milliseconds. Also the window a backfill snapshot waits for straggling requests from other sinks, so a reindex fanned to every sink is one pass over the table, and the longest a sink holds a live batch that is ready while one of its writes is out, so batches of several indexes go out in one bulk request. |

A batch takes every change the stream has ready and is committed the moment the stream would block, so a lone change on a quiet stream is built at once and never waits for `max_delay_ms`; only a burst, which keeps the stream ready, fills batches to `max_changes`. Fewer changes per batch means more, smaller bulk requests; more means fewer round-trips under load. The defaults suit most deployments — tune only with a measured reason (see [Metrics](metrics.md)).

- **Batches are per index.** Each index groups the changes that touched it into its own batches, with its own `max_delay_ms`. One index's batch closes without waiting for another's, so a busy or slow index doesn't delay a quiet one's writes. A change that touches two indexes goes into a batch of each.
- **One build takes at most `max_changes` documents of its index.** A change that touches more documents of one index than that (a renamed brand embedded in 50,000 products) is built in parts of at most `max_changes` documents, up to [`[concurrency] builds`](#concurrency) parts at once, each its own write to the sink. So a `_bulk` request stays a predictable size, and no batch holds more than `max_changes` documents of one index in memory.
- **A split change counts once its last part is written.** Only then is it [committed](http.md#get-status), confirmed to Postgres, and covered by a [`/wait`](http.md#get-waitpositionposition). Until then a search can see some of its documents rebuilt and others not yet.

Precedence is flag (`--batch-max-changes` / `--batch-max-delay-ms`), then `FLUSSO_BATCH_MAX_CHANGES` / `FLUSSO_BATCH_MAX_DELAY_MS`, then this table, then the default. Runtime overrides are never written to the lock. See [CLI](cli.md#run).

## concurrency

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `writes` | integer 1–64 | `4` | Bulk writes each sink runs at once. `1` writes one batch at a time. A sink that emits an ordered stream (stdout) always writes one at a time. |
| `builds` | integer 1–32 | `4` | Batches of documents the ingest engine builds at once. `1` builds one batch at a time. The source's document builder keeps `builds` + 2 Postgres connections. |
| `max_built_mib` | integer 16–1,048,576 | `512` | MiB of built documents held until every sink acknowledges them. Past it the ingest engine builds nothing more until a sink acknowledges some. |
| `index_budget_share` | number above 0, at most 1 | `0.5` | The share of `max_built_mib` one index's live documents may hold while two or more indexes run. With one index it does nothing. |
| `max_waiting_per_index` | integer ≥ 1 | `2 × [batch] max_changes` (`512`), or `max_waiting` when that is set lower | Documents one index may have waiting to be built before flusso stops reading changes. See [Reading ahead](#reading-ahead). |
| `max_waiting` | integer ≥ `max_waiting_per_index` | `2 × builds × [batch] max_changes` (`2048`), or `max_waiting_per_index` when that is set higher | Documents every index together may have waiting to be built, and changes read but not yet built, before flusso stops reading. |

Several writes run at once, but never two holding the same document, so each document's later version still lands after its earlier one. Live changes go before backfill pages, except every fourth write slot both could take, so a backfill can't stall live indexing and live traffic can't starve a backfill.

End to end on the `reference` benchmark, 2 and 4 writes backfilled about 1.4× faster than one, and 8 a little slower than 4; live latency and burst drain didn't move, since resolving changes on the ingest side bounds them. (ADR 0012's 3× was raw OpenSearch bulk throughput, not flusso end to end.) Raise it for a cluster with headroom, lower it if the cluster answers `429`. Above 64 is refused. Each write holds one delivered batch, so a sink holds up to `writes` + 32 batches beside its lane: see [stream: channel](stream-channel.md).

Several builds run at once, but never two holding the same document, and each sink gets one index's built batches in the order they were started, so a document's later version still lands after its earlier one. Indexes don't wait for each other: one index's batch can land before an older batch of another. Live batches go before backfill batches, except every fourth build slot both could take. A second live build of an index starts only while the batch waiting behind its first is full, so a trickle of changes still batches itself. A backfill gains while building is what holds it back. On the `reference` benchmark, 2 and 4 builds backfilled about 15% faster than one; past that the sink's writes bound it. Peak memory rose there, from about 120 to 300–400 MiB, because built batches wait in the sink's lane. Each build holds a Postgres connection; behind PgBouncer count `builds` + 2 server connections in transaction mode. Up to `2 × builds` built batches of an index wait behind a slow one before that index starts nothing more.

**The memory budget.** A built batch is counted from its build until the last sink acknowledges it, in heap bytes (a document holds about 8–10× its JSON), at least 1 KiB a batch. A backfill holds at most half of `max_built_mib` (past the one batch a backfill holding nothing is always granted), so live changes always have room beside it. A batch larger than the whole budget still builds: a backfill batch once no backfill is held, a live batch once its own index holds no live documents. A sink that stops acknowledging fills the budget, and then ingest waits: the slowest sink paces ingest, as a full queue did before 0.18. The budget can be passed by the batches being built at the moment it fills, each by what it really built: up to `2 × builds` batches, plus one live batch per index and one backfill batch, since an index holding no live documents, and a backfill holding nothing, always get one. A live batch holds at most `[batch] max_changes` documents of one index (256 at the defaults), however many documents its changes touched: a change touching more is built as several batches, each counted here like any other. It doesn't count a sink's request bodies (`writes` × one rendered `_bulk` per sink), the source's read-ahead (`[batch] max_changes` × 16 changes with their row images), or the documents [waiting to be built](#reading-ahead) (an id and a position each, not a document). Plan the process's memory as `max_built_mib` + `2 × builds` batches + one live batch per index + one backfill batch + those two + about 100 MiB, plus 10–20% for the allocator. On Linux, `run` warns at start when `max_built_mib` alone reaches the cgroup's memory limit, or the host's memory without one. `/status` reports what is held (`memory`) and `flusso.memory.built` plots it ([Metrics](metrics.md)).

**One index's share.** With two or more indexes, one index's live documents hold at most `index_budget_share` of the budget: 256 MiB of the default 512. An index whose sink falls behind in a burst stops at its share, and the other indexes keep building in the rest. Its backfill counts with every other backfill against half the budget, not against its share, so an index being backfilled or reindexed keeps building its live changes beside it, as in 0.18. An index holding no live documents always gets one live batch, even with the budget full, so one index stuck at its share (a lane parked for hours) leaves the others building until it has [`max_waiting_per_index`](#reading-ahead) distinct documents waiting (512 by default); then reading stops for every index, as before 0.19.

- **While it waits at its share**, that index's documents wait to be built, and the stream keeps being read for the others.
- **Up to a point.** Once the waiting index has `max_waiting_per_index` distinct documents to build (512 by default), reading stops for every index until that index's sink acknowledges some, as when the whole budget is full. A burst that rewrites the same few documents never gets there; one over thousands of distinct rows of one index still pauses the others. A parked lane never acknowledges, so this is also how long one stuck index leaves the others running: until `max_waiting_per_index` distinct documents of it have changed. Raising it keeps the others running longer; the stuck index then has that many more documents to build once its lane opens.
- **A full lane holds an index the same way.** Each lane has a [backstop](stream-channel.md) of items, which can bind only when `max_built_mib` is past 1,024. With two or more indexes, an index whose lane on any sink reaches it is held as if at its share: its own builds wait for the lane, and the other indexes keep building and reading until it has `max_waiting_per_index` documents waiting.
- **With one index** it holds the whole budget, whatever the share says.

**When to change it.** The default, 512 MiB, keeps the tail of visible latency short while a backfill runs beside live changes.

- **Lower it on a memory-constrained host.** On `reference`, 64 MiB measured 82–87 MiB peak RSS, against about 210 MiB at 512. p50 latency, burst and backfill throughput were unchanged, but the p99 of visible latency rose to about 1 s from about 200 ms (0.17: 2.2–2.5 s). Below 64 MiB the backfill slows.
- **Raise it when documents are large** (a few hundred KiB of JSON or more each): a batch of them can take much of the budget, so fewer batches are in flight and builds and writes stop overlapping.
- **Raise it when many indexes are live at once**: they share the budget, each up to its share.
- **Raise it for a slow but healthy sink** (a busy or distant cluster): more built batches can wait for it, so a burst is absorbed rather than paced.
- **Lower `index_budget_share` when several indexes burst at once.** At `0.5`, two bursting indexes can fill the budget between them; `1 / number of indexes` keeps a slice for each.
- **Raise `index_budget_share` towards `1` when one index does nearly all the work** and the others are quiet. At `1` one index can fill the whole budget, as in 0.18.

Watch `flusso.memory.refused`: if it grows steadily while the sinks are healthy, the budget sets the pace and raising it helps. If it grows only while a sink is down, the budget is doing its job.

The table is a property of the machine that runs flusso, so it is **never written into `flusso.lock`**: `run` with a `flusso.toml` reads it, `run --locked` takes the flag or the env var, else the default. Precedence is flag (`--writes`, `--builds`, `--max-built-mib`, `--index-budget-share`, `--max-waiting-per-index`, `--max-waiting`), then `FLUSSO_WRITES` / `FLUSSO_BUILDS` / `FLUSSO_MAX_BUILT_MIB` / `FLUSSO_INDEX_BUDGET_SHARE` / `FLUSSO_MAX_WAITING_PER_INDEX` / `FLUSSO_MAX_WAITING`, then this table, then the default. See [CLI](cli.md#run).

**A share outside (0, 1] is refused**, `nan` included, by `check`, `build` and `run`, before anything connects. The message names where the value came from and every way to set it:

```text
[concurrency] index_budget_share = 0 (set in flusso.toml's [concurrency] index_budget_share) is outside (0, 1]: it is the share of max_built_mib one index's live documents may hold while two or more indexes run. Set a number above 0 and at most 1 (default 0.5) with flusso.toml's [concurrency] index_budget_share, --index-budget-share or FLUSSO_INDEX_BUDGET_SHARE; a flag wins over its variable, a variable over the file
```

A flag or a variable that isn't a number is refused as `` `half` is not a number; give a share above 0, at most 1 ``.

### Reading ahead

flusso reads changes ahead of the builds, up to a bound, then waits. While a batch builds, the next changes are already read, resolved to the documents they touch, and queued. A document waits there until a build takes it; one changed again while it waits is still built once.

Reading pauses while any of these holds, and goes on as soon as a build takes documents:

- one index has `max_waiting_per_index` documents waiting to be built (512 by default);
- every index together has `max_waiting` documents waiting (2,048 by default);
- `max_waiting` changes are read and not yet in a build, changes that touch no document included. An index [held back](#concurrency) at its share of the memory budget, or behind a full lane, isn't counted here, only by its documents waiting, so a held index rewriting the same few documents never stops the reading.

That bound is the **lead**. A run of changes read at once can pass it by its own size, once.

- **The defaults follow the batch and the builds.** Unset, `max_waiting_per_index` is `2 × [batch] max_changes` and `max_waiting` is `2 × builds × [batch] max_changes`, with both as `run` resolves them, flags and variables included: `--batch-max-changes 100 --builds 2` gives 200 and 400.
- **Setting one alone is always valid.** An unset `max_waiting` rises to a `max_waiting_per_index` set above its default, and an unset `max_waiting_per_index` falls to a `max_waiting` set below its default, as `[backoff]`'s `max_ms` follows `initial_ms`.
- **A stop builds the lead.** A stop reads nothing more, then builds and writes everything already read before it exits ([Stopping](cli.md#stopping)). Under a backlog that's the lead plus the run being read, so a stop can take a little longer, within `--shutdown-timeout-secs`.
- **Nothing is confirmed early.** A change read but not yet built is neither confirmed to the slot nor covered by a [fence](http.md#get-waitpositionposition), so what a restart reads again is unchanged.
- **Memory.** A waiting document is an id and the position of its oldest change, not a built document, and isn't counted in `max_built_mib`. At the defaults that's noise; a lead in the hundreds of thousands keeps that many ids in memory.

**When to change it.** The defaults leave room for two batches per index and two per build, so the next batch is read while one builds.

- **Raise `max_waiting_per_index` to let one stuck index leave the others running longer.** An index held at its [share](#concurrency) stops reading for every index at this many documents waiting. The cost: that index has more to build once its sink recovers, and a stop meanwhile has more to build. Past 2,048 the total rises with it unless `max_waiting` is set too.
- **Raise `max_waiting` when many indexes change at once.** At the defaults, four indexes with 512 documents waiting each reach it, and reading pauses for all of them.
- **Lower both to bound a stop's work**, when builds are slow and `--shutdown-timeout-secs` is short. A smaller lead leaves less overlap between reading and building.

**Refused at start**, by `check`, `build` and `run`, before anything connects. The message names where each value came from and every way to set it. `check` and `build` judge the file; `run` lays its flags and variables over it first.

- **`0`**, for either key:

  ```text
  [concurrency] max_waiting_per_index = 0 (set in flusso.toml's [concurrency] max_waiting_per_index) would stop the reading for good: it is how many documents may wait to be built before flusso stops reading changes. Set at least 1 with flusso.toml's [concurrency] max_waiting_per_index, --max-waiting-per-index or FLUSSO_MAX_WAITING_PER_INDEX; a flag wins over its variable, a variable over the file
  ```

- **`max_waiting` below `max_waiting_per_index`, both set**, wherever each was set. A total from `FLUSSO_MAX_WAITING=1000` under a file's `max_waiting_per_index = 4096`:

  ```text
  [concurrency] max_waiting = 1000 (set by FLUSSO_MAX_WAITING) is below max_waiting_per_index = 4096 (set in flusso.toml's [concurrency] max_waiting_per_index): the documents every index together may have waiting can't be fewer than one index's. Raise max_waiting with flusso.toml's [concurrency] max_waiting, --max-waiting or FLUSSO_MAX_WAITING or lower max_waiting_per_index with flusso.toml's [concurrency] max_waiting_per_index, --max-waiting-per-index or FLUSSO_MAX_WAITING_PER_INDEX, or leave one unset (it then follows the other); a flag wins over its variable, a variable over the file
  ```

- **A flag or a variable that isn't a whole number**: `` `lots` is not a whole number of documents, at least 1 ``, after the variable's name when a variable set it (`FLUSSO_MAX_WAITING: …`).

## backoff

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `initial_ms` | integer ≥ 1 | `1000` | The first wait before trying again, in milliseconds. |
| `max_ms` | integer ≥ `initial_ms` | `60000`, or `initial_ms` when that is larger | The longest wait, in milliseconds. Equal to `initial_ms`, every wait is the same. |

Each wait doubles, up to `max_ms`. One schedule times every retry flusso makes on its own:

- **A failed engine's restart**, sink or ingest, and a restarted sink's staging when it fails. The wait goes back to `initial_ms` after a run that made progress.
- **A parked lane's probe**: its oldest batch written again alone, while the lane is `refused` or `blocked` ([lane health](http.md#lane-health)). Only that lane's probe landing resets the wait.
- **A restaging lane's next staging**, when staging the index again fails.

The OpenSearch sink's bulk retry is not on this schedule: it has its own, 200 ms doubling, up to [`max_retries`](sink-opensearch.md#retries) times.

```toml
[backoff]
initial_ms = 200    # first retry after 0.2 s
max_ms = 10000      # then 0.4 s, 0.8 s, … never more than 10 s apart
```

### How the two keys resolve

| `initial_ms` | `max_ms` | Waits |
| --- | --- | --- |
| unset | unset | 1000, doubling to 60000 |
| `200` | unset | 200, doubling to 60000 |
| `120000` | unset | 120000 every time: an unset `max_ms` follows a larger `initial_ms` |
| unset | `5000` | 1000, doubling to 5000 |
| unset | `999` | refused: below the default first wait |
| `500` | `500` | 500 every time |
| `800` | `200` | refused |

### What is refused

- **`0`**, or anything but a whole number, for either key. In the file: `initial_ms = 0 would try again without waiting; it must be at least 1`. In a flag or a variable: `` `0` is not a whole number of milliseconds, at least 1 ``.
- **An unknown key** in the table.
- **`max_ms` below `initial_ms`**, refused alike by `check`, `build` and `run` (before anything connects). The message names where each value came from and every way to set it:

  ```text
  [backoff] max_ms = 800 (set in flusso.toml's [backoff] max_ms) is below initial_ms = 900 (set by FLUSSO_BACKOFF_INITIAL_MS): the longest wait must be at least the first (equal values wait a constant interval). Raise max_ms with flusso.toml's [backoff] max_ms, --backoff-max-ms or FLUSSO_BACKOFF_MAX_MS or lower initial_ms with flusso.toml's [backoff] initial_ms, --backoff-initial-ms or FLUSSO_BACKOFF_INITIAL_MS; a flag wins over its variable, a variable over the file
  ```

### Where it is set

The table is a property of the machine that runs flusso, so it is **never written into `flusso.lock`**: `run --locked` takes the flags or the variables, else the defaults. Precedence, per key:

1. The flag: `--backoff-initial-ms`, `--backoff-max-ms` ([CLI](cli.md#run)).
2. The variable: `FLUSSO_BACKOFF_INITIAL_MS`, `FLUSSO_BACKOFF_MAX_MS` ([Environment variables](environment.md#cli-flags)). An empty one counts as unset.
3. This table.
4. The default.

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
connection_url = { env = "PG_URL" }
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
