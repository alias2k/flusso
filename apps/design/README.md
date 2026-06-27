# flusso-design

The visual, database-aware schema designer behind `flusso design` — a local web UI that
authors a `*.schema.yml` and its `flusso.toml` wiring without hand-writing the type-first
grammar.

It introspects the configured source *through the source abstraction* (`sources-core`'s
`SchemaIntrospection`), so column and table pickers reflect the real database and any
future source backend gets a designer for free. The committed files stay the source of
truth — the UI is a round-tripping view over them, not a separate model.

## What it does

- **Introspect** — enumerate the live relational catalog (tables, columns + suggested
  flusso types, primary keys, foreign keys, junction candidates).
- **Edit** — the full grammar: scalars, `map`/`custom`/`geo`/`object`, the four join
  verbs, the six aggregate ops, filters, `order_by`, soft-delete, plus the `flusso.toml`
  source/sink/index wiring.
- **Preview** — derive the search document tree and OpenSearch mapping from the schema
  alone (database-free), recomputed on every edit.
- **Validate** — check the edited schemas against the live database and report
  disagreements inline.
- **Save** — canonical regeneration back to `*.schema.yml` / `flusso.toml`. The
  round-trip preserves meaning (it re-parses to the same mapping), not comments or
  incidental formatting.

## Layout

- `codegen` — model → type-first YAML / TOML.
- `preview` — schema → mapping + document tree.
- `api` — the JSON operations (load / introspect / preview / validate / save).
- `server` — the axum router; serves the API and the embedded SPA.
- `frontend/` — the Vite + React + TypeScript UI, built to `dist/` and embedded via
  `rust-embed`. Rebuild with `npm --prefix frontend ci && npm --prefix frontend run build`.

## Use it

```sh
flusso design --config flusso.toml
# flusso designer running at http://127.0.0.1:7700
```

See the [Designing schemas visually](https://alias2k.github.io/flusso/guides/designing.html)
guide for the full walk-through.
