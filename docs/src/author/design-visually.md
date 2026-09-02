# Design a schema visually

Open the designer on a live database, build an index from its tables on a node-graph canvas, preview the document it produces, and save clean `*.schema.yml` and `flusso.toml` files.

## When to use this

You'd rather pick tables and columns than type the grammar, or you want a sample document from real data before committing to a shape. The designer is a view over the canonical files, not a separate model: what it saves is what `flusso check` and `flusso run` read. Hand-written comments in a schema don't survive a save, so keep comment-heavy schemas out of it.

## Steps

1. **Open it.** Point it at a `flusso.toml`, existing or not. It binds to localhost and opens a browser tab.

   ```sh
   flusso design --config flusso.toml
   ```

   The top bar shows a DB chip: **DB connected** or **DB offline**. Offline, editing and saving still work; column pickers, suggested types, and validation don't.

2. **Create an index.** In the sidebar, **New index**: name it, pick its root table from the catalog, then choose the directory and file name for its schema. The canvas shows one node, the root, listing the table's columns.

3. **Include columns.** Check a column to include it. Each included column shows its required state: a muted `*` means required and the column is `NOT NULL`; an accent `*` means required over a nullable column; `=` means a default is set. Shift-click a checkbox to range-select; the funnel filters wide tables; **include all** / **clear all** do the lot.

4. **Shape a field in the inspector.** Click a column row. The inspector reads source to document: **From the database** states the column, SQL type, and nullability; a bridge line spells out the rule (`NOT NULL → required by default`); **In the document** holds the name, flusso type, transforms, required, and default. Rename chips offer the column as-is, its camelCase, or a join's singular. The type shows a **suggested** chip only when your pick diverges from what the SQL type implies.

5. **Fold a related table in.** On a node, **+ join** lists the foreign keys pointing to and from its table. Pick one and the verb is inferred from the key's direction (`belongs_to`, `has_one`, `has_many`, or `many_to_many` through a detected junction); a child node appears, connected by an edge. Aggregates, `geo`, `map`, `custom`, and `constant` come from **+ field** and land on the node as leaf fields.

6. **Preview.** The **Preview** panel has three tabs. **YAML** is the file that would be written. **Mapping** is the derived OpenSearch mapping, the same `flusso check --offline` prints. **Sample** builds a document from one live row, exactly what the sink would write, or synthesizes example data when the table is empty.

7. **Validate.** **Validate** checks every field's column, type, and nullability against the database and highlights offending fields on the canvas with the message on hover.

8. **Save.** **Save** (or `⌘/Ctrl+S`) opens a review of every file that would change, grouped by folder and tagged as a write, a move, a delete, or a new file. Uncheck anything to leave it alone. Saving is atomic: everything is staged first, so a failure leaves the files untouched.

## Options and variations

- **Edit the deployment too.** The sidebar's **Deployment** panel edits the source connection and TLS, the sinks and their index tuning, the index prefix, `on_error`, and the server addresses.
- **Code mode** swaps the canvas for a YAML editor over the same index, with problems, folding, optional auto-format, and VIM keybindings. Edits sync into the project as you type.
- **The command palette** (`⌘/Ctrl+K`) searches actions, indexes, fields, tables and columns, and settings; on-screen results rank first.
- **Undo and redo** (`⌘/Ctrl+Z`, `⇧⌘Z`) cover the whole document. An unsaved dot marks each dirty index and the app warns before you navigate away.
- **Light or dark, English or Italian**, both remembered in your browser.

The full control list is in [Designer reference](designer-reference.md).

## Related

- [Your first schema](first-schema.md) for the same job by hand.
- [Schema top-level keys](../reference/schema-top-level.md) for what each inspector control writes.
