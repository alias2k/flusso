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
| scalar field | **From the database** (column, SQL type, `NOT NULL`/nullable) · bridge rule · **In the document** (rename chips, type with description and *suggested* chip (a `[]` column suggests its element's type), **Array** (ticked when the field is included from a `[]` column; switching it drops the default, which becomes a list; see [array](../reference/field-types.md#array)), `lowercase`/`trim`, **Auto subfields** on `text`/`keyword`/`uuid` and **Indexed** on `json` (each with a hint naming what turning it off loses; changing the type drops the one it no longer takes), required, default) · **Advanced · index tuning** drawer (`options` as key/JSON-value pairs with quick-adds) · **Filters** drawer where the kind takes them |
| enum field | plus **Variants (ordered)** with add, move up/down, remove |
| aggregate | **operation** (switching keeps the table, key, filters and Via hops; the column and `value_type`/`element_type` reset), related table, **input** (a column picked from the table, or an expression checked as you type, with the position of a mistake and a warning for a name the table lacks; it reaches the preview once it parses), `value_type`/`element_type` (over an expression, only the numeric types; a non-numeric one becomes `double`), key, `distinct` + junction filters (over a junction), filters, **Via** hops (table, `foreign_key`, filters, `on` pairs each; **Add a hop**), **On** pairs against the parent or the last hop (only with a `foreign_key` key; switching to a junction drops them, and so does adding, removing, or retargeting the last hop, since their `local` columns then name another table) |
| geo | lat and lon columns, or a single json column |
| map | column (json/jsonb), `values` |
| custom | postgres types (comma-separated), opensearch type, **Array** |
| constant | value (JSON); no source block |
| script | type (the leaf types a script may return), required, **Returns an array**, **Variants** on `enum`, **Auto subfields** on `text`/`keyword`/`uuid`, the **Script** editor (compile errors from the live preview listed under it and underlined in it); no source block. See [Script fields](../reference/scripts.md) |
| every field | **Emitted** (off makes it input-only: fetched for scripts, never in the document or the mapping; see [emit](../reference/scripts.md#emit)). The canvas marks an input-only row with ⊘ and a broken script in red |

The inspector follows the selected field, not its position: adding, removing or moving another field, on the canvas or in Code mode, and undo or redo leave it on the same field. When the selected field itself is removed, the selection clears. Code mode tells fields apart by name: one edit there that both renames the selected field and adds or removes a field above it can clear the selection or move it to the field that took its place.

An option's key is renamed while it is typed. A key that is empty or already used by another option of the field stays in the box, marked with the reason; the field keeps the last free key typed, and leaving the box shows that key again. Typing `boost` while another option is called `boost` therefore leaves the option named `boos`.

Two rules are steered, not forced. A `NOT NULL` column is required by default and may be relaxed. A nullable column marked required demands a default and is flagged (red default field, highlighted column) until one is set; you can still save. A type that's a sharp change from the source column (a `text` over an `integer`) raises a banner with **Fix all** / **Ignore**.

## Deployment panel

Source, stream, and sinks are edited from each adapter's own declaration: a kind picker (one button per registered adapter of that port), the required options and secrets inline (a secret is a literal or an `env var` reference; a connection can also be a `parts` table), and every option with a default folded into an **options** drawer that counts what is set. The stream sits under the source as a drawer and is omitted from the file when left at its defaults. Indexes: name, schema file path (a subfolder creates directories on save), enabled, `on_error`, duplicate, remove, and a **per-sink options** drawer under each row with one form per sink that takes [per-index options](../reference/index-and-on-error.md#sinks) (an OpenSearch sink's `index_settings`). A sink name is read the way flusso reads it, trimmed and lowercased: typing `primaryC` names the sink `primaryc`, and the box shows `primaryc` once you leave it. A name another sink has once read that way (`AUDIT` while `audit` exists), or one flusso rejects (the rules an index name follows, under **Name refused** below), stays in the box, marked with the reason, and is never applied; **Save** is refused while it shows, and leaving the box puts the sink's name back. A new sink takes the first free name of `sink1`, `sink2`, …. Renaming a sink carries its per-index options along; removing it, or switching it to a kind that takes none, drops them. Plus the index prefix and the server addresses. A config whose options its adapter rejects fails **Validate** and **Test connection** with the adapter's message before any connection is attempted.

What an index row does to its schema file on **Save**. Names are compared the way flusso reads them, trimmed and lowercased, so `Orders` and `orders ` are both `orders`. Paths are compared the way the filesystem resolves them, symlinks included, so `./orders.schema.yml`, and `sub/orders.schema.yml` with `sub` a link to `.`, are both `orders.schema.yml`. A schema path is saved with `./`, doubled and trailing slashes dropped.

- **Add**: **+ index** opens the **New index** wizard, as the sidebar's **New index** does: an index can't be saved until it has a schema.
- **Remove**: the file stays on disk; Save doesn't delete it. Delete it yourself if you no longer want it. An index added later under the same path writes its own schema over it, in the designer's own form, and the review flags that the file is replaced.
- **Rename**: the name applies when you leave the box (or press Enter). A file at the old name's default path, `<old name>.schema.yml`, then moves to `<new name>.schema.yml`, unless another row already uses that path. Any other path stays where it is.
- **Name refused**: a name another index already has, or one flusso rejects (anything but lowercase `a`–`z`, digits and `_`, a leading digit, more than 63 characters), stays in the box, marked with the reason, and is never applied (the **New index** wizard refuses it too). **Save** is refused while it shows, naming the row; leaving the box puts the row's name back.
- **Shared file**: two rows may point at one file while their schemas are the same (see [what a save refuses](#what-a-save-refuses)). A row moved off a shared file leaves it in place for the other row.

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

Every changed file, in a folder tree: **New**, write, **Move** (rename or relocate, with "Moved from"). A warning marks any path resolving outside the config directory, a file written over one that isn't that index's own (such as one a removed index left behind), and any file whose layout a save can't keep (see below). Filter, toggle one or all, then **Write N files**. Below the list: **Validating…**, then **All checks passed**, **Database offline**, or the issue count. Unified, split, old, and new diff views.

Unchecked files are left as they are on disk. `flusso.toml` and the schema files that only fit the new one, those of a new, renamed or moved index, are marked with a link and go together: unchecking one unchecks all of them. Everything else can be left out on its own.

Meaning is preserved: the written file parses back to the same schema and mapping, which the preview self-checks on every edit. Values (`constant`, `default`, `options`) are written as plain YAML: `1.5`, `true`, `null`, and a string quoted only where it would read as something else (`'123'`, `'null'`). Files from earlier designer releases carry tags such as `!String hi`; they load unchanged. One of those, `!Decimal '1.5'`, has always read as the string `"1.5"`, so a number saved that way is a string until you change it back. Moves are real: the old file is removed and emptied folders pruned, within the rules [below](#what-a-save-refuses).

### What a save refuses

After any save, every index in the `flusso.toml` on disk has a file of its own, holding its own schema. The server checks every file the save writes against the `flusso.toml` that will be on disk after it: the new one, or the current one when it is left out. A file left out isn't written, so it isn't checked. Which index on disk each index is comes from the ids the designer sends, else from the names. The server refuses, and writes nothing, when:

- two indexes have one name;
- a schema path is the config file, a folder, or another index's path in other letter case (one file on macOS and Windows);
- a file would be written for an index that config doesn't declare, or anywhere but that index's schema path;
- a file another index uses would be written, unless that index writes it too with the same content;
- a move's old file isn't the same index's file in the `flusso.toml` on disk;
- a new, renamed or moved index's file is written without `flusso.toml`;
- the new `flusso.toml` points an index at a file the save doesn't write, unless it is that index's own file on disk (two indexes keep sharing one only if they already did), or a file no index used that reads as a schema, for a new index. An index added with no schema file is refused, by the designer before the review.

A move removes its old file only when no index uses it after the save and it lies in the config directory; otherwise the old file stays. No other file is ever removed.

Every check runs before anything is written. A save that fails anyway (permissions, a full disk) is undone: the project is left exactly as it was, with no temporary or backup file behind. A file that is replaced keeps its permissions. Only if putting a file back fails too does the error name the backup files that still hold the previous content.

### When a file changed since the page loaded

A page remembers each file it loaded (`flusso.toml` and every schema file) by a digest of its bytes, and after each of its own saves, the digest of what that save wrote. Another designer's save, another tab, a text editor, `git checkout`: anything that changes a file after the page loaded it changes its digest, including only its line endings or a byte-order mark. (`flusso run` writes only `flusso.lock`, which the designer doesn't read.)

`flusso.toml` is compared first. Every check of a save reads it, so while it is not as the page loaded it (another page renamed an index, say), nothing is written, even when the save leaves it out. The review then lists only `flusso.toml`, marked: its diff is what this page would write to it, against the file on disk now. The page's schema edits aren't listed; to copy one out, open its index in Code mode.

Then the review compares every other file against the file on disk now. A file the save would write, move or remove that is not as the page loaded it is marked, with what happened to it: changed, deleted, created, or present though the page never loaded it. It is not written. A file the save doesn't touch may change freely.

The save compares again under the project's lock, so a file changed between the review and Write refuses the save, names the files, and writes nothing; the page then opens the review again. It looks once more right before it replaces or removes each file, after everything else is ready: a file changed by then stops the save the same way, and whatever the save already put in place is undone.

What another program (an editor, `git checkout`) writes during the save is never undone or overwritten afterwards: a file it rewrote after the save put it in place keeps its bytes, whether the save then goes through (the project is as saved, plus that write) or is undone (as before, plus that write), and the refusal names every such file. The one write left to chance lands in the instant between the save's last look at a file and its replacement. A program that rewrites the file in place then writes into the save's backup of it (a hard link to the old file): the file holds what the save wrote, and the save keeps that backup and names it, so the bytes are still there to check. A program that replaces the file (writes a new one and renames it over) in that instant loses its write. Nothing short of that program taking the designer's lock closes that instant.

A page served by another version of the designer (one left open across an upgrade) can't save at all: the server asks it to reload.

The ways out, each offered in the review:

- **Reload the page**: it loads what is on disk. Unsaved edits are lost, so the browser asks first.
- **Copy the edit out first**: the right side of each diff is what the page would write; copy it, reload, and apply it again.
- **Overwrite anyway**, per file, after a confirm: the review is computed again with that file taken as it is on disk now, and lists every file Write then writes; overwriting `flusso.toml` plans the whole save against the one on disk, so the page's schema edits show up then. The file is written with the right side of its diff (comments and formatting are kept as for any save); whatever of the change on disk the right side doesn't show is lost. If the file changes again before Write, the save is refused.

There is no merge.

### When a save is cut short

A save lands whole or not at all, even when the designer is killed or the machine loses power in the middle of it. Before its first write, the save records every step in `.flusso-save.journal` beside `flusso.toml`, and it flushes each file to disk before putting it in place. The next time a designer opens the project, or diffs or saves it, it finds the journal and settles the save before reading anything:

- **rolled back**, when not every file was in place: each file is as it was before the save;
- **completed**, when every file was: each file is as the save wrote it, and only the backups are deleted.

Either way, a file holding neither its text from before the save nor the text the save wrote (or deleted) was written by something else, during the save or after it was cut short: it is kept as it is, and the notice names it. Rolling back, its text from before the save stays in its backup (`.<name>.flusso-save.<…>.bak`, beside it), which the notice also names, for you to copy back or delete; completing, the backups go. A backup holding the kept file's own bytes (a hard link the edit went through) is deleted. So the project is as before the save, or after it, plus those writes: never part one and part the other.

The designer says which, with the files involved, in a notice on the page and on its log. After a rollback a diff or save goes on, since the files are as the page loaded them. A completion changes the files the save wrote, so a page that loaded the project before it can't save over them ([above](#when-a-file-changed-since-the-page-loaded)); its next diff or save in that designer also says, once, that the save was completed, whichever file it writes. Two designers open on one project never touch each other's save: a save holds a lock (`.flusso-save.lock`, removed afterwards except on Windows) for as long as it runs, and a designer waits up to five seconds for it.

#### Whose save is recovered

A journal is a file in the project, so anyone who can put a file there (a repository, an archive, a shared folder) could write one, or bring a copy of one your designer really wrote. So the designer acts on a journal **once, for you, in the folder it was written in**. When a save starts, it records the save in your own state folder, before the journal appears: the digest of the journal's exact bytes and the canonical path of `flusso.toml`'s folder. Once every file is in place, it marks that record committed: that mark, not the journal's file name, says whether recovery rolls the save back or finishes it. The record goes when the save is settled (cleaned up, undone, or recovered).

| Platform | State folder |
| --- | --- |
| Linux and other Unix | `$XDG_STATE_HOME/flusso/design-saves/`, else `~/.local/state/flusso/design-saves/` |
| macOS | `~/Library/Application Support/io.github.alias2k.flusso/design-saves/` |
| Windows | `%APPDATA%\alias2k\flusso\data\design-saves\` |

The folder is private to you (mode 0700, each record 0600). One that others can read or write, or that another user owns, is not used.

- **Recovered**: a save your designer was killed in the middle of, opened again by you, on this machine, in the same folder, once. A schema file the save wrote outside that folder (`../shared/x.schema.yml`, which a save allows with a warning) is recovered too.
- **Only reported**: every other journal. One from another machine or user, or that came with the project. A project copied or moved after a cut-short save (open the original folder to recover it there). A save made while the state folder could not be used, or reopened after its record was pruned. A journal of a save that was settled already, found again. One that is damaged, larger than any save writes, or names a file no save of the project writes, and `.flusso-save.journal` and `.flusso-save.done` both there. The steps a report gives name only schema files (`.yml`, `.yaml`) and the config, never a hidden file or a file of another kind; any other path a journal names is shown as written and left alone. The first way out it offers changes no file: delete only the journal and reload.

Deleting the state folder only means the saves cut short until then are reported instead of recovered. Where it can't be used (no home folder, a read-only one, wrong permissions or owner), saves still work and still undo themselves when they fail, but a save the designer is killed in the middle of is only reported. The designer logs this once. When it starts, the designer prunes records older than 180 days, and records whose project folder is missing once they are older than seven days (a folder moved away and back, or on a drive that is not mounted, keeps its record until then).

Acting on your own save, recovery still checks every file against the text the journal records from before and after the save: it puts a file back, or removes one the save created, only when the file holds one of those texts, and it deletes a temporary file or backup only when it holds the text the save gave it. A committed save is finished whatever its files hold by then: a file another program wrote since keeps its bytes and is named in the notice.

#### When recovery stops

When recovery can't settle a save, the designer changes nothing and shows, instead of the project, what stops it and what to do.

Every step the page lists can be followed literally, in order, and ends in the state it names. A backup or temporary file is named as holding a text only when it holds that text right now, checked by digest: a backup hard-linked to a file that was then edited in place (`>>`, or an editor that keeps hard links) changed with it, and the page says no copy of that text is left.

**A journal it does not act on.** The heading says whether this designer wrote it (another folder, damaged, not a save), has no record of it, or can't tell (no usable state folder). The page gives why, then each file the journal names and what it holds now: its text from before the save, the text the save wrote, or neither, with its backup and temporary file when they hold the save's texts. The report only looks inside `flusso.toml`'s folder: a file outside it is shown as the journal writes it, marked *outside the project, not checked*, and is never opened. Then the way out, as numbered steps:

- **to put the project back as it was before the save**: copy each named backup to its file, delete each file the save created, delete every temporary file and backup the save left (wherever it is in the project), remove each folder the save created if it is empty, delete the journal, and reload;
- **or, to keep what the save wrote**: copy each named temporary file to its file, delete each file the save removed, delete the save's temporary files and backups, delete the journal, and reload.

A route is shown only when every file inside the folder can take it. Follow either only for a save of yours: the steps put in the text the journal names, so check each file first (against version control, say). To leave every file as it is instead, delete only the journal and reload; the save's temporary files and backups, listed, stay where they are for you to check and delete.

**A save of yours it can't settle by itself.** The page says which way the designer was taking it:

- **rolling back**, when not every file was in place: every file goes back to its text from before the save;
- **finishing**, when every file was: each keeps the text the save wrote, and only the backups go.

Then, per file:

| Problem | Rolling back | Finishing |
| --- | --- | --- |
| a file still holding the text the save wrote, with its backup gone or no longer holding the text from before | put the file's text from before the save back yourself (from version control, say) and reload | does not happen: finishing needs no backup |
| a file that can't be read, or a step that failed | fix the cause and reload: recovery carries on where it stopped | the same |

A file changed since the save stopped doesn't stop recovery: it is kept ([above](#when-a-save-is-cut-short)). To keep a file whose backup is gone as it is instead (holding the text the save wrote), follow the page's numbered steps: they put every other file back by hand, delete what the save left (the folders it created too, when empty) and the journal last; the project is then as it was before the save, except that file.

A link, a folder or anything but a plain file at `.flusso-save.lock` stops the designer from locking the project; it names the file, and never opens what the link points at. Delete it and reload.

`*.flusso-save.*` files (the journal, the lock, and each save's `*.flusso-save.*.tmp` and `.bak` files) are a save in progress or what one left: they don't belong in version control. Ignore them, in `.gitignore`:

```gitignore
.flusso-save.*
.*.flusso-save.*
```

`*.flusso-save.*` files that no journal names (left by an older designer, by hand, or kept by a recovery because they held other text) are listed in the notice and never touched: those in `flusso.toml`'s folder, in the folders of its schema files, and in every folder a recovered save wrote in. The state folder lives outside the project and is never committed.

### What a save keeps

A save writes the edit into the file as it stands (in Code mode, into the YAML you typed), for `*.schema.yml` and `flusso.toml` alike. Saving after a one-field edit changes only that field's lines.

- **Untouched parts stay byte for byte**: comments, blank lines, key order, quoting, spacing (`x . y = 1` in `flusso.toml` too), flow lists (`[lowercase, trim]`), settings written out at their default (`required: false`), and a byte order mark at the start of the file. A setting the file leaves out stays out, unless the field's type changes.
- **What changed is written the way a new file is.** A changed value is rewritten on its own lines, keeping the comment at the end of its first line and the comments after it. An added field or setting is written after the one before it, without comments; inside an inline table (`{ a = 1 }`), spaced like the entries already there.
- **Line endings stay.** A file with CRLF (Windows) line endings stays CRLF on every line, including the lines a save adds or rewrites, and so does a file the designer has to write in its own form. In a file with mixed endings, each line a save leaves alone keeps its ending, wherever it moves, and a line it adds or rewrites ends the way most of the file's lines do (LF on a tie); a line of `flusso.toml` edited in place keeps its own. A very large `*.schema.yml` with mixed endings (a few thousand lines) is written in its dominant ending throughout. A multi-line string in `flusso.toml` keeps the line breaks it holds, including those in the whitespace a line-ending backslash swallows. A CRLF `flusso.toml` that holds the character U+E00D is written in the designer's own form instead, with the notice (in LF when a value holds it); so is a `flusso.toml` that holds U+E00E or U+E00F and spaces a dotted key unlike an earlier one with the same start (`x . z` after `x.y`).
- **Names are matched the way flusso reads them.** `[sinks.Primary]` is the sink `primary`: editing, renaming or removing it changes that table where it stands, and it keeps its spelling while it keeps its name. The same goes for its `[index.sinks.<name>]` tables and an `[[index]]` entry's `name`. A file that spells one sink twice (`[sinks.Primary]` and `[sinks.primary]`) means whichever comes last, so the save writes it in the designer's own form, with the notice below.

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
