---
description: Scaffold a new flusso index — a *.schema.yml plus its [[index]] entry in flusso.toml.
argument-hint: <index-name> [root-table]
---

Scaffold a new flusso index named `$1` (root table: `$2`, defaulting to `$1` if omitted).

Follow the **flusso-schema** skill for the field syntax. Do this:

1. Locate the project's `flusso.toml` (ask if there are several / none).
2. Create `$1.schema.yml` next to it, starting from this skeleton, then fill in fields by inspecting the root table's columns (ask the user for the column list or a `\d <table>` dump if no DB access):

   ```yaml
   # yaml-language-server: $schema=https://raw.githubusercontent.com/alias2k/flusso/main/libs/2-schema/1-index-yaml/schemas/index.schema.yml
   version: 1
   table: $2
   primary_key: id
   fields:
     - integer: id
     # add fields here — keyword/text/identifier/enum/number/date, joins, aggregates
   ```

3. Add the `[[index]]` entry to `flusso.toml`:

   ```toml
   [[index]]
   name = "$1"
   schema = "$1.schema.yml"
   enabled = true
   ```

4. Run `flusso check --config flusso.toml` (add `--offline` if no DB is reachable) and fix any errors before finishing.

Report what was created and the next command to run. Then, unless the user already said otherwise, **offer the next step**: if a `Cargo.toml` is present, ask whether to generate the Rust query side for `$1` (`/flusso-doc-struct` + the **flusso-query** skill); if this index replaces an existing search/indexer, ask whether to switch that implementation over. One question, then act.
