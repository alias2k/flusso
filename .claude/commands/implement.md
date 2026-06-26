---
description: Implement a GitHub issue end-to-end — branch, logical commits, CI checks, open PR
---

Implement GitHub issue **#$ARGUMENTS** from start to finish.

## 1. Read the issue

- `gh issue view $ARGUMENTS` to get the title, body, and any discussion.
- Make sure you understand the **problem**. The issue describes the problem, **not** the
  solution — so you do not yet know how to solve it. Do not assume.

## 2. Grill on the solution — HIGH intensity

The issue deliberately contains no chosen solution. Before any code, interrogate the user hard
to pin down the solution. This is the most important step — be relentless, not polite:

- Propose 2–3 concrete approaches (use the issue's "Possible solutions" as a starting point if
  present) with explicit trade-offs, and make the user choose.
- Push on every gap: edge cases, failure modes, backwards/forwards compatibility, performance,
  the engine/at-least-once invariants, crate layering, public API surface, migration/rollout,
  what's explicitly *out* of scope.
- Challenge the user's first answer. If something smells wrong, say so and argue it. Surface
  hidden assumptions and force a decision on each. Prefer `AskUserQuestion` for crisp forks.
- Do NOT stop grilling until the solution is fully specified and unambiguous — you should be able
  to state exactly what you'll build, where, and why, with no open "it depends". Only then move on.

## 3. Branch

- Never work on `main`. Create a branch named `<type>/<slug>`, where `<type>` matches the change
  (`feat`, `fix`, `docs`, `chore`, `refactor`, …) and `<slug>` is a short kebab-case summary
  derived from the issue (e.g. `fix/query-log-search-body`).
- `git checkout -b <type>/<slug>`.

## 4. Implement with logical commits

- Do the work in meaningful chunks; commit each coherent unit as you go (not one giant commit).
- **Run `cargo fmt --all` before *every* commit** and stage the result, so no commit lands
  unformatted (the verify step's `cargo fmt --all --check` then has nothing to flag).
- **Conventional Commits** style, matching this repo's history: `fix(query): …`, `feat(engine): …`,
  `chore(fmt)`, etc.
- **Do NOT add a `Co-Authored-By` / AI-attribution trailer** to commit messages.
- Keep `CLAUDE.md` current in the *same* change if you alter crate layout, commands, engine
  invariants, lint policy, or config/schema format (per its "Keeping this file current" rule).

## 5. Update the documentation — code + every README, fully

Bring **all** docs up to date so nothing lags the change. Do this **before** the plugin — docs are
the source of truth the plugin's skills teach from.

- **In-code docs**: `///` on public items and `//!` module headers for anything you added or
  changed; keep doctests compiling (`cargo test --doc` runs in verify).
- **The mdBook manual** under `docs/src/` — the canonical user docs. Update the owning chapter:
  `guides/schema-authoring.md` (schema keys), `guides/configuration.md` (`flusso.toml`/env vars),
  `guides/deploying.md` (Docker/ship), `guides/querying.md` (query side), plus
  `getting-started.md`/`introduction.md`/`SUMMARY.md` if scope/structure changed.
- **Every README**: the root `README.md`, the per-crate `README.md`s (each is its
  crates.io/docs.rs landing via `#![doc = include_str!("../README.md")]`), `libs/README.md`
  (crate-layering map), and `apps/query/README.md` (the full query manual).
- Update the doc that **owns** the content; don't duplicate or resurrect deleted root `.md`s.
- Don't forget `CLAUDE.md` itself (its "Keeping this file current" rule) if layout/commands/
  invariants/format changed.

## 6. Update the flusso Claude plugin

With the docs settled, bring the repo's own Claude plugin under `plugin/` in lockstep:

- `plugin/skills/*/SKILL.md` (`flusso-schema`, `flusso-query`, `flusso-postgres`,
  `flusso-opensearch`, `flusso-internals`, `flusso-integrate`) and their `examples/`.
- `plugin/agents/flusso-expert.md`, `plugin/commands/*.md`, `plugin/hooks/` (the validate/lint
  hooks), `plugin/.claude-plugin/plugin.json`, `plugin/README.md`.
- If the change altered schema/config format, the query derive, engine behavior, CLI commands, or
  anything a skill teaches, update the affected skill/agent/example so the plugin can't teach
  something now wrong. New capability worth surfacing → add/extend a skill.

## 7. Verify — full CI parity before opening the PR

Run, in order, and fix anything that fails before proceeding:

```sh
cargo fmt --all --check
cargo clippy --workspace
cargo check --workspace --all-targets
cargo nextest run --run-ignored all   # needs a running Docker daemon for the e2e tests
cargo test --doc
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items
```

If Docker isn't available for the `--run-ignored all` step, say so explicitly rather than
silently skipping it.

## 8. Open the PR

- Push the branch: `git push -u origin <branch>`.
- Open the PR directly (no confirmation step needed):
  ```sh
  gh pr create --title "<title>" --body "<body>"
  ```
- PR body must:
  - **Describe the solution** — what changed and why, the approach taken, anything reviewers
    should know (trade-offs, follow-ups).
  - Link and close the issue: include `Closes #$ARGUMENTS`.
  - End with:
    ```
    🤖 Generated with [Claude Code](https://claude.com/claude-code)
    ```
- Report the PR URL back.
