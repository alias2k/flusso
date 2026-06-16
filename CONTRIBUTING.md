# Contributing to flusso

Thanks for your interest in improving flusso. This guide covers the practical
bits — how to get a dev environment running, the quality gate your change has to
clear, and how to submit it. The architecture tour and the deeper house rules
live in [`CLAUDE.md`](CLAUDE.md); the docs it links (`README.md`, `SCHEMA.md`,
`SOURCES_AND_SINKS.md`, `CONFIG.md`, `CLIENT.md`) cover everything else.

## Code of conduct

This project ships a [Code of Conduct](CODE_OF_CONDUCT.md). By participating you
agree to uphold it.

## Prerequisites

- A Rust toolchain — the version is pinned in `rust-toolchain.toml` and rustup
  honors it automatically.
- [`just`](https://just.systems) — the task runner: `cargo install just --locked`
- [`cargo-nextest`](https://nexte.st) — the test runner: `cargo install cargo-nextest --locked`
- Docker — for the dev stack and the Postgres end-to-end tests.

Run `just` with no arguments to list every recipe.

## Dev loop

```sh
just up          # Postgres + OpenSearch + Dashboards + Prometheus + Grafana
just check       # validate the dev config/schemas against the database
just run         # backfill + follow live changes; serves /status + /metrics
just dev         # the sync engine + the dev search API together
just down        # stop the stack (keep data); `just reset` wipes volumes
```

See [`dev/README.md`](dev/README.md) for the full walk-through.

## The quality gate

Before opening a PR, run the same gate CI runs:

```sh
just fmt         # format every crate
just lint        # clippy — workspace lints are strict (see below)
just ci          # the full gate: lint → e2e tests → doctests
```

`just ci` mirrors CI: `cargo clippy --workspace` → `cargo nextest run
--run-ignored all` → `cargo test --doc`. CI additionally runs
`cargo fmt --all --check`, `cargo check --workspace --all-targets`, and a
`cargo doc` link check — match those before assuming green. For faster
inner-loop runs, `just test` skips the Docker-backed e2e tests.

**The workspace lints are strict — they fail the build, not just warn.** In
particular: no `.unwrap()`/`.expect()` outside test code (return `Result`), no
`println!`/`eprintln!` (use `tracing`), no slice indexing (`slice[i]`). The full
list and the rationale are in [`CLAUDE.md`](CLAUDE.md) under "Workspace lints are
strict". Test code opts out with `#![allow(clippy::unwrap_used)]`.

## Submitting changes

1. Fork and branch off `main`.
2. Make your change. Keep [`CLAUDE.md`](CLAUDE.md) current — if you add/move a
   crate, change a command or test workflow, or alter the config/schema format,
   update it in the **same** change. It's a living index, not a separate task.
3. Add or update tests. The engine invariants are guarded by named tests; if you
   touch the sync loop, keep them passing (and add one if you add an invariant).
4. Run the quality gate above until it's green.
5. Open a PR against `main` with a clear description of what changed and why.
   Fill out the PR template. Link any related issue.

## Reporting bugs and requesting features

Use the issue templates — pick "Bug report" or "Feature request". For anything
security-sensitive, do **not** open a public issue; follow
[`SECURITY.md`](SECURITY.md) instead.

## License

By contributing, you agree that your contributions will be licensed under the
[Apache License, Version 2.0](LICENSE), the same license that covers the project.
