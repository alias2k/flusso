---
status: accepted
---

# Benchmarks: real-binary scenarios with published history

Performance is measured by **scenarios** that run the shipped `flusso` binary as a child process against a Postgres and an OpenSearch container, plus **component benches** (Criterion) for attribution. Every push to `main` records the results as data points on a `benchmarks` data branch; a script compares each point to the median of the last five and calls a **regression** only when two consecutive points cross the threshold. A PR is gated only by the in-process component benches, run A/B against `main` on the same runner.

## Considered options

- **In-process harness calling `Daemon::start`.** Would need a lib target on the CLI crate (it is binary-only) or a second copy of backend assembly, and would miss the HTTP surface, telemetry, and signal handling. Resource cost (RSS, CPU) would have to be measured inside the process being measured.
- **A child process running the real binary (chosen).** The exact artifact users run is on the path. Peak RSS and CPU time come from sampling the child. Headline metrics are read from `/status` and `/metrics`, the same surfaces an operator uses.
- **`gh-pages` branch for history.** The Pages site is rebuilt from scratch on every push, so history in the site build is lost; a separate deployment path would mean two Pages sources. A dedicated `benchmarks` data branch is folded into the one site build instead.
- **Single-point alerting.** The storage action alerts when one point moves against the previous one. Container-backed runs on shared runners are noisy; one bad point is noise until the next confirms it. A rolling median over the last five points and a two-consecutive-run rule match the glossary's definition of regression exactly, and an image-tag change restarts the window.
- **Allocation counting in the engine bench.** Criterion records time only; allocation counts would need a second output and comparison path. Peak RSS on the real binary is what the memory incidents would have shown, so that is the memory guard.

## Consequences

- `dev/bench` (`flusso-bench`, unpublished) owns the scenarios, their deterministic seed SQL, the seeded change writer, and the JSON report. The dev root tables gain `updated_at` as the latency marker.
- Component benches keep Criterion. The three Docker-backed ones run on `main` pushes for attribution; three in-process ones (engine loop, pgoutput decode over a recorded fixture, sink render) gate PRs at 10% via a same-runner A/B.
- Scale is part of every series name (`reference/ci`, `reference/default`), so CI-sized and local runs are never compared to each other.
- The soak tool (`scripts/bench-users.sh`) is not a benchmark and is renamed to `just load`.
