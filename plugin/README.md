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
| `flusso-query` | Writing read-side Rust with `flusso-query` + `#[derive(FlussoRoot)]` / `#[derive(FlussoFragment)]` — typed query surface, sorting, nested filtering, custom value types, multi-index. Discloses `migration.md`, `options.md`, `maps.md`. |
| `flusso-opensearch` | Understanding the **sink** — `dynamic:strict` ownership, hashed index + alias, generations/reindex, the `flusso_*` analyzers + subfields (which to query), refresh. |
| `flusso-integrate` | Standing flusso up in a project or migrating from a hand-rolled indexer — prerequisites, config, first index, `check`, `run`, `build`. |
| `flusso-internals` | Modifying the flusso Rust codebase — the layering, the seams new code attaches to, and which section of the repo's `CLAUDE.md` answers which task. |

The schema and query skills ship worked `examples/` you can copy from. `flusso-query` also ships
sibling reference files loaded only when the task reaches them.

### Commands

| Command | Does |
| --- | --- |
| `/flusso:expert [question or task]` | **Enter expert mode** — answer questions or drive flusso work, backed by the skills; escalates heavy multi-file work to the `flusso-expert` agent. |
| `/flusso-new-index <name> [table]` | Scaffold a new index: a `*.schema.yml` + its `[[index]]` entry. |
| `/flusso-check [path]` | Run `flusso check` and triage any validation errors. |
| `/flusso-doc-struct <index> [Struct]` | Scaffold a typed `#[derive(FlussoRoot)]` query struct. |
| `/flusso-migrate-query [path]` | Migrate read-side Rust off the removed `FlussoDocument` / `path = "…"` form onto `FlussoRoot` + `FlussoFragment`. |

### Agent

`flusso-expert` — a subagent with the full mental model, for multi-file flusso tasks (designing a schema, planning a migration, query-side Rust, substrate debugging, codebase changes). Its knowledge base is the skills above, which it **reads** from `${CLAUDE_PLUGIN_ROOT}/skills/` (agents can't invoke skills), with the repo docs and the `flusso` binary as fallback/ground truth. `/flusso:expert` delegates to it when a task spans several files.

### Hooks — auto-validation & query lint

Two `PostToolUse` hooks run **after any edit** (`Edit`/`Write`/`MultiEdit`):

**`hooks/flusso_validate.py`** — on a `*.schema.yml` or `flusso.toml` edit, finds the owning `flusso.toml` and runs `flusso check`, so a broken schema is caught and fed back in the same turn instead of at `flusso run` time.

- **Online first, offline fallback.** It validates against the live database (catching type/nullability mismatches against real columns); if the DB isn't reachable it silently falls back to offline structural validation rather than nagging about connections.
- **Silent on success**, speaks only when validation fails. Unrelated edits, files outside a flusso project, or a missing runner all exit quietly.
- **Runner resolution:** `$FLUSSO_CHECK_CMD` (a full command prefix) → `flusso` on `PATH` → `cargo run --quiet --` from the nearest Cargo workspace (repo-dev mode).

**`hooks/flusso_query_lint.py`** — on a `.rs` edit, flags the one query anti-pattern a compiler can't: a `Keyword`/`Text` **string-path handle** (`Keyword::at("…")` / `Text::<Root>::at("…")`) in a file that also uses `#[derive(FlussoRoot)]`. There every schema field already has a generated `Type::field()` handle, so a string path bypasses the compile-time mapping check — the classic escape-hatch mistake. High precision: a file with no derive (hand-written handles) is never flagged; the typed fix is fed back for a same-turn correction. Style only — it never blocks editing.

Both require `python3` (only to parse the hook's stdin JSON; the validation/lint logic is local) and are **silent on success**.

> Plugin hooks run a shell command on your machine; review `hooks/flusso_validate.py` and `hooks/flusso_query_lint.py` before installing, as you would any hook.

## Design notes

`ARCHITECTURE.md` is the contract: one corpus with three consumers, which file owns which meaning,
the self-containment rule for external installs, and the standard any change must meet
(`writing-for-agents`, not `docs/STYLE.md`). Read it before editing anything under `plugin/`.
