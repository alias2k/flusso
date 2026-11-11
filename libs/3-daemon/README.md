# flusso-daemon

The `flusso` daemon — the supervisor around the [`engine`]. Owns the domain (pipeline + observable state); owns no transport.

## Owns vs doesn't

| Owns (domain) | Does **not** own (the binary's) |
| --- | --- |
| building the pluggable parts from a [`Config`] | the HTTP surface |
| running the [`engine`] | process signals |
| a [`StatusObserver`] → shared [`Status`] | the telemetry exporter |
| a [`Fence`]: the wait behind read-your-writes | putting that wait on a wire |
| polling source lag out of band | the metrics recording itself |

It is **telemetry-agnostic**: it depends only on the engine's [`Observer`]
trait, not on any metrics backend. The CLI installs a meter provider, attaches
its own metrics observer via [`Daemon::with_observer`], reads the [`Status`]
handle this exposes, serves it, and drives shutdown:

```text
  CLI ── install meter provider ─▶ Daemon::start ──▶ RunningDaemon
   │                                                   │  .status() ─▶ Arc<Status>  (CLI serves it)
   │                                                   │  .fence()  ─▶ Fence        (CLI serves it as /wait)
   └── shutdown future (signals) ─▶ RunningDaemon::run(shutdown)
```

> ℹ️ **Info** — keeping transport in the binary is what lets the daemon stay a
> pure library: a different host (a test, an embedder) can drive the same
> pipeline and read the same [`Status`] without dragging in an HTTP server or a
> metrics exporter.

## Fences

A [`Fence`] is how a client reads its own write. The client reads the source's
own position after its commit and calls [`Fence::wait`]; the wait returns once
a sink, or every sink, has every commit at or before that position **applied**
(a fetch by id sees it) or **searchable** (a search sees it), or a timeout
passes.

It only observes. The sink engines report the positions they have applied and
made searchable, the source translates them into its own terms, and the fence
compares. Nothing is flushed or refreshed because a client waits, and a
backfill is not covered. See [`FenceRequest`] and [`FenceOutcome`].
