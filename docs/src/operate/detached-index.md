# Bring back a detached index

A detached index stays readable but frozen. A reindex rebuilds it on a sink that can backfill; `flusso reattach --accept-gap` reopens it on one that can't, leaving a gap for good.

## When to use this

`/status` lists the lane under [`detached`](../reference/http.md#get-status), and its `health` reads `detached`. A **detached lane** is one `(index, sink)` lane that flusso stopped writing *and* took out of the replication slot's minimum: the slot moves on without it, so Postgres keeps no WAL for it. In exchange its index misses every change from the detach on.

- **Reads still work.** The generation that served at the detach keeps serving through the same aliases, as it stood then.
- **Nothing reaches it.** Changes made since aren't applied to that index on that sink. Its other sinks, and the sink's other indexes, carry on.
- **Fences stop at the detach.** A [`/wait`](../reference/http.md#get-waitpositionposition) for a position the index had reached passes; one past it is answered at once with `503` and the reason `detached`.
- **It lasts.** The detach is recorded in the sink's [meta documents](../reference/sink-opensearch.md#the-meta-documents), and every start brings the index back detached.
- **Alerts and readiness.** A detached lane is not parked: `flusso_lane_parked` has no series for it, so `FlussoLaneParked` doesn't fire, and [`/readyz`](../reference/http.md#get-readyz) stays `200`. Watch `/status`'s `detached` list.

> ℹ️ **Info** — Nothing in 0.19 detaches an index on its own. A lane is detached only when the sink's `{prefix}flusso_meta` already holds a detach record for its index. This page is what to do then.

## Steps

1. **Find the detached lanes.**

   ```sh
   curl -s localhost:9464/status | jq '.detached'
   ```

   ```json
   [{ "index": "users", "sink": "primary" }]
   ```

   The lane itself, under `sinks.primary.lanes.users`, carries `health: "detached"`, `parked_since` (when it was detached) and `reason` ([the reasons](../reference/http.md#lane-health)).

2. **Pick the way back by the sink's [`backfill`](../reference/config-toml.md#several-sinks).**

   | Sink | Way back | What the index misses afterwards |
   | --- | --- | --- |
   | `backfill = true` (the default) | A **rebuild**: `flusso reindex users --sink primary` | nothing |
   | `backfill = false` | A **reattach**: `flusso reattach users --sink audit --accept-gap` | the changes made while it was detached, for good |

3. **Rebuild** (a sink that can backfill). A reindex of a detached index builds a new generation from the database, like any [reindex](reindex.md):

   ```sh
   flusso reindex users --sink primary --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
   ```

   In `/status` the lane carries `rebuilding: <n>` (the rebuilds the record counts) from the start, and `attaching: true` until it opens; then it backfills, and the index goes `backfilling`, then `seeded`. Reads keep hitting the frozen generation until the seed swaps the alias. Until then a `/wait` on the index answers `rebuilding`. After the seed the record is gone and nothing is missing.

4. **Reattach** (a sink with `backfill = false`, which can't rebuild):

   ```sh
   flusso reattach users --sink audit --accept-gap --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
   ```

   The command prints `reattach of users on audit started: …` once the sink engine has checked the records, kept the gap and deleted the record. The lane shows `attaching: true`, then opens with its gap listed under `gaps`. From there on every change reaches the index again. Without `--accept-gap` the command refuses before sending anything.

5. **Confirm.** `jq '.detached'` no longer lists the lane, and its `health` is `open`.

## What a gap means

A **gap** is the run of changes a reattach accepted as missing: from where reads stood at the detach (`from`) to where the lane was attached again (`to`), both in the source's own text.

- **Documents changed in the gap stay as they were** until their rows change again: flusso builds each document whole from the database, so the next change to a row brings its document up to date.
- **Rows deleted in the gap** keep their documents: no later change names them.
- **A fence inside a gap fails at once.** A `/wait` for a position in it answers `409` with the reason `gap` and the gap; waiting can't help. A position after the reattach is vouched for as usual.
- **Several detaches, several gaps.** Each reattach adds its own to the list kept with the seed marker.
- **Only a rebuild ends them**, once seeded: [Ending a gap](#ending-a-gap).
- **`settling: true`** on a gap means its end isn't known yet (a reattach cut short). flusso settles it at its next read of the records while it holds the slot; meanwhile every position after `from` is refused.

## Ending a gap

A gap ends only when the index is seeded into a **new generation**. A backfill writes only the rows that still exist, so a backfill into the generation reads see would leave the documents of rows deleted in the gap in place.

- **On a `backfill = false` sink** nothing seeds, so the gaps stay for good.
- **`backfill` turned back on, the index unseeded** (a sink that was live only from the start): flusso doesn't backfill in place. It rebuilds the index into a new generation, as `flusso reindex` does, and logs a `WARN`: `the index keeps a gap on the generation reads see, where rows deleted in the gap keep their documents: it is rebuilt into a new generation, and the gap stays until that rebuild is seeded`. Until that seed, `/status` shows the index `backfilling`, and a `/wait` answers `409` with the reason `gap` inside the gap and `rebuilding` outside it. While the rebuild waits for the slot, the lane is `restaging` with the reason `the index keeps a gap on the generation reads see, …; it could not stage: <error>`.
- **`backfill` turned back on, the index seeded** (it was seeded before a live-only detach): nothing rebuilds it on its own. The gaps keep answering `409` until you run `flusso reindex <index> --sink <sink>`.
- **At the seed** the gaps are gone, and so are the documents of rows deleted in them.

A rebuild or reindex that hasn't seeded yet, a recreated generation, or `backfill` turned on and off again keeps every gap.

## Options and variations

- **A reindex naming no sink** rebuilds where it can and lists in `skipped` the lanes it didn't touch, with why. A detached lane on a `backfill = false` sink is one of them, its reason naming the `flusso reattach` command to run.
- **A reindex that is refused.** A rebuild that is starting, running, or waiting on a block on the serving generation takes no second reindex: `409` with the reason ([details](../reference/http.md#post-indexesnamereindexsinkname)). A rebuild parked for another reason starts again.
- **A rebuild that can't begin** leaves the lane detached, its reason `the rebuild could not begin: <why>; tried again in <d>`, and is tried again on the [`[backoff]`](../reference/config-toml.md#backoff) schedule.
- **A restart during a rebuild** resumes it. A start that can't backfill (a `backfill = false` sink, or `--skip-backfill`) puts the index back to detached instead: reattach it, or reindex it again on a start that backfills.
- **A reattach that is refused** answers `409` with the reason and what works instead: [the cases](../reference/http.md#a-reattach-that-is-refused). Where the reason says to restart flusso, a start on the `backfill = false` sink puts the record back so the reattach applies, and logs `a detach record written while this sink could rebuild, or beside a seed marker moved off the generation reads see, is put back to detached, live only, on that generation: flusso reattach --accept-gap applies`.
- **While flusso doesn't hold the slot** (a reconnect), a reattach is refused and a rebuild waits: see [Run one flusso per slot](one-process-per-slot.md#while-flusso-doesnt-hold-the-slot).

## Known limits

**A generation deleted by hand.** After a generation index of a detached or rebuilding index is deleted by hand, its lane may not reattach or rebuild: the records can name a generation that is gone, or one whose number was taken again. A rebuild is then never taken as finished, so every start takes it over again, and a live-only lane is never reattached; restarting flusso doesn't change that. There is no way out from flusso yet: see [issue #492](https://github.com/alias2k/flusso/issues/492). Don't delete a generation index of a detached or rebuilding index by hand.

**A start that can't backfill, during a rebuild.** A start with `--skip-backfill`, or with the sink switched to `backfill = false`, in the middle of a rebuild lets `/wait` answer for writes that reads can't see yet: see [issue #493](https://github.com/alias2k/flusso/issues/493). Don't start with `--skip-backfill`, or switch a sink to `backfill = false`, while an index is being rebuilt.

## Related

- [HTTP endpoints](../reference/http.md#lane-health): lane states and reasons.
- [CLI](../reference/cli.md#reattach): `flusso reattach`.
- [Read your own writes](read-your-writes.md): fences.
