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
columns (check to include; click a row to rename it and set its type in the inspector).
Pull in a related table by clicking one of the node's FK suggestions: the verb (`belongs_to`
/ `has_one` / `has_many`, or `many_to_many` through a detected junction) is inferred from
the foreign-key direction, and a new child node appears. Aggregates (`count`/`sum`/`ids`/…),
`geo`, `map`, `custom`, and `constant` are added from the node's **+ field** menu as leaf
fields, not child nodes.

Selecting a node or field opens the **inspector** for the details a node can't show
cleanly — join keys and verb, `order_by`, `limit`, filters, `value_type`/`element_type`,
transforms, soft-delete, nullability. Node positions are remembered in your browser; they
aren't written to the files (the files only hold the document tree).

**Some choices are guided by the source, not free.** A column's nullability comes from
the database, so the **required** toggle follows it: a `NOT NULL` column is required by
default (you may relax it to optional), while a nullable column is optional by default. If
you mark a nullable column **required**, a **default** becomes mandatory — otherwise the
field could be missing from the document — and the designer flags it (a red default field
and a highlighted column) until you set one. A `belongs_to` join follows the same logic
from its foreign-key column: a nullable FK starts optional, a `NOT NULL` FK required. The
designer steers you toward a correct schema but doesn't force it — you can save anyway, or
drop to *Raw YAML* and take full control. A field's **type** is a softer case: the SQL type
only *suggests* a flusso type (`keyword` vs `text` is a real authoring choice), so the
inspector shows the suggestion — with a one-click *use* — only when your pick diverges from it.

### Working faster

- **Undo/redo** (`⌘/Ctrl+Z`, `⇧⌘Z`) over the whole document; an **unsaved-changes** dot
  per index, and a warning before you navigate away with edits.
- **Per-node column tools**: include-all/clear, a filter for wide tables, and collapse a node
  to just its header. Each included column shows its required state at a glance: a **muted `*`**
  = required and aligned with a `NOT NULL` column, an **accent `*`** = required but overriding a
  nullable column, an **`=`** = a default is set, and **nothing** = optional.
- **Jump-to-node** search and shortcuts: `⌘/Ctrl+S` save, `Delete` remove the selected
  node/field, `Esc` deselect; click an edge to select its join.
- **Guidance**: a kind-colour legend, FK tooltips, a one-line grammar hint per field kind,
  and an inspector breadcrumb. A **light/dark** theme toggle and a **language** switch
  (English and Italian; the choice is remembered in your browser).
- **Browse the database**: a *Tables* explorer lists every table with its columns, primary
  keys, and foreign keys (and which tables are junctions) — to explore the schema on its
  own, independent of what's on the canvas.

### Saving, validating, and escape hatches

- **Diff before save**: *Save* shows exactly what would change on disk per file; it writes
  only on confirm.
- **Validate against the DB** highlights the offending fields right on the canvas (with the
  message on hover), not just a list.
- **Editable `flusso.toml`**: the source connection, index entries, and **sinks** (the
  OpenSearch URL, shards, analyzers, …) are all editable; a DB-status chip re-tests the
  connection.
- **Raw-YAML fallback**: when the visual editor can't represent something, switch an index
  to *Raw YAML*, edit the file text directly, and save it verbatim.

## What it gives you

- **Discovery from the live database.** Every table, column, type, primary key, and
  foreign key is read from the configured source — through the source abstraction, so
  any future backend gets the designer for free. Column and table pickers offer what's
  really there; each column carries a suggested flusso type derived from its SQL type.
- **The document, as you build it.** A live preview shows the resulting search document
  tree and the derived OpenSearch mapping, recomputed on every edit from the schema
  alone — the same database-free projection `flusso check` prints. On demand it also
  builds a **sample document** from one real row — exactly what the sink would write —
  so you can sanity-check the schema against actual data. If the table has no rows yet,
  it shows example data synthesized from the declared types instead (marked *example*),
  so you still see the document's shape.
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
