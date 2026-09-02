# Designer reference

Every panel, control, and shortcut in `flusso design`, and how saving maps to files.

## Launch

| Flag | Env | Default | Meaning |
| --- | --- | --- | --- |
| `--config` | `FLUSSO_CONFIG` | `flusso.toml` | Deployment to edit. May not exist yet. |
| `--address` | `FLUSSO_DESIGN_ADDRESS` | `127.0.0.1:7700` | Bind address. Localhost only by default. |
| `--no-open` | `FLUSSO_DESIGN_NO_OPEN` | off | Print the URL instead of opening a browser. |

Absent from the server-only build (`cargo build -p flusso-cli --no-default-features` and the registry image).

## Layout

| Region | Holds |
| --- | --- |
| Top bar | sidebar toggle, **Tables** (catalog browser), **Visual** / **Code** mode switch, undo, redo, theme, DB status chip with re-test, **Preview**, **Validate**, **Save**, **Reset**, language |
| Sidebar | **Deployment** panel; the **Indexes** list with a dirty dot per index and an on/off state; **New index**; the legend of node kinds and field types |
| Canvas | the node graph for the selected index: root, `object` groups, and joins as nodes; relations as edges |
| Inspector | the selected node's or field's details |
| Preview | **YAML** / **Mapping** / **Sample** tabs |

## Canvas

| Control | Does |
| --- | --- |
| checkbox on a column | include or exclude it; Shift-click selects a range, Ctrl/Cmd-click toggles one without moving the anchor |
| filter box | narrows a wide table's column list |
| include all / clear all | the whole table |
| **+ join** | search the foreign keys to and from this table; pick one to add a child node with the inferred verb |
| **+ field** | add an aggregate (`count` `sum` `avg` `min` `max` `ids`), `geo`, `map`, `custom`, or `constant` leaf |
| `+ column name, Enter` | add a column by name when the catalog is offline |
| double-click a node title | rename the field in place |
| collapse / expand | one node; the toolbar does all |
| click an edge | select its join |
| toolbar | zoom in/out, fit to view, lock/unlock layout, reset layout, minimap, collapse/expand all |

Node positions live in the browser, not in the files. A node whose join is missing a required key is flagged until it's set in the inspector.

Column badges: muted `*` = required and `NOT NULL`; accent `*` = required over a nullable column; `=` = has a default; nothing = optional. A node header counts fields that disagree with the database after a validate.

## Inspector

| Selection | Blocks |
| --- | --- |
| index root | root table, schema, root filters, soft-delete |
| object group | name; its fields are edited on the node |
| join | verb, related table, key (`column` for `belongs_to`, `foreign_key` for `has_one`/`has_many`, junction table + keys for `many_to_many`), required (to-one), filters, the FK nullability note |
| scalar field | **From the database** (column, SQL type, `NOT NULL`/nullable) · bridge rule · **In the document** (rename chips, type with description and *suggested* chip, `lowercase`/`trim`, required, default) · **Advanced · index tuning** drawer (`options` as key/JSON-value pairs with quick-adds) · **Filters** drawer where the kind takes them |
| enum field | plus **Variants (ordered)** with add, move up/down, remove |
| aggregate | related table, column to aggregate, `value_type`/`element_type`, key, filters |
| geo | lat and lon columns, or a single json column |
| map | column (json/jsonb), `values` |
| custom | postgres types (comma-separated), opensearch type |
| constant | value (JSON); no source block |

Two rules are steered, not forced. A `NOT NULL` column is required by default and may be relaxed. A nullable column marked required demands a default and is flagged (red default field, highlighted column) until one is set; you can still save. A type that's a sharp change from the source column (a `text` over an `integer`) raises a banner with **Fix all** / **Ignore**.

## Deployment panel

Source: connection as URL, env var, or host/port/user/password/database parts; TLS mode with the `require`-verifies-nothing warning. Sinks: per-sink connection and index tuning. Indexes: name, schema file path (a subfolder creates directories on save), enabled, `on_error`, duplicate, remove (the schema file stays on disk). Plus the index prefix and the server addresses.

## Code mode

A YAML editor over the selected index. Problems panel with a count, valid-YAML indicator, search, block folding, cursor position, **Format**, **Auto-format** on focus loss when valid, VIM keybindings. Edits sync into the project as you type; Save reviews and writes as usual.

## Preview

| Tab | Shows |
| --- | --- |
| YAML | the canonical file that would be written |
| Mapping | the derived OpenSearch mapping, database-free |
| Sample | a document built from one live row via the same path the sink uses; synthesized example data (marked *example*) when the table is empty; **Build sample** / **refresh** |

Incomplete pieces (a field with a blank name mid-edit) are pruned from the live preview payload only, so a half-typed field doesn't error the panel.

## Save review

Every changed file, in a folder tree: **New**, write, **Move** (rename or relocate, with "Moved from"), **Delete** (a removed index). A warning marks any path resolving outside the config directory. Filter, toggle one or all, then **Write N files**. Below the list: **Validating…**, then **All checks passed**, **Database offline**, or the issue count. Unified, split, old, and new diff views.

Saving is canonical regeneration: meaning is preserved (the written file parses back to the same schema and mapping, which the preview self-checks on every edit), comments and incidental formatting are not. Moves are real: the old file is removed and emptied folders pruned. The save is atomic.

## Catalog browser

**Tables** opens every table with its columns, primary key, foreign keys, "Referenced by", and a junction badge, independent of the canvas. Filter tables and columns by name.

## Shortcuts

| Keys | Does |
| --- | --- |
| `⌘/Ctrl+K` | command palette: actions, indexes, fields, tables and columns, settings, legend; Tab completes; recent entries |
| `⌘/Ctrl+S` | save |
| `⌘/Ctrl+Z`, `⇧⌘/Ctrl+Z` | undo, redo (outside a text field) |
| `Delete` / `Backspace` | remove the selected node or field (outside a text field) |
| `Esc` | deselect |
| Shift-click / Ctrl-click a column | range select / toggle one |
| double-click a node title | rename |

## Offline

With the database unreachable the top bar shows an offline banner. Discovery and validation are off; editing, preview, and save keep working. Re-test from the DB chip.
