---
status: accepted
---

# A fence waits on the source's own position, per sink, at two observed levels

A client that writes to Postgres and then reads OpenSearch could not know when its commit had arrived (issue #136). flusso is asynchronous by design, so the read was a guess: sleep, or poll for the expected value. A **fence** replaces the guess with a wait: the client names a point in the source's change order and flusso answers once a sink has caught up with it.

The mechanism: after its commit the client reads the **source position** from the source itself (Postgres: `SELECT pg_current_wal_lsn()`) and calls `GET /wait?position=…` on the daemon. The request is held until the named sink, or every sink, has every commit at or before that position **applied** (its flush returned: a fetch by id sees it) or **searchable** (a refresh covered it: a search sees it), or until a timeout. `flusso-query` wraps it: `client.after(position)` returns a client whose reads wait first.

Three facts carry the design:

- The source can translate a stream **position** (its private sequence) into a source position: `ChangeCapture::reached(position)` is the source position fully covered once every change through `position` is processed. The Postgres adapter answers it from the map it already kept for confirmation, plus a **horizon**: the LSN through which everything read is registered, which keeps moving over keepalives and filtered commits.
- Each sink engine reports the position it has applied, and the position that has become searchable, to the observer. The daemon's `Fence` compares them with the client's position.
- A sink says when its flush becomes searchable (`FlushReport.visibility`): at once, within a duration, or not until a later immediate flush. The sink engine turns that into a searchable position with a queue and a timer.

## Considered options

- **flusso hands out its own marker for each write.** The client would have to route its writes, or a call per write, through flusso. The source already numbers its commits; reading that number costs one statement on a connection the client holds anyway. Rejected.
- **Wait on the watermark (every sink) only.** The first design. A second sink that stalls would time out every fence, even for a client that only reads the healthy one. Per-sink positions cost little once the searchable level needed per-sink bookkeeping anyway. Chosen: per sink, with all sinks as the default.
- **One level: applied.** Simple, and wrong for the common read: OpenSearch does not search a document until the next refresh, so a list query after an applied fence can still miss it. Chosen: two levels, the default being searchable because it is right for any read.
- **Force a refresh when a client waits for searchable** (OpenSearch's own `refresh=wait_for` idea). Refresh cost would then follow client traffic, which the sink's `refresh_interval` exists to prevent. Rejected: flusso controls the flow and only observes. A caught-up flush already refreshes; under a backlog the wait is bounded by `refresh_interval`.
- **Proxy every read through flusso.** One endpoint for clients and room to add behaviour OpenSearch lacks. But flusso is single-instance (one replication slot), so reads would depend on a process that cannot be replicated, and every search would pass through it. Rejected; a stateless gateway is the follow-up (#183). Only a fenced read contacts the daemon.
- **Push instead of wait**: publish a message per document once it is readable. Useful for cache invalidation and live UIs, but it cannot answer "is *my* commit in": a message names a document, not a commit. Deferred to stacked sinks (#129, #182).
- **Confirm the watermark the moment a lane acknowledges**, so the confirmed position is fresh enough to fence on. Unneeded once the fence reads each sink's own position through `reached`: confirmation timing no longer affects it, and the stream port stays as it was.

## Consequences

- **The position is the source's, not flusso's.** A client must read it *after* the commit and *outside* the transaction (inside, it lies before the commit and the fence can pass early), and from the **primary** (a replica reports its own replay position). The ports stay neutral: `source::SourcePosition` is an ordered number only the adapter parses and prints.
- **A fence never reads stale.** On timeout the client gets an error carrying how far the sink got; nothing falls back to an unfenced read.
- **Searchable is derived from time for an unforced flush.** A flush that forced no refresh is reported searchable `refresh_interval` after it returned, which is when the periodic refresh is due, not a confirmation that it completed. A refresh that runs long can make the fence pass slightly early in that one case (a backlog that never drains). A caught-up flush, the normal case, is exact.
- **With `refresh_interval = "-1"`** nothing refreshes an unforced flush, so the OpenSearch sink remembers the indexes it wrote and refreshes them on the next caught-up flush. That is the sink's own caught-up policy, and it also fixes those documents staying unsearchable until a later flush happened to touch their shard.
- **A backfill is not fenced, and neither is an index mid-reindex.** Positions belong to live changes; an index that is still seeding is reported by `/status` and `/readyz`. During an on-demand reindex the live writes land in the new generation while the alias still serves the old one (the existing behaviour, #6), so a fence can pass for a change a read will not see until the swap. A fence is per sink, not per index, so holding it for the length of a rebuild would stall every other index on that sink.
- **A fence and a confirmation stop at different points.** With a change outstanding, a fence claims everything short of that change's commit; the slot is advanced only to the end of the last covered commit, because Postgres skips a transaction whose commit record starts before the confirmed LSN.
- **A restart begins searchable.** What a run flushed without a refresh is remembered only in memory, so the OpenSearch sink refreshes each seeded index once when it stages. Fences are unavailable until staging is done, so "nothing waiting for a refresh" is true when the first one is answered.
- **Applied is reported before the acknowledgement.** The source drops a position once every sink confirmed it; the fence must have translated it by then. For the same reason the fence translates a position when it is applied and keeps the answer until it is searchable.
- **The position map changed shape**, and with it two defects went away: a transaction split across two batches was confirmed at its commit LSN after its first half (a restart in between lost the rest), and a keepalive that arrived while a change was in flight stopped the slot from following keepalives until the next watched change.
- **`FlushReport` gained a field**, which breaks a sink that builds it by literal: a libs minor.

## Amendment: positions describe themselves (ADR 0012, stage 6)

The first fact above changed. The source no longer keeps a map from position to commit: each position carries its own stamps (`fence_before`, `confirm_before`, `confirm_through`), and the engine's `Progress` records, per lane, what was routed there and has not landed or become searchable. `ChangeCapture::reached` takes that answer as a `Landed` case (`Before(o)`, `Through(p)` or `Idle`) instead of a position. The second fact changed with it: a sink engine reports each lane item landed and how far it is searchable to `Progress`, and the fence reads both levels off it per sink when a waiter checks.

- **Per sink, as before.** Each lane is registered as its own send succeeds and answers from the newest position routed to it, so a sink whose lane is full holds back no fence on another sink.
- **What still holds:** a fence claims everything short of the oldest outstanding change's commit (`o.fence_before`), and a split transaction is fenced only once its last change has landed; searchable never runs ahead of applied; each level is monotonic across a reopened source stream.
- **No longer needed:** reporting applied before the acknowledgement so the source could still translate the position, and the fence's own queue of translations taken at applied time. The position carries the answer for as long as anyone holds it.
- **One answer is lower than before.** With every routed change landed and a later one emitted but not yet routed, the fence answers the last landed change's own commit (`p.confirm_through`), not "short of the next commit": the engine no longer reports which emitted change comes next. A waiter waits until that change is routed and lands; nothing is ever claimed early.

## Amendment: a fence vouches only for what reads see (#379)

The consequence "a backfill is not fenced, and neither is an index mid-reindex" no longer holds.

- **Why it changed.** Since #377 a client rides out a restart, so a `Fresh` restart turned that documented limit into a `200` followed by a stale read.
- **What makes it possible.** Fences are per `(index, sink)` lane since ADR 0012 stage 7. Holding one lane no longer stalls the other indexes of the sink, which was the reason for the limit.
- **The decision.** Each lane tracks whether reads see what it writes, from the sink engine's staging, rebuild and seed events.
  - A wait that needs a lane being seeded or rebuilt holds until it is seeded, and passes then.
  - Past its timeout it is answered `503` with the reason `backfilling` or `rebuilding`.
  - During an on-demand reindex the serving generation gets nothing new, so the lane still vouches for what it had reached when the rebuild began.
  - After a `Fresh` source it vouches for nothing until seeded.
- **Where it's written up:** `docs/adr/0012-stage-notes/fence-rebuilding.md`.
