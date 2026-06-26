# flusso-daemon

The `flusso` daemon — the supervisor around the [`engine`].

It builds the pluggable parts from a validated [`Config`], wires a
[`StatusObserver`] that updates a shared [`Status`], runs the engine, and
polls source lag out of band.

It owns the **domain**: the pipeline and its observable state, and it is
telemetry-agnostic — it depends only on the engine's [`Observer`] trait, not
on any metrics backend. It does *not* own **transport**: the HTTP surface,
process signals, the telemetry exporter, *and the metrics recording itself*
live in the binary (the CLI), which installs a meter provider, attaches its
own metrics observer via [`Daemon::with_observer`], reads the [`Status`]
handle this exposes, serves it, and drives shutdown:

```text
  CLI ── install meter provider ─▶ Daemon::start ──▶ RunningDaemon
   │                                                   │  .status() ─▶ Arc<Status>  (CLI serves it)
   └── shutdown future (signals) ─▶ RunningDaemon::run(shutdown)
```
