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

## Design notes

- **Self-contained.** Skills point at `flusso schema index|config` to generate live JSON Schemas for editor validation, and at the repo docs (`SCHEMA.md`, `SOURCES_AND_SINKS.md`, `CONFIG.md`, `CLIENT.md`, `CLAUDE.md`) as the source of truth — they don't bundle stale copies, and they assume **none** of the repo's `dev/` paths.
- **Validation-first.** Every authoring/config workflow ends in `flusso check`; codebase work ends by matching CI order under the strict workspace lints.
