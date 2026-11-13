# Designer reference

Every panel, control, and shortcut in `flusso design`, and how saving maps to files.

## Launch

`flusso design` and its three flags are in the [CLI reference](../reference/cli.md#design). It binds to localhost and is absent from the server-only build (`--no-default-features`, which the registry image uses).

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
| **+ field** | add an aggregate (`count` `exists` `sum` `avg` `min` `max` `ids`), `geo`, `map`, `custom`, `constant`, or `script` leaf |
| `+ column name, Enter` | add a column by name when the catalog is offline |
| double-click a node title | rename the field in place |
| collapse / expand | one node; the toolbar does all |
| click an edge | select its join |
| toolbar | zoom in/out, fit to view, lock/unlock layout, reset layout, minimap, collapse/expand all |

Node positions and collapsed nodes live in the browser, per index, not in the files. They follow the node, so removing, adding, or duplicating a field elsewhere leaves them alone, both while you edit and across a reload, including when the schema file changed on disk in between. Nodes are matched by field name within their parent; a node added since the last save, or renamed outside the designer, starts fresh after a reload. A node whose join is missing a required key, or has an `on` pair missing a side or listed twice, is flagged until it's set in the inspector.

Column badges: muted `*` = required and `NOT NULL`; accent `*` = required over a nullable column; `=` = has a default; nothing = optional. A node header counts fields that disagree with the database after a validate, each matched by its path from the root, so a namesake elsewhere isn't flagged. The root's header has a second count for the index-level keys (`table`, `primary_key`, `filters`, `soft_delete`), with the messages on hover. A join's header counts the findings on the join itself, such as a missing table or a table without a single-column primary key, with the messages on hover.

## Inspector

| Selection | Blocks |
| --- | --- |
| index root | root table, schema, root filters, soft-delete |
| object group | name; its fields are edited on the node |
| join | verb, related table, key (`column` for `belongs_to`, `foreign_key` for `has_one`/`has_many`, junction table + keys + junction filters for `many_to_many`), required (to-one), **Indexed** (off stores the rows without indexing them; see [Joins](../reference/joins.md#indexed)), **On** pairs (a column of the parent table = a column of the related table; **Add a pair**; not on `many_to_many`, and switching to it drops them; see [Joins](../reference/joins.md#on)), filters, the FK nullability note |
| scalar field | **From the database** (column, SQL type, `NOT NULL`/nullable) · bridge rule · **In the document** (rename chips, type with description and *suggested* chip, `lowercase`/`trim`, **Auto subfields** on `text`/`keyword`/`uuid` and **Indexed** on `json` (each with a hint naming what turning it off loses; changing the type drops the one it no longer takes), required, default) · **Advanced · index tuning** drawer (`options` as key/JSON-value pairs with quick-adds) · **Filters** drawer where the kind takes them |
| enum field | plus **Variants (ordered)** with add, move up/down, remove |
| aggregate | **operation** (switching keeps the table, key, filters and Via hops; the column and `value_type`/`element_type` reset), related table, **input** (a column picked from the table, or an expression checked as you type, with the position of a mistake and a warning for a name the table lacks; it reaches the preview once it parses), `value_type`/`element_type` (over an expression, only the numeric types; a non-numeric one becomes `double`), key, `distinct` + junction filters (over a junction), filters, **Via** hops (table, `foreign_key`, filters, `on` pairs each; **Add a hop**), **On** pairs against the parent or the last hop (only with a `foreign_key` key; switching to a junction drops them, and so does adding, removing, or retargeting the last hop, since their `local` columns then name another table) |
| geo | lat and lon columns, or a single json column |
| map | column (json/jsonb), `values` |
| custom | postgres types (comma-separated), opensearch type |
| constant | value (JSON); no source block |
| script | type (the leaf types a script may return), required, **Returns an array**, **Variants** on `enum`, **Auto subfields** on `text`/`keyword`/`uuid`, the **Script** editor (compile errors from the live preview listed under it and underlined in it); no source block. See [Script fields](../reference/scripts.md) |
| every field | **Emitted** (off makes it input-only: fetched for scripts, never in the document or the mapping; see [emit](../reference/scripts.md#emit)). The canvas marks an input-only row with ⊘ and a broken script in red |

The inspector follows the selected field, not its position: adding, removing or moving another field, on the canvas or in Code mode, and undo or redo leave it on the same field. When the selected field itself is removed, the selection clears. Code mode tells fields apart by name: one edit there that both renames the selected field and adds or removes a field above it can clear the selection or move it to the field that took its place.

An option's key is renamed while it is typed. A key that is empty or already used by another option of the field stays in the box, marked with the reason; the field keeps the last free key typed, and leaving the box shows that key again. Typing `boost` while another option is called `boost` therefore leaves the option named `boos`.

Two rules are steered, not forced. A `NOT NULL` column is required by default and may be relaxed. A nullable column marked required demands a default and is flagged (red default field, highlighted column) until one is set; you can still save. A type that's a sharp change from the source column (a `text` over an `integer`) raises a banner with **Fix all** / **Ignore**.

## Deployment panel

Source, stream, and sinks are edited from each adapter's own declaration: a kind picker (one button per registered adapter of that port), the required options and secrets inline (a secret is a literal or an `env var` reference; a connection can also be a `parts` table), and every option with a default folded into an **options** drawer that counts what is set. The stream sits under the source as a drawer and is omitted from the file when left at its defaults. Indexes: name, schema file path (a subfolder creates directories on save), enabled, `on_error`, duplicate, remove, and a **per-sink options** drawer under each row with one form per sink that takes [per-index options](../reference/index-and-on-error.md#sinks) (an OpenSearch sink's `index_settings`). Renaming a sink carries its per-index options along; removing it, or switching it to a kind that takes none, drops them. Plus the index prefix and the server addresses. A config whose options its adapter rejects fails **Validate** and **Test connection** with the adapter's message before any connection is attempted.

What an index row does to its schema file on **Save**. Names are compared the way flusso reads them, trimmed and lowercased, so `Orders` and `orders ` are both `orders`. Paths are compared the way the filesystem resolves them, symlinks included, so `./orders.schema.yml`, and `sub/orders.schema.yml` with `sub` a link to `.`, are both `orders.schema.yml`. A schema path is saved with `./`, doubled and trailing slashes dropped.

- **Remove**: the file stays on disk. Save neither deletes nor rewrites it; delete it yourself if you no longer want it.
- **Rename**: the name applies when you leave the box (or press Enter). A file at the old name's default path, `<old name>.schema.yml`, then moves to `<new name>.schema.yml`, unless another row already uses that path. Any other path stays where it is.
- **Name refused**: a name another index already has, or one flusso rejects (anything but lowercase `a`–`z`, digits and `_`, a leading digit, more than 63 characters), stays in the box, marked with the reason, and is never applied (the **New index** wizard refuses it too). **Save** is refused while it shows, naming the row; leaving the box puts the row's name back.
- **Shared file**: two rows may point at one file while their schemas are the same. If they differ, Save refuses and names both rows. A row moved off a shared file leaves it in place for the other row.
- **Config file**: a schema path that is the config file itself refuses the save.

Each row keeps its own schema through any mix of these, undo and redo included: one index's schema is never written to another index's file. The server checks this again on every save, see [what a save refuses](#what-a-save-refuses).

## Code mode

A YAML editor over the selected index, opened on the file as a save would write it: its comments included. Problems panel with a count, valid-YAML indicator, search, block folding, cursor position, **Format**, **Auto-format** on focus loss when valid, VIM keybindings. Edits sync into the project as you type; Save reviews and writes as usual, and comments typed here are saved too, even after later edits on the canvas.

Code mode shows the whole schema, so it opens only when no field is [incomplete](#incomplete-fields): otherwise the canvas stays and a message names each piece to finish or remove (a field with a blank name by its position, `#3`). A schema the server refuses for another reason, such as a malformed column name, keeps the canvas too, with the server's message. A Code URL reached with Back or Forward is refused the same way, and the history entry is replaced, so Back still moves past it.

Opening Code mode and leaving it again changes nothing: the YAML it opens with is never applied back, only what you type.

- **Undo, Redo, Reset** change the project, and the YAML is rewritten from the result. Undo and redo stay one step each, whichever mode made the edit. A comment typed here is a step too: undoing it takes it out of what Save writes, so with every step undone there is nothing to save. If the result has an incomplete field, the canvas comes back with the message above; nothing is dropped.
- **`⌘/Ctrl+Z` inside the editor** is the editor's own undo. It changes the YAML, and the project follows like any other edit. It starts afresh when Undo, Redo or Reset rewrites the YAML, so it can't bring back what they took out.
- **Leaving** (the toggle, another index, Deployment, the command palette, Back or Forward) applies YAML you typed that isn't in the project yet. **Tables** is not leaving: the index stays in Code mode behind it, and picking that index in the sidebar comes back to its YAML as you left it, even YAML that doesn't parse.
- **Save** applies it first too, so the review shows exactly what gets written. It is enabled as soon as you type.
- **YAML that doesn't parse** holds a move away with a message, since its latest edits aren't in the project. Fix it, or pick **Discard** in the message to drop them. Switching again only repeats the message. Save is refused until the YAML parses.

## Preview

| Tab | Shows |
| --- | --- |
| YAML | the file a save would write, minus anything [left out](#incomplete-fields) |
| Mapping | the derived OpenSearch mapping, database-free; an opted-out field shows `subfields: false` or `indexed: false` |
| Sample | a document built from one live row via the same path the sink uses, scripts run and input-only fields stripped; **Row id** picks the row (blank: the first); a failing script shows its field, message and line with a caret over the row as built; what the scripts printed; synthesized example data (marked *example*) when the table is empty; **Build sample** / **refresh** |

The document tree marks a stored-only field (`indexed: false`) with a *not indexed* chip.

### Incomplete fields

The canvas flags an incomplete field and the live preview leaves it out, so a half-built field doesn't error the panel. A warning above the preview tabs lists what's left out, and the YAML and Mapping tabs show the schema without it. The document keeps it: it comes back in the preview once it's complete.

A field is incomplete when:

- its name is blank or not a valid field name;
- its column, a `geo` coordinate, or a script's source is blank;
- a join's `table`, `primary_key`, or key (`column`, `foreign_key`, or the `through` table and keys) is blank;
- an aggregate's table, key, or column is blank, or a `sum`, `min`, or `max` has no `value_type`;
- a `via` hop's table or key is blank.

A new field of any kind from **+ field** or **+ join** never errors the preview: it is either complete as added, or incomplete until filled in. A new `constant` starts at `null`.

A half-written or duplicated `on` pair flags its field, but only the pair is left out of the preview. A filter, `order_by` entry, or soft-delete marker with a blank column, and a soft-delete `field` that isn't a valid field name, are left out too.

A preview that fails anyway shows an error banner, cleared by the next preview that succeeds or by switching index.

## Save review

Every changed file, in a folder tree: **New**, write, **Move** (rename or relocate, with "Moved from"). A warning marks any path resolving outside the config directory, a move that replaces a file already at its destination (such as one a removed index left behind), and any file whose layout a save can't keep (see below). Filter, toggle one or all, then **Write N files**. Below the list: **Validating…**, then **All checks passed**, **Database offline**, or the issue count. Unified, split, old, and new diff views.

Unchecked files are left as they are on disk. `flusso.toml` and the schema files that only fit the new one, those of a new, renamed or moved index, are marked with a link and go together: unchecking one unchecks all of them. Everything else can be left out on its own. So after any save, every index in `flusso.toml` on disk still has its file.

Meaning is preserved: the written file parses back to the same schema and mapping, which the preview self-checks on every edit. Values (`constant`, `default`, `options`) are written as plain YAML: `1.5`, `true`, `null`, and a string quoted only where it would read as something else (`'123'`, `'null'`). Files from earlier designer releases carry tags such as `!String hi`; they load unchanged. One of those, `!Decimal '1.5'`, has always read as the string `"1.5"`, so a number saved that way is a string until you change it back. Moves are real: the old file is removed and emptied folders pruned, within the rules [above](#what-a-save-refuses).

### What a save refuses

The server checks every save against the `flusso.toml` that will be on disk after it: the new one, or the current one when it is left out. It refuses, and writes nothing, when:

- two indexes have one name;
- a schema path is the config file, a folder, or another index's path in other letter case (one file on macOS and Windows);
- a file would be written for an index that config doesn't declare, or anywhere but that index's schema path;
- a file another index uses would be written, unless that index writes it too with the same content;
- a move would take the config file as its source;
- a new, renamed or moved index's file is written without `flusso.toml`, or `flusso.toml` without it;
- the new `flusso.toml` names a schema file that won't exist after the save.

A move removes its old file only when that is the same index's file in the `flusso.toml` on disk, no index uses it after the save, and it lies in the config directory; otherwise the old file stays and the move is a plain write. No other file is ever removed.

Every check runs before anything is written. The files are then staged next to their targets and swapped in, `flusso.toml` last, and old files are removed after that. If a write still fails (permissions, a full disk), the files swapped before it keep their new content, the rest and `flusso.toml` keep the old, and no temporary file is left behind.

### What a save keeps

A save writes the edit into the file as it stands (in Code mode, into the YAML you typed), for `*.schema.yml` and `flusso.toml` alike. Saving after a one-field edit changes only that field's lines.

- **Untouched parts stay byte for byte**: comments, blank lines, key order, quoting, flow lists (`[lowercase, trim]`), and settings written out at their default (`required: false`). A setting the file leaves out stays out, unless the field's type changes.
- **What changed is written the way a new file is.** A changed value is rewritten on its own lines, keeping the comment at the end of its first line and the comments after it. An added field or setting is written after the one before it, without comments.

#### Which comment belongs to what

A comment stays with what it describes, through a rename, a type change and a move, and is deleted with it. It never moves to something else.

| Where the comment is | It belongs to |
|---|---|
| before the first key or table | the file: it stays first |
| directly above a field, a key, a list item or a table, with no blank line between | that field, key, item or table |
| a blank line away from what follows (a section heading, a commented-out block) | nothing: it stays where it is, before whatever follows it |
| at the end of a line | that line |
| after a list's last item, at the item's dash column or further left | the list |
| after a field's last line, indented deeper than its dash | the field |
| in `flusso.toml`, directly below a table's last key, with no blank line between (at the end of the file too) | that table |

So deleting a join deletes its own comments and everything inside it, while the heading a blank line above it stays, before the next field. Deleting a sink keeps the commented-out options the sink above ends with, and deleting the last `[[index]]` entry deletes the comment directly below it at the end of the file.

The designer tells the save which field each field was, which `[[index]]` entry each entry was and which sink each sink was, by an id it keeps for the session, so renaming one field and deleting its neighbour in one save can't hand the neighbour's comments to it. A renamed sink is renamed where it stands, in `[sinks]` and in every `[index.sinks.<name>]`: its place, key order and comments stay. When the file changed on disk since the designer read it, fields, entries and sinks are matched by name instead: a renamed one then counts as removed and added, and its comments as lost.

A field moved into another list (a different join or object, which only Code mode can do) is written there without its comments; the review counts them.

#### What the review reports

- **Comments that could not be kept.** A comment inside a value that had to be rewritten (a flow list `[…]` with a comment in it, an item the save can't follow line by line), on a field moved into another list, or on a field, entry or sink matched by name that may have been renamed, is dropped. Matched by name, a removed one and a renamed one look the same, so both count. The review marks the file and says above its diff how many comments it loses.
- **A file written in the designer's own form.** A file whose layout the save can't follow (a top level written as a flow mapping `{…}`, for instance), or whose result wouldn't read back as exactly the schema being saved, is written in the designer's own form instead, without its comments. The review marks it and says so above its diff.
- **Windows line endings.** A `flusso.toml` saved with CRLF line endings is written back with LF, comments kept. A CRLF `*.schema.yml` keeps its line endings through a value edit; renaming a field writes it in the designer's own form, and the review says so.

Both show before anything is written. A file the designer creates is written in its own form.

### Typed values

A `constant`, a `default` and each `options` value are typed as JSON: text in double quotes (`"like this"`), `true`, `null`, lists, objects, numbers. Numbers are a little looser than JSON: a leading `+`, leading zeros, and a bare point on either side (`5.`, `.5`) are accepted and read as the number they spell. The designer reads what you type the way the saved file will be read, so the preview and the content hash (the physical index name) match what flusso deploys from that file. A value is read once typing pauses, or at once when the box loses focus; Save, Code mode, undo and redo wait for it, and **Save** is enabled while one is being read. It goes into the field it was typed for, even when you have selected, renamed or moved another field by the time it is read; a field deleted in the meantime gets nothing. It goes into the schema only once it is accepted; a refused one keeps the field red with the reason underneath and leaves the previous value in the schema. **Reset** discards a value still being read.

When the designer cannot be reached to read a value, the box keeps what you typed, says it could not be checked, and the schema keeps the previous value. **Save** tries it again; if it still gets no answer within a few seconds, it writes nothing and names each field (`orders.items.quantity.default`) whose value could not be checked. A value still being read when an option is removed or renamed goes with it, and leaving the page while one is being read asks first.

| Typed | Stored as | A `constant` maps as |
|---|---|---|
| an integer that fits a signed 64-bit integer (`5`, `-12`, `9007199254740993`) | that integer | `long` |
| a larger integer the file holds exactly (`9223372036854775808`) | a decimal | `double` |
| a fraction or an exponent (`1.5`, `5.`, `.5`, `1e3`, `1e-7`) | a decimal, as the file reads a float: `1e3` is `1000.0` | `double` |
| `+5`, `007`, `-0`, spaces around the number | the same number: `5`, `7`, `0` | as above |

When the saved file spells the value differently from what you typed, a *Saved as* note under the box shows the value as the file writes it (`1e3` → `1000.0`, `+5` → `5`, `[1, 1e3]` → `[1, 1000.0]`, ` 5 ` → `5`); when it is written as typed (`1e-7`, `1.5`), there is no note. Refused, with a message, and never rounded:

- a number with more digits than the file keeps, such as `0.1000000000000000000001`, or most integers past 2⁶⁴ (`18446744073709551616`);
- a number too large or too small to be a number in the file, such as `1.5e300` or `1e-30`;
- lists and objects nested more than 32 levels deep;
- a list or an object as a column `default`, which takes a single value.

To keep a number as written, quote it: `"1.5e300"` is stored as text.

Integers past 9007199254740991 need a browser with `JSON.rawJSON` (current Chrome, Edge, Firefox and Safari). Without it, the browser rounds such an integer as it reads it: typing one is refused, and when a loaded file holds one, **Save** refuses to write and names each field that holds it, so the file never gets another number. Open the designer in a current browser to edit it.

**Format** and **Auto-format** in Code mode change only layout: every scalar keeps its text (`9007199254740993`, `0.1000000000000000000001`, `007`, `0x10`, `.inf`, `'007'`, `yes`), and comments stay.

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
