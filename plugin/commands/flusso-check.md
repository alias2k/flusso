---
description: Validate a flusso deployment — run flusso check against the config + schemas and triage any errors.
argument-hint: [path/to/flusso.toml]
---

Validate the flusso deployment at `$1` (find the project's `flusso.toml` if no path is given; ask if
there are several).

1. Run validation. Prefer a live DB, which also checks declared types and nullability against real
   columns and prints publication coverage:
   ```sh
   flusso check --config $1            # or: cargo run -- check --config $1
   flusso check --config $1 --offline  # format and rules only, no DB
   ```

2. If it passes, report the typed mapping summary and stop.

3. If it fails, the error names the cause. Triage with the **flusso-schema** skill, which owns the
   field, join, aggregate and filter rules the message refers to. Propose the fix, apply it once the
   user agrees, then re-run step 1.
