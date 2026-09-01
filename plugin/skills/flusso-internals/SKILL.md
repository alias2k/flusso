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
0-core  →  1-{queue,sources,sinks}  →  2-{engine,schema}  →  3-daemon  →  apps
```

A crate depends only on lower-numbered layers. Keep it acyclic. Three placements carry rationale
worth knowing before you move code: the file parsers sit at layer 2 so a layer-1 backend **cannot**
reach config-loading machinery or see the assembled `Config`; `libs/3-daemon` assembles the pipeline
while naming no concrete backend; and the published `flusso-query` has **zero** flusso dependencies,
so its live e2e lives in the unpublished `dev/query-e2e` crate. Keep it that way.

## The seams — where new things attach

- **A source or sink backend** → a match arm in `apps/cli/src/backends.rs` plus its crate. The
  engine and daemon stay untouched. The source builder takes a `SourceSpec`, not the whole `Config`.
- **Catalog introspection** (discovery tooling, the designer) → `sources-core`'s
  `SchemaIntrospection`, beside `Catalog` (one column's type) and `CaptureProvisioning` (coverage).
- **Transport, telemetry, signals** → `apps/cli/src/`. The daemon owns none of them.
- **Daemon wiring** → `libs/3-daemon/src/`. It exposes data: `Observer` events and a `Status` handle.

Adding a crate means placing it in that layering first, and picking the release train it belongs to.
Everything else about the change is in `CLAUDE.md`, including the rule that a change making
`CLAUDE.md` wrong fixes it in the same commit.
