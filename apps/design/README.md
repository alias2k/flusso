# flusso-design

The visual, database-aware schema designer behind `flusso design`: a local web UI that authors `*.schema.yml` and `flusso.toml` without hand-writing the type-first grammar.

It introspects the configured source through `sources-core`'s `SchemaIntrospection`, so column and table pickers reflect the real database and any future source backend gets a designer for free. The committed files stay the source of truth; the UI is a round-tripping view over them.

```sh
flusso design --config flusso.toml
# flusso designer running at http://127.0.0.1:7700
```

## What it does

- **Introspect** the live catalog: tables, columns with suggested flusso types, primary keys, foreign keys, junction candidates.
- **Edit** the full grammar on a node-graph canvas with an inspector: scalars, `map`/`custom`/`geo`/`object`, the four join verbs, the six aggregate ops, filters, `order_by`, soft-delete, plus the `flusso.toml` source/sink/index wiring.
- **Preview** the derived mapping and document tree on every edit, and build a **sample document** from one live row.
- **Validate** the schemas against the database and flag disagreements on the canvas.
- **Save** by canonical regeneration, reviewed as a diff first, atomically.

## Layout

- `codegen`: model to type-first YAML and TOML.
- `preview`: schema to mapping and document tree.
- `api`: the JSON operations, project, adapters (the registered adapter descriptions the forms render from), catalog, test-connection, parse, preview, validate, sample, diff, save.
- `server`: the axum router serving the API and the embedded SPA.
- `frontend/`: Vite + React + TypeScript; React Flow canvas, Tailwind v4 + shadcn. Built to `dist/` and embedded with `rust-embed`; rebuild with `npm --prefix frontend ci && npm --prefix frontend run build`. Localized through `frontend/src/i18n.tsx` with per-language catalogs in `frontend/src/locales/` (English base, Italian), an ICU MessageFormat evaluator, and a CI check that every key exists in every locale.

## Learn more

[Design a schema visually](https://alias2k.github.io/flusso/author/design-visually.html) is the walkthrough; [Designer reference](https://alias2k.github.io/flusso/author/designer-reference.html) lists every panel, control, and shortcut.
