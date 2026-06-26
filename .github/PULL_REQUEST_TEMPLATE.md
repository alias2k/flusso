<!--
Thanks for contributing to flusso! Please fill out the sections below.
See CONTRIBUTING.md for the dev loop and the full quality gate.
-->

## What & why

<!-- What does this change do, and why? Keep it focused. -->

## Related issues

<!-- e.g. "Closes #123". Remove if none. -->

## Quality gate

I ran the same gate CI runs (`just ci`, plus the extra CI-only steps):

- [ ] `cargo fmt --all --check` — formatted
- [ ] `cargo clippy --workspace` — no warnings (workspace lints are deny-level)
- [ ] `cargo check --workspace --all-targets` — benches & examples compile
- [ ] `cargo nextest run --run-ignored all` — tests pass (incl. Postgres e2e)
- [ ] `cargo test --doc` — doctests pass
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items` — docs build, intra-doc links resolve

## Housekeeping

- [ ] Updated [`CLAUDE.md`](../CLAUDE.md) if this added/moved a crate, changed a command or test workflow, altered the config/schema format, or introduced an engine invariant
- [ ] Added or updated tests for the change (and kept the named engine-invariant tests green if the sync loop was touched)
- [ ] Updated the relevant docs (`README.md`, the `docs/` manual, `apps/query/README.md`) if behavior or config changed

## Notes for reviewers

<!-- Anything that needs context: trade-offs, follow-ups, areas to scrutinize. Remove if none. -->
