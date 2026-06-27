# Designing schemas visually

`flusso design` opens a local, database-aware web UI that authors a `*.schema.yml`
and its `flusso.toml` wiring without hand-writing the type-first grammar — pick tables
and columns from the real database, watch the search document take shape, and save the
files in place.

## What it is

The designer is a view over the canonical files, not a separate model. It reads the
`flusso.toml` and the `*.schema.yml` files it references, lets them be edited on a visual
canvas, and writes them back as clean, deterministic YAML/TOML. The files stay the source
of truth — committing them is committing the design.

```sh
flusso design --config flusso.toml
# flusso designer running at http://127.0.0.1:7700
```

It opens the UI in the default browser on start; `--no-open` (or
`FLUSSO_DESIGN_NO_OPEN`) prints the URL instead — handy on a headless box or over SSH.
`--address` (or `FLUSSO_DESIGN_ADDRESS`) moves the bind address; `--config` (or
`FLUSSO_CONFIG`) picks the deployment to edit. The UI binds to localhost only.

## On the canvas

Each index is a **node graph**. A node is a place in the document — the root, an `object`
group, or a join — and shows the table it draws from plus a checklist of that table's
columns (check to include, rename inline, set the type). Pull in a related table by
clicking one of the node's FK suggestions: the verb (`belongs_to` / `has_one` / `has_many`,
or `many_to_many` through a detected junction) is inferred from the foreign-key direction,
and a new child node appears. Aggregates (`count`/`sum`/`ids`/…), `geo`, `map`, `custom`,
and `constant` are added from the node's **+ field** menu as leaf fields, not child nodes.

Selecting a node or field opens the **inspector** for the details a node can't show
cleanly — join keys and verb, `order_by`, `limit`, filters, `value_type`/`element_type`,
transforms, soft-delete, nullability. Node positions are remembered in your browser; they
aren't written to the files (the files only hold the document tree).

## What it gives you

- **Discovery from the live database.** Every table, column, type, primary key, and
  foreign key is read from the configured source — through the source abstraction, so
  any future backend gets the designer for free. Column and table pickers offer what's
  really there; each column carries a suggested flusso type derived from its SQL type.
- **The document, as you build it.** A live preview shows the resulting search document
  tree and the derived OpenSearch mapping, recomputed on every edit from the schema
  alone — the same database-free projection `flusso check` prints.
- **Validation against the real schema.** *Validate against DB* checks every field's
  column, type, and nullability against the live store and reports disagreements inline —
  the late-bound errors that otherwise surface only at `flusso check` time.
- **The whole grammar.** Scalars, dynamic-key `map`s, `custom` types, `geo` points,
  nested `object`s, all four join verbs (`belongs_to`/`has_one`/`has_many`/
  `many_to_many`), all six aggregate ops (`count`/`sum`/`avg`/`min`/`max`/`ids`),
  filters, `order_by`, and soft-delete — plus the `flusso.toml` source/sink/index wiring.

## How saving works

Saving is **canonical regeneration**: the files are rewritten from the model in a tidy,
deterministic layout. The round-trip preserves *meaning* — what's written parses back to
the same schema and resolves to the same mapping (the preview self-checks this on every
edit) — but **not** hand-written comments or incidental formatting. Author comment-heavy
schemas by hand if that matters; use the designer to get a correct schema fast, then
refine.

## Offline authoring

The designer works without a reachable database — discovery and DB validation degrade to
a banner, while editing, preview, and saving keep working. Connect a database to get the
column pickers, suggested types, and validation back.
