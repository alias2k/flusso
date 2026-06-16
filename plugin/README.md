# flusso plugin

A Claude Code plugin that teaches an agent to work with [flusso](https://github.com/alias2k/flusso) — declarative Postgres → OpenSearch sync. It covers all three sides: **authoring** config/schemas, **integrating/migrating** a deployment, **querying** the index from Rust, and **contributing** to the flusso codebase.

## Install

```
/plugin marketplace add alias2k/flusso
/plugin install flusso@flusso
```

(Or, locally from a checkout: `/plugin marketplace add /path/to/flusso`.)

## What's inside

### Skills (model-invoked, loaded on demand)

| Skill | Use when |
| --- | --- |
| `flusso-schema` | Creating/editing a `*.schema.yml` or `flusso.toml` — type-first fields, joins, aggregates, geo, filters, soft-delete, validation. |
| `flusso-integrate` | Standing flusso up in a project or migrating from a hand-rolled indexer — prerequisites, config, first index, `check`, `run`, `build`. |
| `flusso-query` | Writing read-side Rust with `flusso-query` + `#[derive(FlussoDocument)]` — typed query surface, nested filtering, custom value types, multi-index. |
| `flusso-internals` | Modifying the flusso Rust codebase — crate layering, the sync pipeline, engine invariants, the query derive, the strict lints, CI order. |

Each schema/query skill ships worked `examples/` you can copy from.

### Commands

| Command | Does |
| --- | --- |
| `/flusso-new-index <name> [table]` | Scaffold a new index: a `*.schema.yml` + its `[[index]]` entry. |
| `/flusso-check [path]` | Run `flusso check` and triage any validation errors. |
| `/flusso-doc-struct <index> [Struct]` | Scaffold a typed `#[derive(FlussoDocument)]` query struct. |

### Agent

`flusso-expert` — a subagent with the full mental model, for multi-file flusso tasks (designing a schema, planning a migration, query-side Rust, codebase changes). Delegate to it when a task spans several files.

### Hook — auto-validation

A `PostToolUse` hook (`hooks/flusso_validate.py`) runs **after any edit** to a `*.schema.yml` or `flusso.toml`: it finds the owning `flusso.toml` and runs `flusso check`, so a broken schema is caught and fed back in the same turn instead of at `flusso run` time.

- **Online first, offline fallback.** It validates against the live database (catching type/nullability mismatches against real columns); if the DB isn't reachable it silently falls back to offline structural validation rather than nagging about connections.
- **Silent on success**, speaks only when validation fails. Unrelated edits, files outside a flusso project, or a missing runner all exit quietly.
- **Runner resolution:** `$FLUSSO_CHECK_CMD` (a full command prefix) → `flusso` on `PATH` → `cargo run --quiet --` from the nearest Cargo workspace (repo-dev mode).
- Requires `python3` (used only to parse the hook's stdin JSON and bridge to `flusso check` — the validation itself is flusso's).

> Plugin hooks run a shell command on your machine; review `hooks/flusso_validate.py` before installing, as you would any hook.

## Design notes

- **Self-contained.** Skills point at `flusso schema index|config` to generate live JSON Schemas for editor validation, and at the repo docs (`SCHEMA.md`, `SOURCES_AND_SINKS.md`, `CONFIG.md`, `CLIENT.md`, `CLAUDE.md`) as the source of truth — they don't bundle stale copies, and they assume **none** of the repo's `dev/` paths.
- **Validation-first.** Every authoring/config workflow ends in `flusso check`; codebase work ends by matching CI order under the strict workspace lints.
