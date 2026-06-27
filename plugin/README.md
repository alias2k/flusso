# flusso plugin

A Claude Code plugin that teaches an agent to work with [flusso](https://github.com/alias2k/flusso) — declarative Postgres → OpenSearch sync. It covers the whole stack: **authoring** config/schemas, the **Postgres** source and **OpenSearch** sink substrate, **querying** the index from Rust, **integrating/migrating** a deployment, and **contributing** to the flusso codebase.

Type **`/flusso:expert`** to drop into an expert that answers "how does X work?" and drives real work (new index → query it, update an existing one), backed by the knowledge skills below.

## Install

```
/plugin marketplace add alias2k/flusso
/plugin install flusso@flusso
```

(Or, locally from a checkout: `/plugin marketplace add /path/to/flusso`.)

## What's inside

### Skills (model-invoked, loaded on demand) — the knowledge corpus

One module per domain, layered the way flusso is (source → bridge → sink). They auto-trigger on a matching question, and they double as the `flusso-expert` agent's reference corpus.

| Skill | Use when |
| --- | --- |
| `flusso-postgres` | Understanding/debugging the **source** — logical replication, the slot, the publication (`manage_publication`), `REPLICA IDENTITY`, relational→join mapping, privileges. |
| `flusso-schema` | Creating/editing a `*.schema.yml` or `flusso.toml` — type-first fields, joins, aggregates, geo, filters, soft-delete, validation. Points to `flusso design` (the visual, DB-aware editor) for a no-grammar path. |
| `flusso-query` | Writing read-side Rust with `flusso-query` + `#[derive(FlussoDocument)]` — typed query surface, nested filtering, custom value types, multi-index. |
| `flusso-opensearch` | Understanding the **sink** — `dynamic:strict` ownership, hashed index + alias, generations/reindex, the `flusso_*` analyzers + subfields (which to query), refresh. |
| `flusso-integrate` | Standing flusso up in a project or migrating from a hand-rolled indexer — prerequisites, config, first index, `check`, `run`, `build`. |
| `flusso-internals` | Modifying the flusso Rust codebase — crate layering, the sync pipeline, engine invariants, the query derive, the strict lints, CI order. |

Each schema/query skill ships worked `examples/` you can copy from.

### Commands

| Command | Does |
| --- | --- |
| `/flusso:expert [question or task]` | **Enter expert mode** — answer questions or drive flusso work, backed by the skills; escalates heavy multi-file work to the `flusso-expert` agent. |
| `/flusso-new-index <name> [table]` | Scaffold a new index: a `*.schema.yml` + its `[[index]]` entry. |
| `/flusso-check [path]` | Run `flusso check` and triage any validation errors. |
| `/flusso-doc-struct <index> [Struct]` | Scaffold a typed `#[derive(FlussoDocument)]` query struct. |

### Agent

`flusso-expert` — a subagent with the full mental model, for multi-file flusso tasks (designing a schema, planning a migration, query-side Rust, substrate debugging, codebase changes). Its knowledge base is the skills above, which it **reads** from `${CLAUDE_PLUGIN_ROOT}/skills/` (agents can't invoke skills), with the repo docs and the `flusso` binary as fallback/ground truth. `/flusso:expert` delegates to it when a task spans several files.

### Hooks — auto-validation & query lint

Two `PostToolUse` hooks run **after any edit** (`Edit`/`Write`/`MultiEdit`):

**`hooks/flusso_validate.py`** — on a `*.schema.yml` or `flusso.toml` edit, finds the owning `flusso.toml` and runs `flusso check`, so a broken schema is caught and fed back in the same turn instead of at `flusso run` time.

- **Online first, offline fallback.** It validates against the live database (catching type/nullability mismatches against real columns); if the DB isn't reachable it silently falls back to offline structural validation rather than nagging about connections.
- **Silent on success**, speaks only when validation fails. Unrelated edits, files outside a flusso project, or a missing runner all exit quietly.
- **Runner resolution:** `$FLUSSO_CHECK_CMD` (a full command prefix) → `flusso` on `PATH` → `cargo run --quiet --` from the nearest Cargo workspace (repo-dev mode).

**`hooks/flusso_query_lint.py`** — on a `.rs` edit, flags the one query anti-pattern a compiler can't: a `Keyword`/`Text` **string-path handle** (`Keyword::at("…")` / `Text::<Root>::at("…")`) in a file that also uses `#[derive(FlussoDocument)]`. There every schema field already has a generated `Type::field()` handle, so a string path bypasses the compile-time mapping check — the classic escape-hatch mistake. High precision: a file with no derive (hand-written handles) is never flagged; the typed fix is fed back for a same-turn correction. Style only — it never blocks editing.

Both require `python3` (only to parse the hook's stdin JSON; the validation/lint logic is local) and are **silent on success**.

> Plugin hooks run a shell command on your machine; review `hooks/flusso_validate.py` and `hooks/flusso_query_lint.py` before installing, as you would any hook.

## Design notes

- **One corpus, three consumers.** The skills are the single knowledge source. They auto-trigger for the dev (ambient "how does X work?"), they're the body `/flusso:expert` routes through, and they're what the `flusso-expert` agent reads (`${CLAUDE_PLUGIN_ROOT}/skills/`) since agents can't invoke skills. No duplicated knowledge — `/flusso:expert` and the agent are thin layers over the same files.
- **Substrate is scoped, not copied.** `flusso-postgres` and `flusso-opensearch` carry only the *flusso-relevant slice* of each system and link out to the official docs for the rest — flusso's own behavior is documented in full; Postgres/OpenSearch are not re-manualed.
- **Self-contained.** Skills point at `flusso schema index|config` to generate live JSON Schemas for editor validation, and at the repo docs (the `docs/src/guides/` manual — `schema-authoring.md`, `configuration.md`, `deploying.md` — plus `apps/query/README.md` and `CLAUDE.md`) as the source of truth — they don't bundle stale copies, and they assume **none** of the repo's `dev/` paths.
- **Validation-first.** Every authoring/config workflow ends in `flusso check`; codebase work ends by matching CI order under the strict workspace lints.
