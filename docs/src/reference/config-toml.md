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
| `max_pending_documents` | integer ≥ 1 | `1000000` | Documents every [exempt](#a-stuck-index) index together may have waiting to be built before flusso stops reading changes for every index. |

Several writes run at once, but never two holding the same document, so each document's later version still lands after its earlier one. Live changes go before backfill pages, except every fourth write slot both could take, so a backfill can't stall live indexing and live traffic can't starve a backfill.

End to end on the `reference` benchmark, 2 and 4 writes backfilled about 1.4× faster than one, and 8 a little slower than 4; live latency and burst drain didn't move, since resolving changes on the ingest side bounds them. (ADR 0012's 3× was raw OpenSearch bulk throughput, not flusso end to end.) Raise it for a cluster with headroom, lower it if the cluster answers `429`. Above 64 is refused. Each write holds one delivered batch, so a sink holds up to `writes` + 32 batches beside its lane: see [stream: channel](stream-channel.md).

Several builds run at once, but never two holding the same document, and each sink gets one index's built batches in the order they were started, so a document's later version still lands after its earlier one. Indexes don't wait for each other: one index's batch can land before an older batch of another. Live batches go before backfill batches, except every fourth build slot both could take. A second live build of an index starts only while the batch waiting behind its first is full, so a trickle of changes still batches itself. A backfill gains while building is what holds it back. On the `reference` benchmark, 2 and 4 builds backfilled about 15% faster than one; past that the sink's writes bound it. Peak memory rose there, from about 120 to 300–400 MiB, because built batches wait in the sink's lane. Each build holds a Postgres connection; behind PgBouncer count `builds` + 2 server connections in transaction mode. Up to `2 × builds` built batches of an index wait behind a slow one before that index starts nothing more.

**Backfills of several indexes run at once.** Each index's backfill (a first seed, a [reindex](../operate/reindex.md), a fresh backfill after its target was deleted) starts as soon as its sinks have asked, beside any other index's. Their builds share the `builds` slots, and root keys are read one page at a time over every index, so backfilling more indexes opens no Postgres connection beyond `builds` + 2. Each index is marked seeded when its own backfill lands, so which one finishes first depends on how much each has to build, not on the order they were asked for.

**The memory budget.** A built batch is counted from its build until the last sink acknowledges it, in heap bytes (a document holds about 8–10× its JSON), at least 1 KiB a batch. Backfills together hold at most half of `max_built_mib`, past one batch per index backfilling (an index whose backfill holds nothing always gets one), so live changes always have room beside them. A batch larger than the whole budget still builds: a backfill batch once its index holds no backfill documents, a live batch once its index holds no live documents. A sink that stops acknowledging fills the budget, and then ingest waits: the slowest sink paces ingest, as a full queue did before 0.18. The budget can be passed by the batches being built at the moment it fills, each by what it really built: up to `2 × builds` batches, plus one live and one backfill batch per index, since an index holding no live documents, or no backfill documents, always gets one of each. A live batch holds at most `[batch] max_changes` documents of one index (256 at the defaults), however many documents its changes touched: a change touching more is built as several batches, each counted here like any other. It doesn't count a sink's request bodies (`writes` × one rendered `_bulk` per sink), the source's read-ahead (`[batch] max_changes` × 16 changes with their row images), each backfilling index's root keys (at most 1,024 at a time, about 110 KB for a one-column integer key), or the documents [waiting to be built](#reading-ahead) (an id and a position each, not a document). Nor does it count a refused lane's [fresh build](index-and-on-error.md#a-refused-lanes-fresh-build): one per parked lane, about the size of the batch it replaces. Plan the process's memory as `max_built_mib` + `2 × builds` batches + one live and one backfill batch per index + those three + about 100 MiB, plus 10–20% for the allocator. With two or more indexes, add up to 270–320 MB at the default `max_pending_documents` while a [stuck index](#a-stuck-index) is exempt. On Linux, `run` warns at start when `max_built_mib` alone reaches the cgroup's memory limit, or the host's memory without one. `/status` reports what is held (`memory`) and `flusso.memory.built` plots it ([Metrics](metrics.md)).

**One index's share.** With two or more indexes, one index's live documents hold at most `index_budget_share` of the budget: 256 MiB of the default 512. An index whose sink falls behind in a burst stops at its share, and the other indexes keep building in the rest. Its backfill counts with every other backfill against half the budget, not against its share, so an index being backfilled or reindexed keeps building its live changes beside it, as in 0.18. A backfill whose sink stops acknowledging can keep that half for as long as it is stuck; every other index's backfill still gets a batch each time its previous one is acknowledged, so it goes on, one batch in flight at a time. An index holding no live documents always gets one live batch, even with the budget full, so one index stuck at its share (a lane parked for hours) leaves the others building and reading.

- **While it waits at its share**, that index's documents wait to be built, and the stream keeps being read for the others.
- **At its waiting bound it becomes exempt.** Once the stuck index has [`max_waiting_per_index`](#reading-ahead) distinct documents to build (512 by default), and another index is working, it stops pacing the reading. Its changes are kept as documents to build, and the other indexes keep being read and written, up to `max_pending_documents` over every such index ([A stuck index](#a-stuck-index)).
- **A full lane makes an index stuck the same way.** Each lane has a [backstop](stream-channel.md) of items, which can bind only when `max_built_mib` is past 1,024. With two or more indexes, an index whose lane on any sink reaches it is stuck as if at its share: its own builds wait for the lane, the other indexes keep building and reading, and at `max_waiting_per_index` documents waiting it becomes exempt too.
- **With one index** it holds the whole budget, whatever the share says.

**When to change it.** The default, 512 MiB, keeps the tail of visible latency short while a backfill runs beside live changes.

- **Lower it on a memory-constrained host.** On `reference`, 64 MiB measured 82–87 MiB peak RSS, against about 210 MiB at 512. p50 latency, burst and backfill throughput were unchanged, but the p99 of visible latency rose to about 1 s from about 200 ms (0.17: 2.2–2.5 s). Below 64 MiB the backfill slows.
- **Raise it when documents are large** (a few hundred KiB of JSON or more each): a batch of them can take much of the budget, so fewer batches are in flight and builds and writes stop overlapping.
- **Raise it when many indexes are live at once**: they share the budget, each up to its share.
- **Raise it for a slow but healthy sink** (a busy or distant cluster): more built batches can wait for it, so a burst is absorbed rather than paced.
- **Lower `index_budget_share` when several indexes burst at once.** At `0.5`, two bursting indexes can fill the budget between them; `1 / number of indexes` keeps a slice for each.
- **Raise `index_budget_share` towards `1` when one index does nearly all the work** and the others are quiet. At `1` one index can fill the whole budget, as in 0.18.

Watch `flusso.memory.refused`: if it grows steadily while the sinks are healthy, the budget sets the pace and raising it helps. If it grows only while a sink is down, the budget is doing its job.

The table is a property of the machine that runs flusso, so it is **never written into `flusso.lock`**: `run` with a `flusso.toml` reads it, `run --locked` takes the flag or the env var, else the default. Precedence is flag (`--writes`, `--builds`, `--max-built-mib`, `--index-budget-share`, `--max-waiting-per-index`, `--max-waiting`, `--max-pending-documents`), then `FLUSSO_WRITES` / `FLUSSO_BUILDS` / `FLUSSO_MAX_BUILT_MIB` / `FLUSSO_INDEX_BUDGET_SHARE` / `FLUSSO_MAX_WAITING_PER_INDEX` / `FLUSSO_MAX_WAITING` / `FLUSSO_MAX_PENDING_DOCUMENTS`, then this table, then the default. See [CLI](cli.md#run).

**A share outside (0, 1] is refused**, `nan` included, by `check`, `build` and `run`, before anything connects. The message names where the value came from and every way to set it:

```text
[concurrency] index_budget_share = 0 (set in flusso.toml's [concurrency] index_budget_share) is outside (0, 1]: it is the share of max_built_mib one index's live documents may hold while two or more indexes run. Set a number above 0 and at most 1 (default 0.5) with flusso.toml's [concurrency] index_budget_share, --index-budget-share or FLUSSO_INDEX_BUDGET_SHARE; a flag wins over its variable, a variable over the file
```

A flag or a variable that isn't a number is refused as `` `half` is not a number; give a share above 0, at most 1 ``.

### Reading ahead

flusso reads changes ahead of the builds, up to a bound, then waits. While a batch builds, the next changes are already read, resolved to the documents they touch, and queued. A document waits there until a build takes it; one changed again while it waits is still built once.

Reading pauses while any of these holds, and goes on as soon as a build takes documents:

- one index has `max_waiting_per_index` documents waiting to be built (512 by default), unless it is [exempt](#a-stuck-index);
- the indexes that aren't exempt have `max_waiting` documents waiting together (2,048 by default);
- `max_waiting` changes are read and not yet in a build, changes that touch no document included. An index [stuck](#a-stuck-index) at its share of the memory budget, or behind a full lane, isn't counted here, only by its documents waiting, so a stuck index rewriting the same few documents never stops the reading;
- the exempt indexes have `max_pending_documents` documents waiting together (1,000,000 by default).

That bound is the **lead**. A run of changes read at once can pass it by its own size, once.

- **The defaults follow the batch and the builds.** Unset, `max_waiting_per_index` is `2 × [batch] max_changes` and `max_waiting` is `2 × builds × [batch] max_changes`, with both as `run` resolves them, flags and variables included: `--batch-max-changes 100 --builds 2` gives 200 and 400.
- **Setting one alone is always valid.** An unset `max_waiting` rises to a `max_waiting_per_index` set above its default, and an unset `max_waiting_per_index` falls to a `max_waiting` set below its default, as `[backoff]`'s `max_ms` follows `initial_ms`.
- **A stop builds the lead.** A stop reads nothing more, then builds and writes everything already read before it exits ([Stopping](cli.md#stopping)). Under a backlog that's the lead plus the run being read, plus every exempt index's documents, so a stop can take longer, within `--shutdown-timeout-secs`.
- **Nothing is confirmed early.** A change read but not yet built is neither confirmed to the slot nor covered by a [fence](http.md#get-waitpositionposition), so what a restart reads again is unchanged.
- **Memory.** A waiting document is an id and the position of its oldest change, not a built document, and isn't counted in `max_built_mib`: about 270 B (266 B steady, 314 B at the peak while the set grows, measured with a one-column integer key; a wider key costs more). The lead at its defaults is noise; a stuck index's documents can take far more ([A stuck index](#a-stuck-index)).

**When to change it.** The defaults leave room for two batches per index and two per build, so the next batch is read while one builds.

- **Raise `max_waiting_per_index` when one index builds slowly but keeps up.** A slow index paces the reading at this many documents waiting; a larger bound lets the others run further ahead of it. The cost: a stop has more to build. Past 2,048 the total rises with it unless `max_waiting` is set too. A stuck index beside one that is working stops nobody here: it becomes [exempt](#a-stuck-index).
- **Raise `max_waiting` when many indexes change at once.** At the defaults, four indexes with 512 documents waiting each reach it, and reading pauses for all of them.
- **Lower both to bound a stop's work**, when builds are slow and `--shutdown-timeout-secs` is short. A smaller lead leaves less overlap between reading and building. A stuck index's documents count apart, under `max_pending_documents`.

**Refused at start**, by `check`, `build` and `run`, before anything connects. The message names where each value came from and every way to set it. `check` and `build` judge the file; `run` lays its flags and variables over it first.

- **`0`**, for any of the three keys:

  ```text
  [concurrency] max_waiting_per_index = 0 (set in flusso.toml's [concurrency] max_waiting_per_index) would stop the reading for good: it is how many documents may wait to be built before flusso stops reading changes. Set at least 1 with flusso.toml's [concurrency] max_waiting_per_index, --max-waiting-per-index or FLUSSO_MAX_WAITING_PER_INDEX; a flag wins over its variable, a variable over the file
  ```

  ```text
  [concurrency] max_pending_documents = 0 (set by FLUSSO_MAX_PENDING_DOCUMENTS) would stop the reading as soon as an index is held: it is how many documents held indexes together may have waiting before flusso stops reading changes for every index. Set at least 1 with flusso.toml's [concurrency] max_pending_documents, --max-pending-documents or FLUSSO_MAX_PENDING_DOCUMENTS; a flag wins over its variable, a variable over the file
  ```

- **`max_waiting` below `max_waiting_per_index`, both set**, wherever each was set. A total from `FLUSSO_MAX_WAITING=1000` under a file's `max_waiting_per_index = 4096`:

  ```text
  [concurrency] max_waiting = 1000 (set by FLUSSO_MAX_WAITING) is below max_waiting_per_index = 4096 (set in flusso.toml's [concurrency] max_waiting_per_index): the documents every index together may have waiting can't be fewer than one index's. Raise max_waiting with flusso.toml's [concurrency] max_waiting, --max-waiting or FLUSSO_MAX_WAITING or lower max_waiting_per_index with flusso.toml's [concurrency] max_waiting_per_index, --max-waiting-per-index or FLUSSO_MAX_WAITING_PER_INDEX, or leave one unset (it then follows the other); a flag wins over its variable, a variable over the file
  ```

- **A flag or a variable that isn't a whole number**: `` `lots` is not a whole number of documents, at least 1 ``, after the variable's name when a variable set it (`FLUSSO_MAX_WAITING: …`).

`max_pending_documents` below `max_waiting_per_index` is accepted: an index becomes exempt at its waiting bound, and its first documents past it stop the reading.

### A stuck index

With two or more indexes, one stuck index doesn't stop the others, as long as another index is working. Its changes wait as documents to build, up to `max_pending_documents` over every such index, while the others keep being read, built and written.

**Stuck.** An index is stuck while the memory budget refuses its next live batch (its [share](#concurrency) or the whole limit is full), or while a batch of it waits on a full lane. The log calls it *held*.

**Working.** An index is working while it isn't stuck **and** nothing it sent has waited 5 seconds for an acknowledgement with none acknowledged. The 5 seconds are fixed, not a setting.

- **A batch waits until every sink acknowledged it.** A healthy sink acknowledging an index doesn't make it working while another sink holds its batches.
- **An index that sends nothing counts as working**: one with no changes, or with changes that build no document (an update of columns no document reads).

**When it becomes exempt.** A stuck index with `max_waiting_per_index` documents waiting becomes **exempt** while another index is working. Exempt, it no longer paces the reading:

- **The other indexes go on.** They are read, built and written as usual, and a [`/wait`](http.md#get-waitpositionposition) that names only them passes.
- **Its changes are kept as documents to build.** A document changed many times is still built once. These documents count only toward `max_pending_documents`.
- **At `max_pending_documents`** documents waiting over every exempt index, reading stops for every index. It goes on as they're built.

**When it stops growing.** Once its own hold ends, or no other index is working, an exempt index may build its backlog down but not add to it. Reading stops for every index while it has as many documents waiting as when it stopped growing; as it builds, it may refill up to that count. Stuck again while another index is working, it grows again.

**When it is no longer exempt.** Once fewer than `max_waiting_per_index` of its documents wait, it paces the reading at `max_waiting_per_index` like any index. So does an index that is only slow: its batches keep being built and written, however slowly, so it is never stuck.

**When it recovers.** Its hold ends once its lane or its share of the budget has room again. It builds its waiting documents oldest first, `[batch] max_changes` at a time, taking turns with the other indexes.

**A whole sink down** (every index of two or more stuck at once):

- **Reading stops within about 5 seconds** of each index's first unacknowledged batch, at `max_waiting_per_index` / `max_waiting`, as for an index that is only slow. Nothing is read toward `max_pending_documents`, so it takes no more memory and sends Postgres no queries while it waits.
- **In those first seconds**, an index already stuck may keep about 5 seconds of its changes, then stops growing at that count.
- **An index that sends nothing during the outage counts as working**, so the stuck ones read on up to `max_pending_documents`. flusso can't tell this from one stuck index beside a healthy, quiet one. The first batch that index sends and the sink doesn't acknowledge within 5 seconds stops the reading.
- **Reading goes on** once any index is working again.

**A slow acknowledgement looks stuck for a while.** A healthy sink that takes more than 5 seconds to acknowledge one write (a large backfill or reindex bulk), or a sink engine restart whose failing write runs to its timeout, makes its index count as not working for that long. An exempt neighbour stops growing, and reading may stop, until that acknowledgement plus up to a second. This is never worse than 0.18, where one stuck index stopped every index once the budget filled.

**What it doesn't change.**

- **The slot stays pinned.** Nothing past the stuck index's oldest unbuilt change is confirmed to Postgres, so WAL grows for as long as it's stuck. [`flusso.replication.slot_lag`](metrics.md) shows how much, and `pinned_by` ([`/status`](http.md#get-status), `flusso.slot.pinned_by`) names the index. Past `max_slot_wal_keep_size`, Postgres invalidates the slot.
- **With one index**, nothing changes: there is no other index to read on for.
- **A `/wait` covering the stuck index**, by naming it or by naming none, waits for it.
- **Shown per index.** `/status`'s [`ingest.indexes.<index>`](http.md#per-index-ingest-state) and the `flusso_index_*` [metrics](metrics.md) show the exemption, what holds the index and whether it works; `FlussoIndexBacklog` alerts on it ([runbook](../operate/stalled-lane.md#when-flussoindexbacklog-fires)).

**The log.** One line per index names its state, and one line names the cap:

| Level | Line | Means |
| --- | --- | --- |
| `WARN` | `index held at max_waiting_per_index: exempt, so reading goes on for the other indexes; its changes are kept as documents waiting, up to max_pending_documents for every exempt index, and the replication slot stays pinned by it, so WAL grows` | The index is exempt and growing. Carries `index` and `waiting`. |
| `WARN` | `exempt index frozen: its hold ended or no other index is making progress, so it may drain but not grow; reading stops for every index while it has this many documents waiting or more` | The index has stopped growing. Carries `index` and `waiting`, the count it stops the reading at. |
| `INFO` | `index no longer exempt: it paces the reading again at max_waiting_per_index` | Fewer than `max_waiting_per_index` of its documents wait. |
| `WARN` | `reading stopped for every index: the exempt indexes have max_pending_documents documents waiting to be built; it reads on as they are built` | The cap is reached. Carries `documents` and `max_pending_documents`. |
| `INFO` | `the exempt indexes are below max_pending_documents: reading on` | Below the cap again. Carries `documents`. |

- **At most one line a minute per index**, whichever state it names, and one a minute for the cap. The first always logs.
- **The last line is current.** Within a minute and a second of any change, the last line of an index, and the cap's, names the state it is in now. That holds on an idle stream and across an ingest engine restart too.
- **Except while the ingest engine waits to restart** after a failure: nothing is read and nothing is logged then. Its first second back logs the state.

**A stop** builds and writes every exempt index's documents too, within `--shutdown-timeout-secs`. When it can't, flusso exits `1` and names each index left with documents read and not built; the next start reads them again ([Stopping](cli.md#stopping)).

**Memory.** Each waiting document costs about 270 B ([Reading ahead](#reading-ahead)): about 270–320 MB at the default `max_pending_documents`, held only while an index is exempt, outside `max_built_mib`.

**When to change `max_pending_documents`.**

- **Lower it on a memory-constrained host.**
- **Raise it to keep the others running longer beside one stuck index**, when memory allows. The WAL the slot keeps grows either way.
- **Don't raise it for a whole sink down.** Reading stops there within seconds; beside an index that sends nothing, a larger cap only takes more memory before it stops. Fix the sink.

### A failing index

With two or more enabled indexes, a failure of one index's own work restarts that index alone, after a wait. The other indexes keep indexing, and a [`/wait`](http.md#get-waitpositionposition) that names only them passes.

| What failed | What waits | What goes on |
| --- | --- | --- |
| **Its live work**: a build that fails (Postgres refuses the document query, a connection lost mid-build) or panics, or a live document that fails to build under [`on_error = "stop"`](index-and-on-error.md#on_error) | The whole index is **down** until it restarts. A backfill it was running stops and starts again after the restart. | Every other index. |
| **Its backfill**: reading its root keys, building a backfill batch, a backfill document that fails to build under `stop` | Only the backfill, which is **retried**. | The index's live changes, and every other index. |

**What still restarts the whole ingest engine** (with [`[backoff]`](#backoff), reading again from the slot): a failure of the change stream, of resolving a change to its documents, of a lane or the request lane; any failure while flusso stops; any failure with one enabled index.

**While an index is down:**

- **Its changes are kept** as documents to build. Nothing is read again: when it restarts, it builds them oldest first, then follows the stream with the others.
- **The slot stays before its oldest unbuilt change**, so WAL grows. A `/wait` covering the index waits for it.
- **It can grow past its waiting bound.** From [`max_waiting_per_index`](#reading-ahead) documents on it is [exempt](#a-stuck-index) while another index is working, up to `max_pending_documents`. Then reading stops for every index.
- **Backfill requests for it wait** (a reindex, a new sink) and are served once it is back.

**While a backfill waits to retry:**

- **Live changes keep flowing**: the index isn't down.
- **Each sink resumes after the last batch it received.** Nothing is backfilled again from the first row.
- **A new request for the index joins the retry** (a reindex, a new sink's backfill).
- **The retry starts once no other backfill of that index runs**, so its due time can pass while one does.

**The waits** follow [`[backoff]`](#backoff): `initial_ms`, doubling to `max_ms` (1 s to 60 s by default). Each index has two schedules, one per kind of failure, each starting over on its own progress only:

- **Live**: starts over once the index hands a live batch to its sinks again.
- **Backfill**: starts over once the backfill gets further than at its last failure. A row that always fails doubles it to `max_ms`, where it stays.

**A document that fails every time** (a poison row under `stop`, a script that always throws on it) keeps its index down, tried again every `max_ms`. The other indexes go on until the down index has `max_pending_documents` documents waiting. Fix the row or the script, or set `on_error = "skip"` for that index and restart.

**A stop:**

- **With an index down**, the stop cuts its wait short and tries it once more, within `--shutdown-timeout-secs`. If that fails too, flusso exits `1` with the error. The next start reads again from the slot, which stayed at the down index's oldest change, so **the other indexes' changes since then are read and built again**. The longer the index was down, the larger that replay. Writes are idempotent, so it costs time, not correctness.
- **During a backfill's wait**, the stop exits `0`. The backfill is asked for again at the next start.

**What it costs.**

- **Memory, and reverse queries.** Beside a healthy index, a down index can grow to `max_pending_documents` documents waiting (about 270–320 MB at the default) before reading stops for every index. Meanwhile flusso keeps resolving every change for it, so its reverse queries run for an index that builds nothing.
- **It helps with one index's faults only.** A database-wide fault (Postgres down or unreachable) fails resolving changes too, and that restarts the whole ingest engine. A per-index restart covers a query one schema makes Postgres refuse, a poison row, a script.
- **A replay after a failed stop**, above.

**With one index**, every failure restarts the whole ingest engine, as before. One change: a build that panics as it starts now restarts the ingest engine too, where it used to end the deployment with `ingest engine task failed`. A panic that happens every time is an endless restart loop, paced by `[backoff]`.

**Shown per index.** `/status`'s [`ingest.indexes.<index>`](http.md#per-index-ingest-state) carries `state`, `down_since`, `last_error`, `restarts` and `backfill.state`; the metrics are `flusso_index_down`, `flusso_index_backfill_retrying` and `flusso_index_restarts_total{kind}` ([Metrics](metrics.md)). The runbook is [When an index is down or its backfill keeps failing](../operate/stalled-lane.md#when-an-index-is-down-or-its-backfill-keeps-failing).

**The log.** Each failure and each restart logs one line, carrying `index` and `kind` (`"live"` or `"backfill"`); a failure adds `error` and `retry_in`:

| Level | Line |
| --- | --- |
| `ERROR` | `the index's builder failed; it restarts alone after the wait while the other indexes go on: its documents wait, and the replication slot stays before the oldest of them` |
| `INFO` | `the index's builder restarted: building its backlog first` |
| `ERROR` | `the index's backfill failed; it is walked again after the wait, each sink from where it got, while the index's live changes keep flowing` |
| `INFO` | `the index's backfill started again, each sink from where it got` |

## backoff

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `initial_ms` | integer ≥ 1 | `1000` | The first wait before trying again, in milliseconds. |
| `max_ms` | integer ≥ `initial_ms` | `60000`, or `initial_ms` when that is larger | The longest wait, in milliseconds. Equal to `initial_ms`, every wait is the same. |

Each wait doubles, up to `max_ms`. One schedule times every retry flusso makes on its own:

- **A failed engine's restart**, sink or ingest, and a restarted sink's staging when it fails. The wait goes back to `initial_ms` after a run that made progress.
- **A parked lane's probe**: its oldest batch, or a [fresh build](index-and-on-error.md#a-refused-lanes-fresh-build) of it, written alone, while the lane is `refused` or `blocked`, or a seed refused by the old generation's block ([lane health](http.md#lane-health)). Only that lane's probe landing resets the wait.
- **A restaging lane's next staging**, when staging the index again fails. A lane a restart [couldn't ensure](http.md#when-a-restart-cant-ensure-an-index) is tried again as soon as the run starts (after one wait, if the lane had only just reopened), then on this schedule.
- **One index's restart**, with two or more indexes, after its live work failed. The wait goes back to `initial_ms` once the index hands a live batch to its sinks again ([A failing index](#a-failing-index)).
- **One index's backfill retry**, after its backfill failed. The wait goes back to `initial_ms` once the backfill gets further than at its last failure.

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
| `public_address` | `host:port` | `127.0.0.1:9464` | The unauthenticated surface: `/healthz`, `/readyz`, `/status`, `/metrics`, `/wait`, `/indexes/<name>/status`. |
| `private_address` | `host:port` | `127.0.0.1:9465` | The Basic-auth control surface: `/indexes`, `/indexes/<name>/reindex`. |

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
