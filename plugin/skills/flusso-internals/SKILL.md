---
name: flusso-internals
description: Change the flusso Rust codebase — add a source/sink backend, edit the engine pipeline, the config/schema layers, or the query derive. Use when contributing to flusso's implementation rather than authoring config.
---

# Working on the flusso implementation

For authoring config use **flusso-schema**; for the read client use **flusso-query**. This skill is
for changing the Rust.

**The repo's `CLAUDE.md` owns the detail and is the only current copy.** It is a living index kept
in lockstep with the code, and it is already loaded in any checkout, so this skill does not restate
it. Changing the codebase requires a checkout; without one, there is nothing here to act on.

## Where to look in `CLAUDE.md`

| Task | Section |
| --- | --- |
| Find the file for anything | "Where things live" — the fastest jump-to map, start here |
| Touch the sync loop, batching, or ack ordering | "The pipeline" — the invariants and each one's guard test |
| Add or change a config / schema key | "Config layer — two-stage parse then convert", **plus the format-freeze bullet under "Commands"**: the formats are frozen backwards for the major, so deprecate rather than remove, and `golden_lock.rs` / `compat.rs` will tell you when you broke it |
| Change the query derive | "Query side", plus the `flusso-query-derive` memory note |
| Release, or get a libs fix into the shipped binaries | "Conventions" — releases are three independent trains, and a libs fix reaches the binaries only through a `fix(cli): adopt …` commit on `apps/cli/ADOPTIONS.md` |
| Before claiming green | "Workspace lints are strict", and the CI-order bullet under "Commands" — note a PR confined to the query train skips the container suites it can't break |
| Anything a user authors | "Keeping the designer current" — the designer and its translations ship in the same change, and two CI guards enforce it |

## Crate layering — the numeric prefix is the dependency layer

```
0-kernel  →  1-{ports/*, config}  →  2-{adapters/*, engine}  →  3-daemon  →  apps, sdk
```

A crate depends only on lower-numbered layers. Keep it acyclic. The five seams are named in the
repo's `CONTEXT.md` (kernel, ports, adapters, engine, daemon). Three placements carry rationale
worth knowing before you move code: the config crate and the adapters both sit above the kernel
and below the engine, so an adapter **cannot** reach the assembled `Config` and the config crate
never names an adapter (each port table is a `type` plus an opaque options tree the adapter
validates); `libs/3-daemon` assembles the pipeline while naming no adapter; and the published
`flusso-query` has **zero** flusso adapter dependencies, so its live e2e lives in the unpublished
`dev/query-e2e` crate. Keep it that way.

## The seams — where new things attach

- **A source, stream, or sink adapter** → its crate under `libs/2-adapters/<port>-<technology>`
  with a `#[derive(AdapterConfig)]` config struct, plus one entry in the registry at
  `apps/cli/src/adapters.rs` and the build arm in `apps/cli/src/backends.rs`. The engine, daemon,
  and config crate stay untouched; the editor schema, the Reference table, and the designer form
  come from the derive (`just schema-gen` regenerates the committed copies). The source builder
  takes a `SourceSpec`, not the whole `Config`.
- **Catalog introspection** (discovery tooling, the designer) → the `source` port's
  `SchemaIntrospection`, beside `Catalog` (one column's type) and `CaptureProvisioning` (coverage).
- **Transport, telemetry, signals** → `apps/cli/src/`. The daemon owns none of them.
- **Daemon wiring** → `libs/3-daemon/src/`. It exposes data: `Observer` events and a `Status` handle.

Adding a crate means placing it in that layering first, and picking the release train it belongs to.
Everything else about the change is in `CLAUDE.md`, including the rule that a change making
`CLAUDE.md` wrong fixes it in the same commit.
