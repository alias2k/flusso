---
description: Validate a flusso deployment — run flusso check against the config + schemas and triage any errors.
argument-hint: [path/to/flusso.toml]
---

Validate the flusso deployment at `$1` (find the project's `flusso.toml` if no path is given; ask if there are several).

1. Run validation. Prefer a live DB (it also checks declared types/nullability against real columns); fall back to `--offline` if no database is reachable:
   ```sh
   flusso check --config $1            # or: cargo run -- check --config $1
   flusso check --config $1 --offline  # format + rules only, no DB
   ```
   (If `flusso` isn't on PATH, use `cargo run -- check …` from the repo.)

2. If it passes, report the typed mapping summary and stop.

3. If it fails, the error names the cause. Triage with the **flusso-schema** skill — common ones:
   - exactly one type key per field; only allowed siblings for that type;
   - a join's key sibling must match its verb (`column`/`foreign_key`/`through`);
   - `sum`/`min`/`max` need `column` + `value_type`; aggregates need exactly one of `foreign_key`/`through`;
   - `between` takes exactly two values; `in`/`not_in` take a list;
   - `primary_key` must be set on the root once any relation exists.

   Propose the fix, apply it if the user agrees, then re-run step 1.
