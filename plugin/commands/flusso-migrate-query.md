---
description: Migrate read-side Rust off the removed FlussoDocument / path = "…" form onto FlussoRoot + FlussoFragment.
argument-hint: [path-or-crate]
---

Migrate this project's `flusso-query` code from the old derive form to the current one. Scope: `$1`
if given, else the whole workspace.

Read `${CLAUDE_PLUGIN_ROOT}/skills/flusso-query/migration.md` and follow "Moving off the removed
form" step by step. It is the only copy of the procedure, including the ordering constraint (steps
1-2 are find-and-replace; `cargo check` only becomes useful after them) and what to report at the
end.

Start by finding the work. If the search comes back empty the project is already current, so say so
and stop.
