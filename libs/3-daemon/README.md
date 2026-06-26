# flusso-daemon

The `flusso` daemon — the supervisor around the [`engine`]. Owns the domain (pipeline + observable state); owns no transport.

## Owns vs doesn't

| Owns (domain) | Does **not** own (the binary's) |
| --- | --- |
| building the pluggable parts from a [`Config`] | the HTTP surface |
| running the [`engine`] | process signals |
| a [`StatusObserver`] → shared [`Status`] | the telemetry exporter |
| polling source lag out of band | the metrics recording itself |

It is **telemetry-agnostic**: it depends only on the engine's [`Observer`]
trait, not on any metrics backend. The CLI installs a meter provider, attaches
its own metrics observer via [`Daemon::with_observer`], reads the [`Status`]
handle this exposes, serves it, and drives shutdown:

```text
  CLI ── install meter provider ─▶ Daemon::start ──▶ RunningDaemon
   │                                                   │  .status() ─▶ Arc<Status>  (CLI serves it)
   └── shutdown future (signals) ─▶ RunningDaemon::run(shutdown)
```

> ℹ️ **Info** — keeping transport in the binary is what lets the daemon stay a
> pure library: a different host (a test, an embedder) can drive the same
> pipeline and read the same [`Status`] without dragging in an HTTP server or a
> metrics exporter.
